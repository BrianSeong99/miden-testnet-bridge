use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
};
use uuid::Uuid;

use crate::{
    AppState,
    api::errors::ApiError,
    chains::{
        evm::evm_quote_requires_deposit_address,
        miden::{asset_symbol, miden_quote_requires_deposit_address, parse_account_id},
        miden_bridge_note::BridgeOutDepositMemo,
    },
    now_iso8601,
    types::{DepositMode, Quote, QuoteRequest, QuoteResponse},
};

pub(crate) async fn quote(
    State(state): State<AppState>,
    request: Result<Json<QuoteRequest>, JsonRejection>,
) -> Result<Json<QuoteResponse>, ApiError> {
    let Json(request) = request.map_err(ApiError::from_json_rejection)?;
    Ok(Json(create_quote(&state, request).await?))
}

pub(crate) async fn create_quote(
    state: &AppState,
    request: QuoteRequest,
) -> Result<QuoteResponse, ApiError> {
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

    let mut deposit_derivation_path = None;
    let mut deposit_memo = None;
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
        let Some(bootstrap) = state
            .store
            .get_miden_bootstrap()
            .await
            .map_err(|error| ApiError::internal(error.to_string()))?
        else {
            return Err(ApiError::internal(
                "Miden bootstrap state is missing for bridge-note quote".to_owned(),
            ));
        };
        let bridge_account_id = bootstrap.solver_account_id;
        let memo = BridgeOutDepositMemo::from_quote(
            &request,
            correlation_id,
            &price_quote.output_amount,
            &bridge_account_id,
        )
        .map_err(|error| ApiError::internal(error.to_string()))?;
        tracing::info!(
            %correlation_id,
            origin_asset = %request.origin_asset,
            destination_asset = %request.destination_asset,
            bridge_account_id = %memo.bridge_account_id,
            quote_hash = %memo.storage.quote_hash,
            storage_encoding = %memo.storage_encoding,
            storage_items = memo.storage.storage_items.len(),
            "created Miden BridgeOutV1 quote instruction"
        );
        deposit_memo = Some(
            memo.to_deposit_memo()
                .map_err(|error| ApiError::internal(error.to_string()))?,
        );
        if let Some(miden_client) = state.miden_client.as_ref() {
            let account_id = parse_account_id(&bridge_account_id)
                .map_err(|error| ApiError::internal(error.to_string()))?;
            Some(miden_client.encode_basic_wallet_address(account_id))
        } else {
            Some(bridge_account_id)
        }
    } else {
        Some(format!("mock-{correlation_id}"))
    };

    let response = QuoteResponse {
        correlation_id: correlation_id.to_string(),
        timestamp,
        // TODO: Quote signing lands in a later iteration.
        signature: String::new(),
        quote_request: request.clone(),
        quote: Quote {
            deposit_address: deposit_address.clone(),
            deposit_memo: deposit_memo.clone(),
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
    }

    Ok(response)
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use miden_client::account::{AccountStorageMode, AccountType};
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use crate::{
        AppState, app,
        chains::miden_bridge_note::BridgeOutDepositMemo,
        core::state::MidenBootstrapRecord,
        test_support::{memory_state, test_miden_account_id},
        types::QuoteResponse,
    };

    fn sample_quote_request() -> Value {
        json!({
            "dry": false,
            "depositMode": "SIMPLE",
            "swapType": "EXACT_INPUT",
            "slippageTolerance": 100.0,
            "originAsset": "eth-anvil:eth",
            "depositType": "ORIGIN_CHAIN",
            "destinationAsset": "miden-testnet:eth",
            "amount": "1000",
            "refundTo": "0xfeed",
            "refundType": "ORIGIN_CHAIN",
            "recipient": "recipient",
            "recipientType": "DESTINATION_CHAIN",
            "deadline": "2026-06-12T00:00:00Z"
        })
    }

    fn miden_origin_quote_request() -> Value {
        json!({
            "dry": false,
            "depositMode": "SIMPLE",
            "swapType": "EXACT_INPUT",
            "slippageTolerance": 100.0,
            "originAsset": "miden-testnet:eth",
            "depositType": "ORIGIN_CHAIN",
            "destinationAsset": "eth-anvil:eth",
            "amount": "1000",
            "refundTo": "0xrefund",
            "refundType": "ORIGIN_CHAIN",
            "recipient": "0xrecipient",
            "recipientType": "DESTINATION_CHAIN",
            "deadline": "2026-06-12T00:00:00Z"
        })
    }

    async fn seed_miden_bootstrap(store: &crate::core::state::DynStateStore) {
        let solver_account_id = test_miden_account_id(
            AccountType::RegularAccountUpdatableCode,
            AccountStorageMode::Private,
            0xccdd_eeff,
        )
        .to_hex();
        store
            .upsert_miden_bootstrap(&MidenBootstrapRecord {
                solver_account_id,
                eth_faucet_account_id: "0xeth".to_owned(),
                usdc_faucet_account_id: "0xusdc".to_owned(),
                usdt_faucet_account_id: "0xusdt".to_owned(),
                btc_faucet_account_id: "0xbtc".to_owned(),
            })
            .await
            .expect("seed miden bootstrap");
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
    async fn miden_origin_quote_returns_public_bridge_note_instruction() {
        let store = memory_state();
        seed_miden_bootstrap(&store).await;
        let app = app(AppState::new(store.clone()));

        let response = app
            .oneshot(
                Request::post("/v0/quote")
                    .header("content-type", "application/json")
                    .body(Body::from(miden_origin_quote_request().to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let quote: QuoteResponse = serde_json::from_slice(&body).expect("quote response");
        let bootstrap = store
            .get_miden_bootstrap()
            .await
            .expect("bootstrap")
            .expect("bootstrap row");
        let deposit_address = quote
            .quote
            .deposit_address
            .as_deref()
            .expect("deposit address");
        let deposit_memo = quote.quote.deposit_memo.as_deref().expect("deposit memo");
        let memo: BridgeOutDepositMemo =
            serde_json::from_str(deposit_memo).expect("bridge note memo");

        assert_eq!(deposit_address, bootstrap.solver_account_id);
        assert_eq!(memo.version, "bridge-out-v1");
        assert_eq!(memo.note_type, "PUBLIC");
        assert_eq!(memo.bridge_account_id, bootstrap.solver_account_id);
        assert_eq!(memo.storage.correlation_id, quote.correlation_id);
        assert_eq!(memo.storage.origin_asset, "miden-testnet:eth");
        assert_eq!(memo.storage.destination_asset, "eth-anvil:eth");
        assert_eq!(memo.storage.amount_in, "1000");
        assert_eq!(memo.storage.min_amount_out, "1000");
        assert_eq!(memo.storage.destination_recipient, "0xrecipient");
        assert_eq!(memo.storage.refund_account, "0xrefund");

        let stored = store
            .get_quote_by_deposit(deposit_address, Some(deposit_memo))
            .await
            .expect("state query")
            .expect("stored quote");
        assert_eq!(stored.miden_deposit_account_id, None);
        assert_eq!(stored.miden_deposit_seed_hex, None);

        let tracked = store
            .list_miden_tracked_quotes()
            .await
            .expect("tracked quotes");
        assert_eq!(tracked.len(), 1);
        assert_eq!(tracked[0].deposit_memo.as_deref(), Some(deposit_memo));
        assert_eq!(tracked[0].miden_deposit_account_id, None);
    }

    #[tokio::test]
    async fn invalid_json_returns_spec_shaped_bad_request() {
        let app = app(AppState::new(memory_state()));
        let response = app
            .oneshot(
                Request::post("/v0/quote")
                    .header("content-type", "application/json")
                    .body(Body::from("{"))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let error: crate::types::BadRequestResponse =
            serde_json::from_slice(&body).expect("error response");
        assert!(!error.message.is_empty());
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
