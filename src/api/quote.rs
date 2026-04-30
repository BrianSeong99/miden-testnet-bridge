use axum::{Json, extract::State};
use uuid::Uuid;

use crate::{
    AppState,
    api::errors::ApiError,
    chains::{
        evm::evm_quote_requires_deposit_address, miden::asset_symbol,
        miden::miden_quote_requires_deposit_address,
        miden_deposit_account::derive_outbound_deposit_account,
    },
    now_iso8601,
    types::{DepositMode, Quote, QuoteRequest, QuoteResponse},
};

pub(crate) async fn quote(
    State(state): State<AppState>,
    Json(request): Json<QuoteRequest>,
) -> Result<Json<QuoteResponse>, ApiError> {
    if request
        .custom_recipient_msg
        .as_deref()
        .is_some_and(|message| !message.is_empty())
    {
        return Err(ApiError::bad_request(
            "customRecipientMsg is not supported for Miden bridge quotes",
        ));
    }

    if request.deposit_mode == Some(DepositMode::Memo) {
        return Err(ApiError::bad_request(
            "depositMode MEMO is not supported for Miden bridge quotes",
        ));
    }

    let correlation_id = Uuid::new_v4();
    let timestamp = now_iso8601();
    let mut deposit_derivation_path = None;
    let mut miden_deposit_artifact = None;
    let deposit_address = if request.dry {
        None
    } else if evm_quote_requires_deposit_address(&request.origin_asset, &request.destination_asset)
    {
        if let Some(evm) = state.evm.as_ref() {
            let (address, derivation_path) = evm
                .derive_deposit_address(correlation_id)
                .await
                .map_err(|error| ApiError::internal(error.to_string()))?;
            deposit_derivation_path = Some(derivation_path);
            Some(address.to_string())
        } else {
            Some(format!("mock-{correlation_id}"))
        }
    } else if miden_quote_requires_deposit_address(&request.origin_asset) {
        let Some(miden_client) = state.miden_client.as_ref() else {
            return Err(ApiError::internal(
                "Miden client is not configured for outbound deposit accounts".to_owned(),
            ));
        };
        let Some(master_seed) = state.miden_master_seed.as_ref() else {
            return Err(ApiError::internal(
                "Miden master seed is not configured for outbound deposit accounts".to_owned(),
            ));
        };
        let (account, _secret_key, init_seed, auth_seed) =
            derive_outbound_deposit_account(master_seed, correlation_id)
                .map_err(|error| ApiError::internal(error.to_string()))?;
        miden_deposit_artifact = Some((
            account.id().to_hex(),
            format!(
                "{}:{}",
                alloy::hex::encode(init_seed),
                alloy::hex::encode(auth_seed)
            ),
        ));
        Some(miden_client.encode_basic_wallet_address(account.id()))
    } else {
        Some(format!("mock-{correlation_id}"))
    };

    let origin_symbol = asset_symbol(&request.origin_asset)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let destination_symbol = asset_symbol(&request.destination_asset)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let price_quote = state
        .pricer
        .quote(origin_symbol, destination_symbol, &request.amount)
        .await
        .map_err(|error| match error {
            crate::core::pricer::PricerError::UnsupportedAssetSymbol(_)
            | crate::core::pricer::PricerError::InvalidAmount(_) => {
                ApiError::bad_request(error.to_string())
            }
            _ => ApiError::internal(error.to_string()),
        })?;

    let response = QuoteResponse {
        correlation_id: correlation_id.to_string(),
        timestamp,
        // TODO: Quote signing lands in a later iteration.
        signature: String::new(),
        quote_request: request.clone(),
        quote: Quote {
            deposit_address: deposit_address.clone(),
            deposit_memo: None,
            amount_in: request.amount.clone(),
            amount_in_formatted: request.amount.clone(),
            amount_in_usd: price_quote.input_usd,
            min_amount_in: request.amount.clone(),
            max_amount_in: None,
            amount_out: price_quote.output_amount.clone(),
            amount_out_formatted: price_quote.output_amount.clone(),
            amount_out_usd: price_quote.output_usd,
            min_amount_out: price_quote.output_amount,
            deadline: (!request.dry).then(|| request.deadline.clone()),
            time_when_inactive: (!request.dry).then(|| request.deadline.clone()),
            time_estimate: 120.0,
            virtual_chain_recipient: request.virtual_chain_recipient.clone(),
            virtual_chain_refund_recipient: request.virtual_chain_refund_recipient.clone(),
            custom_recipient_msg: None,
            refund_fee: None,
        },
    };

    if deposit_address.is_some() {
        state
            .store
            .insert_quote(&response, &request)
            .await
            .map_err(|error| ApiError::internal(error.to_string()))?;
        if let (Some(evm), Some(derivation_path)) = (state.evm.as_ref(), deposit_derivation_path) {
            evm.persist_deposit_derivation_path(correlation_id, &derivation_path)
                .await
                .map_err(|error| ApiError::internal(error.to_string()))?;
        }
        if let Some((account_id, seed_hex)) = miden_deposit_artifact {
            state
                .store
                .set_miden_deposit_account(correlation_id, &account_id, &seed_hex)
                .await
                .map_err(|error| ApiError::internal(error.to_string()))?;
        }
    }

    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use crate::{AppState, app, test_support::memory_state, types::QuoteResponse};

    fn sample_quote_request() -> Value {
        json!({
            "dry": false,
            "depositMode": "SIMPLE",
            "swapType": "EXACT_INPUT",
            "slippageTolerance": 100.0,
            "originAsset": "eth-anvil:eth",
            "depositType": "ORIGIN_CHAIN",
            "destinationAsset": "miden-local:eth",
            "amount": "1000",
            "refundTo": "0xfeed",
            "refundType": "ORIGIN_CHAIN",
            "recipient": "recipient",
            "recipientType": "DESTINATION_CHAIN",
            "deadline": "2026-06-12T00:00:00Z"
        })
    }

    fn with_override(mut value: Value, key: &str, replacement: Value) -> Value {
        value
            .as_object_mut()
            .expect("quote request fixture should be an object")
            .insert(key.to_owned(), replacement);
        value
    }

    #[tokio::test]
    async fn happy_path_returns_quote_response() {
        let app = app(AppState::new(memory_state()));
        let response = app
            .oneshot(
                Request::post("/v0/quote")
                    .header("content-type", "application/json")
                    .body(Body::from(sample_quote_request().to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let quote: QuoteResponse = serde_json::from_slice(&body).expect("quote response");

        assert!(!quote.correlation_id.is_empty());
        assert!(quote.quote.deposit_address.is_some());
    }

    #[tokio::test]
    async fn custom_recipient_msg_returns_bad_request() {
        let app = app(AppState::new(memory_state()));
        let payload = with_override(
            sample_quote_request(),
            "customRecipientMsg",
            Value::String("anything".to_owned()),
        );
        let response = app
            .oneshot(
                Request::post("/v0/quote")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn memo_deposit_mode_returns_bad_request() {
        let app = app(AppState::new(memory_state()));
        let payload = with_override(
            sample_quote_request(),
            "depositMode",
            Value::String("MEMO".to_owned()),
        );
        let response = app
            .oneshot(
                Request::post("/v0/quote")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn dry_quote_omits_deposit_fields() {
        let app = app(AppState::new(memory_state()));
        let payload = with_override(sample_quote_request(), "dry", Value::Bool(true));
        let response = app
            .oneshot(
                Request::post("/v0/quote")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let quote: QuoteResponse = serde_json::from_slice(&body).expect("quote response");

        assert_eq!(quote.quote.deposit_address, None);
        assert_eq!(quote.quote.time_when_inactive, None);
        assert_eq!(quote.quote.deadline, None);
    }

    #[tokio::test]
    async fn bearer_header_is_accepted() {
        let app = app(AppState::new(memory_state()));
        let response = app
            .oneshot(
                Request::post("/v0/quote")
                    .header("content-type", "application/json")
                    .header("authorization", "Bearer xyz")
                    .body(Body::from(sample_quote_request().to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn persisted_quote_calls_state_store() {
        let store = memory_state();
        let app = app(AppState::new(store.clone()));

        let response = app
            .oneshot(
                Request::post("/v0/quote")
                    .header("content-type", "application/json")
                    .body(Body::from(sample_quote_request().to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let quote: QuoteResponse = serde_json::from_slice(&body).expect("quote response");
        let stored = store
            .get_quote_by_deposit(
                quote
                    .quote
                    .deposit_address
                    .as_deref()
                    .expect("deposit address"),
                None,
            )
            .await
            .expect("state query");

        assert!(stored.is_some());
    }
}
