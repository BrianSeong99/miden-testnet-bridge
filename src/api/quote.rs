use axum::{Json, extract::State};
use uuid::Uuid;

use crate::{
    AppState,
    api::errors::ApiError,
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

    let correlation_id = Uuid::new_v4().to_string();
    let timestamp = now_iso8601();
    let deposit_address = (!request.dry).then(|| format!("mock-{correlation_id}"));

    let response = QuoteResponse {
        correlation_id: correlation_id.clone(),
        timestamp,
        // TODO: Quote signing lands in a later iteration.
        signature: String::new(),
        quote_request: request.clone(),
        quote: Quote {
            deposit_address: deposit_address.clone(),
            deposit_memo: None,
            amount_in: request.amount.clone(),
            amount_in_formatted: request.amount.clone(),
            amount_in_usd: "1.0".to_owned(),
            min_amount_in: request.amount.clone(),
            max_amount_in: None,
            amount_out: request.amount.clone(),
            amount_out_formatted: request.amount.clone(),
            amount_out_usd: "1.0".to_owned(),
            min_amount_out: request.amount.clone(),
            deadline: (!request.dry).then(|| request.deadline.clone()),
            time_when_inactive: (!request.dry).then(|| request.deadline.clone()),
            time_estimate: 120.0,
            virtual_chain_recipient: request.virtual_chain_recipient.clone(),
            virtual_chain_refund_recipient: request.virtual_chain_refund_recipient.clone(),
            custom_recipient_msg: None,
            refund_fee: None,
        },
    };

    if let Some(deposit_address) = deposit_address {
        state
            .quotes
            .write()
            .await
            .insert((deposit_address, None), response.clone());
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

    use crate::{AppState, app, types::QuoteResponse};

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
        let app = app(AppState::default());
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
        let app = app(AppState::default());
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
        let app = app(AppState::default());
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
        let app = app(AppState::default());
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
        let app = app(AppState::default());
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
}
