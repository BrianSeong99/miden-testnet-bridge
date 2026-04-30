use std::sync::Arc;

use axum::{
    Router,
    routing::{get, post},
};
use chains::evm::EvmClient;
use chains::miden::{MidenClient, MidenHealthCheck};
use time::OffsetDateTime;
use tracing_subscriber::{EnvFilter, fmt};

pub mod api;
pub mod chains;
pub mod core;
pub mod test_support;
pub mod types;

use crate::core::pricer::{DynPricer, MockPricer};
use crate::core::state::DynStateStore;

#[derive(Clone)]
pub struct AppState {
    pub store: DynStateStore,
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
