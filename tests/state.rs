use std::path::Path;

use miden_testnet_bridge::{
    core::state::{PostgresStateStore, StateStore, TxHashColumn},
    types::{
        DepositMode, DepositType, Quote, QuoteRequest, QuoteResponse, RecipientType, RefundType,
        SwapType,
    },
};
use serde_json::Value;
use sqlx::{migrate::Migrator, query::query, row::Row};
use sqlx_postgres::PgPool;
use testcontainers::{ContainerAsync, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use uuid::Uuid;

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
        let migrator = Migrator::new(Path::new("migrations"))
            .await
            .expect("migrator");
        migrator.run(&pool).await.expect("run migrations");
        pool
    }
}

fn sample_quote_request() -> QuoteRequest {
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
        deadline: "2026-06-12T00:00:00Z".to_owned(),
        referral: None,
        quote_waiting_time_ms: None,
        app_fees: None,
    }
}

fn sample_quote_response(correlation_id: Uuid) -> QuoteResponse {
    let request = sample_quote_request();

    QuoteResponse {
        correlation_id: correlation_id.to_string(),
        timestamp: "2026-04-30T00:00:00Z".to_owned(),
        signature: String::new(),
        quote_request: request.clone(),
        quote: Quote {
            deposit_address: Some("mock-deposit-address".to_owned()),
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

async fn setup_store() -> (
    TestDatabase,
    PgPool,
    PostgresStateStore,
    QuoteRequest,
    QuoteResponse,
) {
    let db = TestDatabase::start().await;
    let pool = db.pool().await;
    let store = PostgresStateStore::new(pool.clone());
    let request = sample_quote_request();
    let response = sample_quote_response(Uuid::new_v4());
    (db, pool, store, request, response)
}

#[tokio::test(flavor = "current_thread")]
async fn insert_quote_persists_quote_and_bootstrap_rows() {
    let (_db, pool, store, request, response) = setup_store().await;

    store
        .insert_quote(&response, &request)
        .await
        .expect("insert quote");

    let quote_count: i64 = query::<sqlx_postgres::Postgres>("SELECT COUNT(*) AS count FROM quotes")
        .fetch_one(&pool)
        .await
        .expect("count quotes")
        .try_get("count")
        .expect("quote count");
    let chain_count: i64 =
        query::<sqlx_postgres::Postgres>("SELECT COUNT(*) AS count FROM chain_artifacts")
            .fetch_one(&pool)
            .await
            .expect("count artifacts")
            .try_get("count")
            .expect("artifact count");
    let event = query::<sqlx_postgres::Postgres>(
        "SELECT event_kind, to_status FROM lifecycle_events LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .expect("fetch lifecycle event");

    assert_eq!(quote_count, 1);
    assert_eq!(chain_count, 1);
    assert_eq!(
        event
            .try_get::<String, _>("event_kind")
            .expect("event kind"),
        "QUOTE_CREATED"
    );
    assert_eq!(
        event.try_get::<String, _>("to_status").expect("to status"),
        "PENDING_DEPOSIT"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn get_quote_by_deposit_returns_joined_record() {
    let (_db, _pool, store, request, response) = setup_store().await;
    let correlation_id = Uuid::parse_str(&response.correlation_id).expect("correlation id");
    store
        .insert_quote(&response, &request)
        .await
        .expect("insert quote");
    store
        .append_tx_hash(correlation_id, TxHashColumn::EvmDepositTxHashes, "0xabc")
        .await
        .expect("append deposit tx");
    store
        .append_tx_hash(correlation_id, TxHashColumn::IntentHashes, "intent-1")
        .await
        .expect("append intent hash");

    let record = store
        .get_quote_by_deposit("mock-deposit-address", None)
        .await
        .expect("get quote by deposit")
        .expect("quote record");

    assert_eq!(
        record.quote_response.correlation_id,
        response.correlation_id
    );
    assert_eq!(record.status, "PENDING_DEPOSIT");
    assert_eq!(record.evm_deposit_tx_hashes, vec!["0xabc".to_owned()]);
    assert_eq!(record.intent_hashes, vec!["intent-1".to_owned()]);
}

#[tokio::test(flavor = "current_thread")]
async fn record_event_updates_status_and_writes_lifecycle_event() {
    let (_db, pool, store, request, response) = setup_store().await;
    let correlation_id = Uuid::parse_str(&response.correlation_id).expect("correlation id");
    store
        .insert_quote(&response, &request)
        .await
        .expect("insert quote");

    store
        .record_event(
            correlation_id,
            Some("PENDING_DEPOSIT"),
            "KNOWN_DEPOSIT_TX",
            "DEPOSIT_SUBMITTED",
            Some("manual test"),
            Some(serde_json::json!({ "txHash": "0xabc" })),
        )
        .await
        .expect("record event");

    let record = store
        .get_quote_by_deposit("mock-deposit-address", None)
        .await
        .expect("get quote by deposit")
        .expect("quote record");
    let event = query::<sqlx_postgres::Postgres>(
        r#"
        SELECT from_status, to_status, event_kind, reason, metadata
        FROM lifecycle_events
        WHERE correlation_id = $1
        ORDER BY id DESC
        LIMIT 1
        "#,
    )
    .bind(correlation_id)
    .fetch_one(&pool)
    .await
    .expect("fetch lifecycle event");

    assert_eq!(record.status, "KNOWN_DEPOSIT_TX");
    assert_eq!(
        event
            .try_get::<Option<String>, _>("from_status")
            .expect("from status"),
        Some("PENDING_DEPOSIT".to_owned())
    );
    assert_eq!(
        event.try_get::<String, _>("to_status").expect("to status"),
        "KNOWN_DEPOSIT_TX"
    );
    assert_eq!(
        event
            .try_get::<String, _>("event_kind")
            .expect("event kind"),
        "DEPOSIT_SUBMITTED"
    );
    assert_eq!(
        event
            .try_get::<Option<String>, _>("reason")
            .expect("reason"),
        Some("manual test".to_owned())
    );
    assert_eq!(
        event
            .try_get::<Option<Value>, _>("metadata")
            .expect("metadata"),
        Some(serde_json::json!({ "txHash": "0xabc" }))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn record_idempotency_key_is_a_no_op_on_replay() {
    let (_db, _pool, store, request, response) = setup_store().await;
    let correlation_id = Uuid::parse_str(&response.correlation_id).expect("correlation id");
    store
        .insert_quote(&response, &request)
        .await
        .expect("insert quote");

    let first = store
        .record_idempotency_key(correlation_id, "idempotency-key-1")
        .await
        .expect("first idempotency insert");
    let second = store
        .record_idempotency_key(correlation_id, "idempotency-key-1")
        .await
        .expect("second idempotency insert");
    let record = store
        .get_quote_by_deposit("mock-deposit-address", None)
        .await
        .expect("get quote by deposit")
        .expect("quote record");

    assert!(first);
    assert!(!second);
    assert_eq!(
        record.idempotency_keys,
        vec!["idempotency-key-1".to_owned()]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn append_tx_hash_updates_target_jsonb_array() {
    let (_db, _pool, store, request, response) = setup_store().await;
    let correlation_id = Uuid::parse_str(&response.correlation_id).expect("correlation id");
    store
        .insert_quote(&response, &request)
        .await
        .expect("insert quote");

    store
        .append_tx_hash(correlation_id, TxHashColumn::NearTxHashes, "near-hash-1")
        .await
        .expect("append near tx hash");
    store
        .append_tx_hash(correlation_id, TxHashColumn::MidenMintTxIds, "miden-tx-1")
        .await
        .expect("append miden mint tx");

    let record = store
        .get_quote_by_deposit("mock-deposit-address", None)
        .await
        .expect("get quote by deposit")
        .expect("quote record");

    assert_eq!(record.near_tx_hashes, vec!["near-hash-1".to_owned()]);
    assert_eq!(record.miden_mint_tx_ids, vec!["miden-tx-1".to_owned()]);
}

#[tokio::test(flavor = "current_thread")]
async fn quote_state_survives_store_restart() {
    let (db, pool, store, request, response) = setup_store().await;
    store
        .insert_quote(&response, &request)
        .await
        .expect("insert quote");
    drop(store);
    drop(pool);

    let restarted_pool = db.pool().await;
    let restarted_store = PostgresStateStore::new(restarted_pool);
    let record = restarted_store
        .get_quote_by_deposit("mock-deposit-address", None)
        .await
        .expect("get quote after restart")
        .expect("quote record after restart");

    assert_eq!(
        record.quote_response.correlation_id,
        response.correlation_id
    );
}

#[tokio::test(flavor = "current_thread")]
async fn migrations_run_up_and_down_successfully() {
    let db = TestDatabase::start().await;
    let pool = sqlx_postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&db.url)
        .await
        .expect("postgres pool");
    let migrator = Migrator::new(Path::new("migrations"))
        .await
        .expect("migrator");

    migrator.run(&pool).await.expect("run migrations");
    migrator.undo(&pool, 0).await.expect("undo migrations");
    migrator.run(&pool).await.expect("rerun migrations");

    let quotes_exists = query::<sqlx_postgres::Postgres>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM information_schema.tables
            WHERE table_name = 'quotes'
        ) AS exists
        "#,
    )
    .fetch_one(&pool)
    .await
    .expect("quotes existence check")
    .try_get::<bool, _>("exists")
    .expect("exists flag");

    assert!(quotes_exists);
}
