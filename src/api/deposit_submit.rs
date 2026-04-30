use axum::{Json, extract::State};

use crate::{
    AppState,
    api::{errors::ApiError, status::empty_swap_details},
    now_iso8601,
    types::{SubmitDepositTxRequest, SubmitDepositTxResponse, SwapStatus},
};

pub(crate) async fn submit_deposit(
    State(state): State<AppState>,
    Json(request): Json<SubmitDepositTxRequest>,
) -> Result<Json<SubmitDepositTxResponse>, ApiError> {
    state
        .deposit_submissions
        .write()
        .await
        .push(request.clone());

    let key = (request.deposit_address, request.memo);
    let quote_response = state
        .quotes
        .read()
        .await
        .get(&key)
        .cloned()
        .ok_or_else(|| ApiError::bad_request("deposit address not found"))?;

    Ok(Json(SubmitDepositTxResponse {
        correlation_id: quote_response.correlation_id.clone(),
        quote_response,
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

    use crate::{AppState, app};

    #[tokio::test]
    async fn returns_ok_not_not_implemented() {
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
