use std::{env, net::SocketAddr, path::Path, sync::Arc};

use anyhow::{Context, Result};
use miden_testnet_bridge::{
    AppState, LogFormat, app, arc_store,
    chains::evm::{EvmClient, EvmConfig, token_addresses_path_from_env},
    core::state::{PostgresStateStore, connect_pool},
    init_tracing,
};
use sqlx::migrate::Migrator;
use tracing::info;

#[derive(Clone, Debug)]
struct Config {
    database_url: String,
    miden_rpc_url: String,
    evm_rpc_url: String,
    master_mnemonic: String,
    solver_private_key: String,
    evm_chain_id: u64,
    http_port: u16,
    rust_log: String,
    log_format: LogFormat,
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
                .unwrap_or_else(|_| {
                    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
                        .to_owned()
                }),
            solver_private_key: env::var("SOLVER_PRIVATE_KEY")
                .unwrap_or_else(|_| "replace-with-solver-private-key".to_owned()),
            evm_chain_id: env::var("EVM_CHAIN_ID")
                .unwrap_or_else(|_| "271828".to_owned())
                .parse()
                .context("EVM_CHAIN_ID must be a valid u64")?,
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
    init_tracing(&config.rust_log, config.log_format);

    let pool = connect_pool(&config.database_url, 10)
        .await
        .context("failed to connect to postgres")?;
    Migrator::new(Path::new("migrations"))
        .await
        .context("failed to load migrations")?
        .run(&pool)
        .await
        .context("failed to run migrations")?;

    let store = arc_store(PostgresStateStore::new(pool));
    let evm = Arc::new(EvmClient::new(
        store.clone(),
        EvmConfig {
            rpc_url: config.evm_rpc_url.clone(),
            master_mnemonic: config.master_mnemonic.clone(),
            solver_private_key: config.solver_private_key.clone(),
            token_addresses_path: token_addresses_path_from_env(),
            chain_id: config.evm_chain_id,
        },
    )?);
    tokio::spawn(evm.clone().watch_deposits());

    let state = AppState::with_evm(store, evm);
    let app = app(state);
    let listener = tokio::net::TcpListener::bind(config.listen_addr())
        .await
        .context("failed to bind HTTP listener")?;

    info!(address = %config.listen_addr(), "bridge listening");

    axum::serve(listener, app)
        .await
        .context("HTTP server stopped unexpectedly")
}
