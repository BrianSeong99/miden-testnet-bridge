use axum::{Json, extract::rejection::JsonRejection};

use crate::{
    api::errors::ApiError,
    chains::agglayer::{
        AgglayerConfig, AgglayerInfo, AgglayerL1DepositPlan, AgglayerL1DepositPlanRequest,
        AgglayerL2WithdrawPlan, AgglayerL2WithdrawPlanRequest, agglayer_info,
        build_l1_deposit_plan, build_l2_withdraw_plan,
    },
};

pub async fn info() -> Result<Json<AgglayerInfo>, ApiError> {
    Ok(Json(
        agglayer_info().map_err(|error| ApiError::internal(error.to_string()))?,
    ))
}

pub async fn l1_deposit_plan(
    request: Result<Json<AgglayerL1DepositPlanRequest>, JsonRejection>,
) -> Result<Json<AgglayerL1DepositPlan>, ApiError> {
    let Json(request) = request.map_err(ApiError::from_json_rejection)?;
    let config =
        AgglayerConfig::from_env().map_err(|error| ApiError::internal(error.to_string()))?;
    build_l1_deposit_plan(config, request)
        .map(Json)
        .map_err(|error| ApiError::bad_request(error.to_string()))
}

pub async fn l2_withdraw_plan(
    request: Result<Json<AgglayerL2WithdrawPlanRequest>, JsonRejection>,
) -> Result<Json<AgglayerL2WithdrawPlan>, ApiError> {
    let Json(request) = request.map_err(ApiError::from_json_rejection)?;
    let config =
        AgglayerConfig::from_env().map_err(|error| ApiError::internal(error.to_string()))?;
    build_l2_withdraw_plan(config, request)
        .map(Json)
        .map_err(|error| ApiError::bad_request(error.to_string()))
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
    async fn info_returns_current_reviewed_constants() {
        let app = app(AppState::new(memory_state()));
        let response = app
            .oneshot(
                Request::get("/agglayer/info")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let json: Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(json["constants"]["destNetwork"], 76);
        assert_eq!(json["constants"]["l2ChainId"], 1_022_211_914);
        assert_eq!(
            json["constants"]["midenBridgeId"],
            "mcst1arychvrurzxdy5qwz0mg5p5umsvsepyx"
        );
    }

    #[tokio::test]
    async fn l1_plan_rejects_invalid_miden_account() {
        let app = app(AppState::new(memory_state()));
        let response = app
            .oneshot(
                Request::post("/agglayer/l1/deposit/plan")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "midenAccountId": "not-an-account",
                            "amountEth": "0.001"
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn l1_plan_rejects_unknown_fields() {
        let app = app(AppState::new(memory_state()));
        let response = app
            .oneshot(
                Request::post("/agglayer/l1/deposit/plan")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "midenAccountId": "0xc98bb07c188cd2500e13f68a069cdc",
                            "amountETH": "1"
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
