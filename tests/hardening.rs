use std::{
    io::{self, Write},
    path::Path,
    sync::{Arc, Mutex},
};

use axum::{
    body::Body,
    http::{HeaderValue, Request, StatusCode, header::AUTHORIZATION},
    middleware::{Next, from_fn},
    response::Response,
};
use miden_testnet_bridge::{
    RedactAuthorizationLayer, arc_store,
    chains::{
        evm::{EvmClient, EvmConfig},
        miden::MidenClient,
    },
    core::{
        hardening::{run_deadline_expiry_tick, run_stuck_processing_scan_tick},
        lifecycle::DefaultLifecycle,
        pricer::MockPricer,
        state::{PostgresStateStore, StateStore},
    },
    redact_authorization_value,
    types::{
        DepositMode, DepositType, Quote, QuoteRequest, QuoteResponse, RecipientType, RefundType,
        SwapType,
    },
};
use serde_json::Value;
use sqlx::{migrate::Migrator, query::query, row::Row};
use sqlx_postgres::PgPool;
use tempfile::tempdir;
use testcontainers::{ContainerAsync, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;
use tracing::subscriber::set_default;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

const DEFAULT_SOLVER_PRIVATE_KEY: &str =
    "0x59c6995e998f97a5a0044966f0945382dbb7d2745078b2336b91c60d50d6b6d7";

struct TestDatabase {
    _container: ContainerAsync<Postgres>,
    url: String,
}

impl TestDatabase {
    async fn start() -> Self {
        let container = Postgres::default()
            .with_db_name("miden_bridge")
            .with_user("postgres")
            .with_password("postgres")
            .start()
            .await
            .expect("postgres container");
        let url = format!(
            "postgres://postgres:postgres@{}:{}/miden_bridge",
            container.get_host().await.expect("postgres host"),
            container
                .get_host_port_ipv4(5432)
                .await
                .expect("postgres port mapping"),
        );

        Self {
            _container: container,
            url,
        }
    }

    async fn pool(&self) -> PgPool {
        let pool = sqlx_postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(&self.url)
            .await
            .expect("postgres pool");
        Migrator::new(Path::new("migrations"))
            .await
            .expect("migrator")
            .run(&pool)
            .await
            .expect("run migrations");
        pool
    }
}

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

#[tokio::test(flavor = "current_thread")]
async fn deadline_expiry_tick_refunds_past_due_quote_without_deposit() {
    let db = TestDatabase::start().await;
    let pool = db.pool().await;
    let concrete_store = PostgresStateStore::new(pool.clone());
    let store = arc_store(concrete_store.clone());
    let lifecycle = build_lifecycle(store.clone()).await;
    let request = sample_quote_request("2026-01-01T00:00:00Z");
    let response = sample_quote_response(Uuid::new_v4(), &request);
    let correlation_id = Uuid::parse_str(&response.correlation_id).expect("correlation id");

    concrete_store
        .insert_quote(&response, &request)
        .await
        .expect("insert quote");

    let processed = run_deadline_expiry_tick(store.clone(), lifecycle)
        .await
        .expect("deadline tick");
    assert_eq!(processed, 1);

    let record = concrete_store
        .get_quote_by_correlation_id(correlation_id)
        .await
        .expect("get quote")
        .expect("quote record");
    assert_eq!(record.status, "REFUNDED");
    assert!(record.evm_refund_tx_hashes.is_empty());

    let event = query::<sqlx_postgres::Postgres>(
        r#"
        SELECT to_status, reason
        FROM lifecycle_events
        WHERE correlation_id = $1
        ORDER BY id DESC
        LIMIT 1
        "#,
    )
    .bind(correlation_id)
    .fetch_one(&pool)
    .await
    .expect("deadline event");
    assert_eq!(
        event.try_get::<String, _>("to_status").expect("to status"),
        "REFUNDED"
    );
    assert_eq!(
        event
            .try_get::<Option<String>, _>("reason")
            .expect("reason"),
        Some("deadline_expired_no_deposit".to_owned())
    );
}

#[tokio::test(flavor = "current_thread")]
async fn stuck_processing_tick_logs_warn_and_leaves_state_unchanged() {
    let db = TestDatabase::start().await;
    let pool = db.pool().await;
    let store = PostgresStateStore::new(pool.clone());
    let request = sample_quote_request("2027-01-01T00:00:00Z");
    let response = sample_quote_response(Uuid::new_v4(), &request);
    let correlation_id = Uuid::parse_str(&response.correlation_id).expect("correlation id");

    store
        .insert_quote(&response, &request)
        .await
        .expect("insert quote");
    store
        .record_event(
            correlation_id,
            Some("PENDING_DEPOSIT"),
            "PROCESSING",
            "SETTLEMENT_INITIATED",
            None,
            None,
        )
        .await
        .expect("set processing");
    query::<sqlx_postgres::Postgres>(
        "UPDATE quotes SET updated_at = NOW() - INTERVAL '31 minutes' WHERE correlation_id = $1",
    )
    .bind(correlation_id)
    .execute(&pool)
    .await
    .expect("age quote");

    let buffer = SharedBuffer::default();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::new("warn"))
        .with_writer(buffer.clone())
        .finish();
    let guard = set_default(subscriber);

    let processed = run_stuck_processing_scan_tick(arc_store(store.clone()))
        .await
        .expect("processing tick");
    drop(guard);

    assert_eq!(processed, 1);

    let record = store
        .get_quote_by_correlation_id(correlation_id)
        .await
        .expect("get quote")
        .expect("quote record");
    assert_eq!(record.status, "PROCESSING");

    let logs = String::from_utf8(buffer.0.lock().expect("log buffer").clone()).expect("utf8 logs");
    let line = logs
        .lines()
        .find(|line| !line.is_empty())
        .expect("warn log");
    let value: Value = serde_json::from_str(line).expect("valid json log");
    assert_eq!(value["level"], "WARN");
    assert_eq!(
        value["fields"]["message"],
        "processing quote exceeds watchdog threshold"
    );
    assert_eq!(
        value["fields"]["correlation_id"],
        correlation_id.to_string()
    );
    assert_eq!(value["fields"]["status"], "PROCESSING");
    assert!(value["fields"]["last_event_at"].is_string());
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

    let app = axum::Router::new()
        .route("/ping", axum::routing::get(|| async { StatusCode::OK }))
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

async fn build_lifecycle(
    store: Arc<dyn miden_testnet_bridge::core::state::StateStore>,
) -> Arc<DefaultLifecycle> {
    let temp_dir = tempdir().expect("tempdir");
    let token_file = temp_dir.path().join("token-addresses.json");
    let evm = Arc::new(
        EvmClient::new(
            store.clone(),
            EvmConfig {
                rpc_url: "http://127.0.0.1:8545".to_owned(),
                master_mnemonic: "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about".to_owned(),
                solver_private_key: DEFAULT_SOLVER_PRIVATE_KEY.to_owned(),
                token_addresses_path: token_file,
                chain_id: 271828,
            },
        )
        .expect("evm client"),
    );
    let miden = Arc::new(
        MidenClient::new("http://localhost:57291", temp_dir.path())
            .await
            .expect("miden client"),
    );
    Arc::new(DefaultLifecycle::new(
        store,
        Arc::new(MockPricer),
        evm,
        miden,
    ))
}

fn sample_quote_request(deadline: &str) -> QuoteRequest {
    QuoteRequest {
        dry: false,
        deposit_mode: Some(DepositMode::Simple),
        swap_type: SwapType::ExactInput,
        slippage_tolerance: 100.0,
        origin_asset: "eth-anvil:eth".to_owned(),
        deposit_type: DepositType::OriginChain,
        destination_asset: "miden-local:eth".to_owned(),
        amount: "1000".to_owned(),
        refund_to: "0xfeed".to_owned(),
        refund_type: RefundType::OriginChain,
        recipient: "recipient".to_owned(),
        connected_wallets: None,
        session_id: None,
        virtual_chain_recipient: None,
        virtual_chain_refund_recipient: None,
        custom_recipient_msg: None,
        recipient_type: RecipientType::DestinationChain,
        deadline: deadline.to_owned(),
        referral: None,
        quote_waiting_time_ms: None,
        app_fees: None,
    }
}

fn sample_quote_response(correlation_id: Uuid, request: &QuoteRequest) -> QuoteResponse {
    QuoteResponse {
        correlation_id: correlation_id.to_string(),
        timestamp: "2026-04-30T00:00:00Z".to_owned(),
        signature: String::new(),
        quote_request: request.clone(),
        quote: Quote {
            deposit_address: Some(format!("mock-{correlation_id}")),
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
            deadline: Some(request.deadline.clone()),
            time_when_inactive: Some(request.deadline.clone()),
            time_estimate: 120.0,
            virtual_chain_recipient: None,
            virtual_chain_refund_recipient: None,
            custom_recipient_msg: None,
            refund_fee: None,
        },
    }
}
