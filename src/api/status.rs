use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;

use crate::{
    AppState,
    api::errors::ApiError,
    types::{StatusResponse, SwapDetails, SwapStatus},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusQuery {
    deposit_address: String,
    deposit_memo: Option<String>,
}

pub(crate) async fn status(
    State(state): State<AppState>,
    Query(query): Query<StatusQuery>,
) -> Result<Json<StatusResponse>, ApiError> {
    let key = (query.deposit_address, query.deposit_memo);
    let quote_response = state
        .quotes
        .read()
        .await
        .get(&key)
        .cloned()
        .ok_or_else(|| ApiError::not_found("deposit address not found"))?;

    Ok(Json(StatusResponse {
        correlation_id: quote_response.correlation_id.clone(),
        updated_at: quote_response.timestamp.clone(),
        quote_response,
        status: SwapStatus::PendingDeposit,
        swap_details: empty_swap_details(),
    }))
}

pub(crate) fn empty_swap_details() -> SwapDetails {
    SwapDetails {
        intent_hashes: Vec::new(),
        near_tx_hashes: Vec::new(),
        amount_in: None,
        amount_in_formatted: None,
        amount_in_usd: None,
        amount_out: None,
        amount_out_formatted: None,
        amount_out_usd: None,
        slippage: None,
        origin_chain_tx_hashes: Vec::new(),
        destination_chain_tx_hashes: Vec::new(),
        refunded_amount: None,
        refunded_amount_formatted: None,
        refunded_amount_usd: None,
        refund_reason: None,
        deposited_amount: None,
        deposited_amount_formatted: None,
        deposited_amount_usd: None,
        referral: None,
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    use crate::{AppState, app, types::StatusResponse};

    #[tokio::test]
    async fn dry_quote_is_not_persisted() {
        let app = app(AppState::default());
        let quote_response = app
            .clone()
            .oneshot(
                Request::post("/v0/quote")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "dry": true,
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

        assert_eq!(quote_response.status(), StatusCode::OK);

        let body = to_bytes(quote_response.into_body(), usize::MAX)
            .await
            .expect("body");
        let quote: crate::types::QuoteResponse =
            serde_json::from_slice(&body).expect("quote response");
        let would_be_deposit_address = format!("mock-{}", quote.correlation_id);

        let response = app
            .oneshot(
                Request::get(format!(
                    "/v0/status?depositAddress={would_be_deposit_address}"
                ))
                .body(Body::empty())
                .expect("request"),
            )
            .await
            .expect("status response");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn persisted_quote_returns_pending_deposit_status() {
        let app = app(AppState::default());
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
                Request::get(format!("/v0/status?depositAddress={deposit_address}"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("status response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let status: StatusResponse = serde_json::from_slice(&body).expect("status response");

        assert_eq!(status.status, crate::types::SwapStatus::PendingDeposit);
    }

    #[tokio::test]
    async fn unknown_deposit_address_returns_not_found() {
        let app = app(AppState::default());
        let response = app
            .oneshot(
                Request::get("/v0/status?depositAddress=unknown")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
