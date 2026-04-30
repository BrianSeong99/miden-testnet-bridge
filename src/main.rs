use std::{collections::HashMap, env, net::SocketAddr, sync::Arc};

use anyhow::{Context, Result};
use axum::{
    Router,
    routing::{get, post},
};
use time::OffsetDateTime;
use tokio::sync::RwLock;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

pub mod api;
pub mod types;

use crate::types::{QuoteResponse, SubmitDepositTxRequest};

type QuoteKey = (String, Option<String>);

#[derive(Clone, Debug)]
struct Config {
    database_url: String,
    miden_rpc_url: String,
    evm_rpc_url: String,
    master_mnemonic: String,
    solver_private_key: String,
    http_port: u16,
    rust_log: String,
    log_format: LogFormat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LogFormat {
    Json,
    Pretty,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AppState {
    pub(crate) quotes: Arc<RwLock<HashMap<QuoteKey, QuoteResponse>>>,
    pub(crate) deposit_submissions: Arc<RwLock<Vec<SubmitDepositTxRequest>>>,
}

impl Config {
    fn from_env() -> Result<Self> {
        let rust_log = env::var("RUST_LOG").unwrap_or_else(|_| "info".to_owned());
        let log_format = match env::var("LOG_FORMAT")
            .unwrap_or_else(|_| "json".to_owned())
            .as_str()
        {
            "json" => LogFormat::Json,
            "pretty" => LogFormat::Pretty,
            other => anyhow::bail!("unsupported LOG_FORMAT: {other}"),
        };

        Ok(Self {
            database_url: env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgres://postgres:postgres@postgres:5432/miden_bridge".to_owned()
            }),
            miden_rpc_url: env::var("MIDEN_RPC_URL")
                .unwrap_or_else(|_| "http://host.docker.internal:57291".to_owned()),
            evm_rpc_url: env::var("EVM_RPC_URL")
                .unwrap_or_else(|_| "http://host.docker.internal:8545".to_owned()),
            master_mnemonic: env::var("MASTER_MNEMONIC")
                .unwrap_or_else(|_| "replace-with-master-mnemonic".to_owned()),
            solver_private_key: env::var("SOLVER_PRIVATE_KEY")
                .unwrap_or_else(|_| "replace-with-solver-private-key".to_owned()),
            http_port: env::var("BRIDGE_HTTP_PORT")
                .unwrap_or_else(|_| "8080".to_owned())
                .parse()
                .context("BRIDGE_HTTP_PORT must be a valid u16")?,
            rust_log,
            log_format,
        })
    }

    fn listen_addr(&self) -> SocketAddr {
        SocketAddr::from(([0, 0, 0, 0], self.http_port))
    }

    fn validate(&self) -> Result<()> {
        for (name, value) in [
            ("DATABASE_URL", self.database_url.as_str()),
            ("MIDEN_RPC_URL", self.miden_rpc_url.as_str()),
            ("EVM_RPC_URL", self.evm_rpc_url.as_str()),
            ("MASTER_MNEMONIC", self.master_mnemonic.as_str()),
            ("SOLVER_PRIVATE_KEY", self.solver_private_key.as_str()),
        ] {
            if value.trim().is_empty() {
                anyhow::bail!("{name} must not be empty");
            }
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_env()?;
    config.validate()?;
    init_tracing(&config);

    let app = app(AppState::default());
    let listener = tokio::net::TcpListener::bind(config.listen_addr())
        .await
        .context("failed to bind HTTP listener")?;

    info!(address = %config.listen_addr(), "bridge listening");

    axum::serve(listener, app)
        .await
        .context("HTTP server stopped unexpectedly")
}

fn init_tracing(config: &Config) {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(config.rust_log.clone()));

    let builder = fmt().with_env_filter(env_filter);
    match config.log_format {
        LogFormat::Json => builder.json().init(),
        LogFormat::Pretty => builder.pretty().init(),
    }
}

pub(crate) fn app(state: AppState) -> Router {
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

pub(crate) fn now_iso8601() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("RFC3339 formatting should succeed")
}
