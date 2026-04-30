use axum::{extract::State, http::StatusCode};
use tokio::time::{Duration, timeout};

use crate::AppState;

pub(crate) async fn healthz(State(state): State<AppState>) -> (StatusCode, &'static str) {
    match timeout(Duration::from_secs(2), state.store.ping()).await {
        Ok(Ok(())) => (StatusCode::OK, "ok"),
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, "db unavailable"),
        Ok(Err(_)) => (StatusCode::SERVICE_UNAVAILABLE, "db unavailable"),
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    use crate::{
        AppState, app,
        test_support::{failing_memory_state, memory_state},
    };

    #[tokio::test]
    async fn returns_ok() {
        let app = app(AppState::new(memory_state()));
        let response = app
            .oneshot(
                Request::get("/healthz")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn returns_service_unavailable_when_ping_fails() {
        let app = app(AppState::new(failing_memory_state()));
        let response = app
            .oneshot(
                Request::get("/healthz")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
