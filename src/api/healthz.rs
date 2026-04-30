use axum::{extract::State, http::StatusCode};
use tokio::time::{Duration, timeout};

use crate::AppState;

pub(crate) async fn healthz(State(state): State<AppState>) -> (StatusCode, &'static str) {
    match timeout(Duration::from_secs(2), state.store.ping()).await {
        Ok(Ok(())) => {}
        Err(_) => return (StatusCode::SERVICE_UNAVAILABLE, "db unavailable"),
        Ok(Err(_)) => return (StatusCode::SERVICE_UNAVAILABLE, "db unavailable"),
    }

    if let Some(miden) = &state.miden {
        match timeout(Duration::from_secs(5), miden.tip_block_height()).await {
            Ok(Ok(_)) => {}
            Err(_) => return (StatusCode::SERVICE_UNAVAILABLE, "miden unavailable"),
            Ok(Err(_)) => return (StatusCode::SERVICE_UNAVAILABLE, "miden unavailable"),
        }
    }

    (StatusCode::OK, "ok")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    use crate::{
        AppState, app,
        chains::miden::MidenHealthCheck,
        test_support::{failing_memory_state, memory_state},
    };

    struct TestMiden {
        ok: bool,
    }

    #[async_trait]
    impl MidenHealthCheck for TestMiden {
        async fn tip_block_height(&self) -> anyhow::Result<u32> {
            if self.ok {
                Ok(1)
            } else {
                anyhow::bail!("miden down")
            }
        }
    }

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

    #[tokio::test]
    async fn returns_service_unavailable_when_miden_is_unreachable() {
        let mut state = AppState::new(memory_state());
        state.miden = Some(Arc::new(TestMiden { ok: false }));
        let app = app(state);
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

    #[tokio::test]
    async fn returns_ok_when_miden_is_reachable() {
        let mut state = AppState::new(memory_state());
        state.miden = Some(Arc::new(TestMiden { ok: true }));
        let app = app(state);
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
}
