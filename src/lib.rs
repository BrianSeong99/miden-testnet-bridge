use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use axum::{
    Router,
    http::{HeaderValue, Request, header::AUTHORIZATION},
    routing::{get, post},
};
use chains::evm::EvmClient;
use chains::miden::{MidenClient, MidenHealthCheck};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use tower::{Layer, Service};
use tracing_subscriber::{EnvFilter, fmt};

pub mod api;
pub mod chains;
pub mod core;
pub mod test_support;
pub mod types;

use crate::core::lifecycle::DynLifecycle;
use crate::core::pricer::{DynPricer, MockPricer};
use crate::core::state::DynStateStore;

#[derive(Clone)]
pub struct AppState {
    pub store: DynStateStore,
    pub lifecycle: Option<DynLifecycle>,
    pub pricer: DynPricer,
    pub evm: Option<Arc<EvmClient>>,
    pub miden: Option<Arc<dyn MidenHealthCheck>>,
    pub miden_client: Option<Arc<MidenClient>>,
    pub miden_master_seed: Option<[u8; 32]>,
}

impl AppState {
    pub fn new(store: DynStateStore) -> Self {
        Self {
            store,
            lifecycle: None,
            pricer: Arc::new(MockPricer),
            evm: None,
            miden: None,
            miden_client: None,
            miden_master_seed: None,
        }
    }

    pub fn with_evm(store: DynStateStore, evm: Arc<EvmClient>) -> Self {
        Self {
            store,
            lifecycle: None,
            pricer: Arc::new(MockPricer),
            evm: Some(evm),
            miden: None,
            miden_client: None,
            miden_master_seed: None,
        }
    }

    pub fn with_clients(
        store: DynStateStore,
        pricer: DynPricer,
        evm: Arc<EvmClient>,
        miden: Arc<MidenClient>,
        miden_master_seed: [u8; 32],
    ) -> Self {
        Self {
            store,
            lifecycle: None,
            pricer,
            evm: Some(evm),
            miden: Some(miden.clone()),
            miden_client: Some(miden),
            miden_master_seed: Some(miden_master_seed),
        }
    }

    pub fn with_pricer(mut self, pricer: DynPricer) -> Self {
        self.pricer = pricer;
        self
    }

    pub fn with_lifecycle(mut self, lifecycle: DynLifecycle) -> Self {
        self.lifecycle = Some(lifecycle);
        self
    }
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/v0/quote", post(api::quote::quote))
        .route("/v0/status", get(api::status::status))
        .route("/v0/tokens", get(api::tokens::tokens))
        .route(
            "/v0/deposit/submit",
            post(api::deposit_submit::submit_deposit),
        )
        .route(
            "/v0/any-input/withdrawals",
            get(api::withdrawals::withdrawals),
        )
        .route("/healthz", get(api::healthz::healthz))
        .layer(RedactAuthorizationLayer)
        .with_state(state)
}

pub fn now_iso8601() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("RFC3339 formatting should succeed")
}

pub fn init_tracing(rust_log: &str, log_format: LogFormat) {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(rust_log));

    let builder = fmt().with_env_filter(env_filter);
    match log_format {
        LogFormat::Json => builder.json().init(),
        LogFormat::Pretty => builder.pretty().init(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogFormat {
    Json,
    Pretty,
}

pub fn arc_store<T>(store: T) -> Arc<dyn crate::core::state::StateStore>
where
    T: crate::core::state::StateStore + 'static,
{
    Arc::new(store)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RedactAuthorizationLayer;

impl<S> Layer<S> for RedactAuthorizationLayer {
    type Service = RedactAuthorizationService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RedactAuthorizationService { inner }
    }
}

#[derive(Clone, Debug)]
pub struct RedactAuthorizationService<S> {
    inner: S,
}

impl<S, B> Service<Request<B>> for RedactAuthorizationService<S>
where
    S: Service<Request<B>>,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<B>) -> Self::Future {
        redact_authorization_header(request.headers_mut());
        let future = self.inner.call(request);
        Box::pin(future)
    }
}

pub fn redact_authorization_header(headers: &mut axum::http::HeaderMap) {
    let Some(value) = headers.get(AUTHORIZATION).cloned() else {
        return;
    };
    let redacted = redact_authorization_value(&value);
    headers.insert(AUTHORIZATION, redacted);
}

pub fn redact_authorization_value(value: &HeaderValue) -> HeaderValue {
    let Ok(raw) = value.to_str() else {
        return HeaderValue::from_static("Bearer invalid");
    };
    let token = raw
        .strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))
        .unwrap_or(raw)
        .trim();
    let digest = Sha256::digest(token.as_bytes());
    let encoded = alloy::hex::encode(digest);
    let prefix = &encoded[..8];
    HeaderValue::from_str(&format!("Bearer {prefix}"))
        .expect("redacted authorization header should be valid")
}

#[cfg(test)]
mod tests {
    use std::{
        io::{self, Write},
        sync::{Arc, Mutex},
    };

    use axum::{
        Router,
        body::Body,
        http::{HeaderValue, Request, StatusCode, header::AUTHORIZATION},
        middleware::{Next, from_fn},
        response::Response,
        routing::get,
    };
    use serde_json::Value;
    use tower::ServiceExt;
    use tracing::subscriber::set_default;
    use tracing_subscriber::EnvFilter;

    use super::{RedactAuthorizationLayer, redact_authorization_value};

    #[derive(Clone, Default)]
    struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for SharedBuffer {
        type Writer = SharedBufferGuard;

        fn make_writer(&'a self) -> Self::Writer {
            SharedBufferGuard(self.0.clone())
        }
    }

    struct SharedBufferGuard(Arc<Mutex<Vec<u8>>>);

    impl Write for SharedBufferGuard {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().expect("log buffer").extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    async fn log_authorization(request: Request<Body>, next: Next) -> Response {
        let authorization = request
            .headers()
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        tracing::info!(authorization, "captured request");
        next.run(request).await
    }

    fn captured_logs(buffer: &SharedBuffer) -> String {
        String::from_utf8(buffer.0.lock().expect("log buffer").clone()).expect("utf8 log output")
    }

    #[tokio::test]
    async fn bearer_authorization_is_redacted_before_logging() {
        let buffer = SharedBuffer::default();
        let subscriber = tracing_subscriber::fmt()
            .json()
            .with_env_filter(EnvFilter::new("info"))
            .with_writer(buffer.clone())
            .finish();
        let expected = redact_authorization_value(&HeaderValue::from_static("Bearer secret"))
            .to_str()
            .expect("header value")
            .to_owned();
        let guard = set_default(subscriber);

        let app = Router::new()
            .route("/ping", get(|| async { StatusCode::OK }))
            .layer(from_fn(log_authorization))
            .layer(RedactAuthorizationLayer);

        let response = app
            .oneshot(
                Request::get("/ping")
                    .header(AUTHORIZATION, "Bearer secret")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        drop(guard);

        assert_eq!(response.status(), StatusCode::OK);

        let logs = captured_logs(&buffer);
        assert!(logs.contains(&expected));
        assert!(!logs.contains("secret"));
    }

    #[test]
    fn json_logs_are_parseable() {
        let buffer = SharedBuffer::default();
        let subscriber = tracing_subscriber::fmt()
            .json()
            .with_env_filter(EnvFilter::new("info"))
            .with_writer(buffer.clone())
            .finish();
        let guard = set_default(subscriber);
        tracing::info!(component = "test", "json log");
        drop(guard);

        let logs = captured_logs(&buffer);
        let line = logs
            .lines()
            .find(|line| !line.is_empty())
            .expect("json line");
        let value: Value = serde_json::from_str(line).expect("valid json log line");
        assert_eq!(value["level"], "INFO");
        assert!(value["target"].as_str().is_some());
        assert_eq!(value["fields"]["message"], "json log");
        assert_eq!(value["fields"]["component"], "test");
        assert!(value.get("timestamp").is_some());
    }
}
