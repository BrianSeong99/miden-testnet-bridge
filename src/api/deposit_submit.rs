use axum::{Json, extract::State};
use serde_json::json;

use crate::{
    AppState,
    api::{errors::ApiError, status::empty_swap_details},
    core::state::TxHashColumn,
    now_iso8601,
    types::{SubmitDepositTxRequest, SubmitDepositTxResponse, SwapStatus},
};

pub(crate) async fn submit_deposit(
    State(state): State<AppState>,
    Json(request): Json<SubmitDepositTxRequest>,
) -> Result<Json<SubmitDepositTxResponse>, ApiError> {
    let quote_record = state
        .store
        .get_quote_by_deposit(&request.deposit_address, request.memo.as_deref())
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?
        .ok_or_else(|| ApiError::bad_request("deposit address not found"))?;
    let correlation_id = quote_record.correlation_id;
    let inserted = state
        .store
        .record_idempotency_key(correlation_id, &request.tx_hash)
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?;

    if inserted {
        state
            .store
            .append_tx_hash(
                correlation_id,
                TxHashColumn::EvmDepositTxHashes,
                &request.tx_hash,
            )
            .await
            .map_err(|error| ApiError::internal(error.to_string()))?;
        state
            .store
            .record_event(
                correlation_id,
                Some(&quote_record.status),
                "KNOWN_DEPOSIT_TX",
                "DEPOSIT_SUBMITTED",
                None,
                Some(json!({
                    "txHash": request.tx_hash,
                    "nearSenderAccount": request.near_sender_account,
                    "memo": request.memo
                })),
            )
            .await
            .map_err(|error| ApiError::internal(error.to_string()))?;
    }

    Ok(Json(SubmitDepositTxResponse {
        correlation_id: correlation_id.to_string(),
        quote_response: quote_record.quote_response,
        status: SwapStatus::KnownDepositTx,
        updated_at: now_iso8601(),
        swap_details: empty_swap_details(),
    }))
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    use crate::{AppState, app, test_support::memory_state};

    #[tokio::test]
    async fn returns_ok_not_not_implemented() {
        let app = app(AppState::new(memory_state()));
        let quote_response = app
            .clone()
            .oneshot(
                Request::post("/v0/quote")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
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
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("quote response");
        let body = to_bytes(quote_response.into_body(), usize::MAX)
            .await
            .expect("body");
        let quote: crate::types::QuoteResponse =
            serde_json::from_slice(&body).expect("quote response");
        let deposit_address = quote.quote.deposit_address.expect("deposit address");

        let response = app
            .oneshot(
                Request::post("/v0/deposit/submit")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "txHash": "0xabc",
                            "depositAddress": deposit_address
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
    }
}
