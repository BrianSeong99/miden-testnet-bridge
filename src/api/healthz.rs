use axum::{extract::State, http::StatusCode};

use crate::AppState;

pub(crate) async fn healthz(State(state): State<AppState>) -> StatusCode {
    let _ = state.quotes.read().await.len();
    StatusCode::OK
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    use crate::{AppState, app};

    #[tokio::test]
    async fn returns_ok() {
        let app = app(AppState::default());
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
