use axum::{Json, extract::Query};
use serde::Deserialize;

use crate::types::WithdrawalsResponse;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawalsQuery {
    #[allow(dead_code)]
    deposit_address: Option<String>,
    #[allow(dead_code)]
    deposit_memo: Option<String>,
    #[allow(dead_code)]
    timestamp_from: Option<String>,
    #[allow(dead_code)]
    page: Option<f64>,
    #[allow(dead_code)]
    limit: Option<f64>,
    #[allow(dead_code)]
    sort_order: Option<String>,
}

pub async fn withdrawals(Query(_query): Query<WithdrawalsQuery>) -> Json<WithdrawalsResponse> {
    Json(WithdrawalsResponse {
        asset: "eth-anvil:eth".to_owned(),
        recipient: String::new(),
        affiliate_recipient: String::new(),
        withdrawals: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use serde_json::Value;
    use tower::ServiceExt;

    use crate::{AppState, app, test_support::memory_state};

    #[tokio::test]
    async fn returns_object_shape() {
        let app = app(AppState::new(memory_state()));
        let response = app
            .oneshot(
                Request::get("/v0/any-input/withdrawals")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let json: Value = serde_json::from_slice(&body).expect("json body");

        assert!(json.is_object());
        assert!(json.get("withdrawals").is_some());
    }
}
