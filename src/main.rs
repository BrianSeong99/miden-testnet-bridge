use std::{
    env,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result};
use axum::http::{HeaderValue, Method};
use miden_testnet_bridge::{
    AppState, LogFormat, app, arc_store,
    chains::{
        evm::{EvmClient, EvmConfig, token_addresses_path_from_env},
        miden::MidenClient,
        miden_bootstrap::bootstrap_miden,
        miden_outbound::poll_outbound_deposits,
    },
    core::{
        hardening::{spawn_deadline_expiry_scanner, spawn_stuck_processing_scanner},
        lifecycle::{DefaultLifecycle, resume_in_flight_quotes},
        pricer::CoinGeckoPricer,
        state::{PostgresStateStore, connect_pool},
    },
    init_tracing,
};
use sqlx::migrate::Migrator;
use tower_http::cors::CorsLayer;
use tracing::info;

#[derive(Clone, Debug)]
struct Config {
    database_url: String,
    miden_rpc_url: String,
    miden_remote_prover_url: Option<String>,
    miden_remote_prover_timeout_secs: u64,
    miden_store_dir: PathBuf,
    miden_master_seed_hex: String,
    evm_rpc_url: String,
    master_mnemonic: String,
    solver_private_key: String,
    evm_chain_id: u64,
    http_port: u16,
    bridge_profile: String,
    bridge_demo_enabled: bool,
    bridge_ui_enabled: bool,
    bridge_cors_allow_origin: String,
    rust_log: String,
    log_format: LogFormat,
    deadline_scan_interval_secs: u64,
}

impl Config {
    fn from_env() -> Result<Self> {
        let rust_log = env::var("RUST_LOG")
            .unwrap_or_else(|_| "info,sqlx=warn,hyper=warn,tower_http=warn".to_owned());
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
                .unwrap_or_else(|_| "https://rpc.testnet.miden.io".to_owned()),
            miden_remote_prover_url: optional_env("MIDEN_REMOTE_PROVER_URL"),
            miden_remote_prover_timeout_secs: env::var("MIDEN_REMOTE_PROVER_TIMEOUT_SECS")
                .unwrap_or_else(|_| "60".to_owned())
                .parse()
                .context("MIDEN_REMOTE_PROVER_TIMEOUT_SECS must be a valid u64")?,
            miden_store_dir: PathBuf::from(
                env::var("MIDEN_STORE_DIR")
                    .unwrap_or_else(|_| "./.miden-store".to_owned()),
            ),
            miden_master_seed_hex: env::var("MIDEN_MASTER_SEED_HEX").unwrap_or_else(|_| {
                "0101010101010101010101010101010101010101010101010101010101010101".to_owned()
            }),
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
            bridge_profile: env::var("BRIDGE_PROFILE").unwrap_or_else(|_| "anvil".to_owned()),
            bridge_demo_enabled: parse_bool_env("BRIDGE_DEMO_ENABLED", false)?,
            bridge_ui_enabled: parse_bool_env("BRIDGE_UI_ENABLED", true)?,
            bridge_cors_allow_origin: env::var("BRIDGE_CORS_ALLOW_ORIGIN")
                .unwrap_or_else(|_| "*".to_owned()),
            rust_log,
            log_format,
            deadline_scan_interval_secs: env::var("DEADLINE_SCAN_INTERVAL_SECS")
                .unwrap_or_else(|_| "30".to_owned())
                .parse()
                .context("DEADLINE_SCAN_INTERVAL_SECS must be a valid u64")?,
        })
    }

    fn listen_addr(&self) -> SocketAddr {
        SocketAddr::from(([0, 0, 0, 0], self.http_port))
    }

    fn validate(&self) -> Result<()> {
        for (name, value) in [
            ("DATABASE_URL", self.database_url.as_str()),
            ("MIDEN_RPC_URL", self.miden_rpc_url.as_str()),
            ("MIDEN_MASTER_SEED_HEX", self.miden_master_seed_hex.as_str()),
            ("EVM_RPC_URL", self.evm_rpc_url.as_str()),
            ("MASTER_MNEMONIC", self.master_mnemonic.as_str()),
            ("SOLVER_PRIVATE_KEY", self.solver_private_key.as_str()),
        ] {
            if value.trim().is_empty() {
                anyhow::bail!("{name} must not be empty");
            }
        }

        match self.bridge_profile.as_str() {
            "anvil" | "sepolia" => {}
            other => anyhow::bail!("BRIDGE_PROFILE must be anvil or sepolia, got {other}"),
        }

        Ok(())
    }
}

fn parse_bool_env(name: &str, default: bool) -> Result<bool> {
    match env::var(name) {
        Ok(value) => match value.as_str() {
            "1" | "true" | "TRUE" | "yes" | "YES" => Ok(true),
            "0" | "false" | "FALSE" | "no" | "NO" => Ok(false),
            other => anyhow::bail!("{name} must be a boolean, got {other}"),
        },
        Err(_) => Ok(default),
    }
}

fn optional_env(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn parse_master_seed_hex(seed_hex: &str) -> Result<[u8; 32]> {
    let bytes = alloy::hex::decode(seed_hex).context("MIDEN_MASTER_SEED_HEX must be valid hex")?;
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("MIDEN_MASTER_SEED_HEX must decode into 32 bytes"))
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
    let miden_master_seed = parse_master_seed_hex(&config.miden_master_seed_hex)?;
    let miden = Arc::new(
        MidenClient::new_with_remote_prover(
            &config.miden_rpc_url,
            &config.miden_store_dir,
            config.miden_remote_prover_url.clone(),
            Duration::from_secs(config.miden_remote_prover_timeout_secs),
        )
        .await?,
    );
    bootstrap_miden(miden.as_ref(), store.clone(), &miden_master_seed).await?;
    // BRIDGE_PRICER=mock bypasses CoinGecko (rate-limited free tier hits 429
    // under E2E load: 5 tests * 8 tokens * multiple polls). Set in compose
    // for tests; production keeps the default CoinGecko path.
    let pricer: miden_testnet_bridge::core::pricer::DynPricer =
        match std::env::var("BRIDGE_PRICER").as_deref() {
            Ok("mock") => Arc::new(miden_testnet_bridge::core::pricer::MockPricer),
            _ => Arc::new(CoinGeckoPricer::new()),
        };
    let evm = Arc::new(
        EvmClient::new(
            store.clone(),
            EvmConfig {
                rpc_url: config.evm_rpc_url.clone(),
                master_mnemonic: config.master_mnemonic.clone(),
                solver_private_key: config.solver_private_key.clone(),
                token_addresses_path: token_addresses_path_from_env(),
                chain_id: config.evm_chain_id,
            },
        )?
        .with_miden_client(miden.clone()),
    );
    let lifecycle = Arc::new(DefaultLifecycle::new(
        store.clone(),
        pricer.clone(),
        evm.clone(),
        miden.clone(),
    ));
    resume_in_flight_quotes(store.clone(), lifecycle.clone()).await?;
    let deadline_scan_interval = Duration::from_secs(config.deadline_scan_interval_secs);
    spawn_deadline_expiry_scanner(store.clone(), lifecycle.clone(), deadline_scan_interval);
    spawn_stuck_processing_scanner(store.clone(), deadline_scan_interval);
    tokio::spawn(evm.clone().watch_deposits(lifecycle.clone()));
    let outbound_miden = miden.clone();
    let outbound_store = store.clone();
    let outbound_evm = evm.clone();
    let outbound_lifecycle = lifecycle.clone();
    tokio::task::spawn_blocking(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("outbound poller runtime");
        runtime
            .block_on(poll_outbound_deposits(
                outbound_miden,
                outbound_store,
                outbound_evm,
                miden_master_seed,
                outbound_lifecycle,
            ))
            .expect("outbound poller");
    });

    let state = AppState::with_clients(store, pricer, evm, miden, miden_master_seed)
        .with_lifecycle(lifecycle)
        .with_runtime_options(
            config.bridge_demo_enabled,
            config.bridge_ui_enabled,
            config.bridge_profile.clone(),
        );
    let app = app(state).layer(cors_layer(&config)?);
    let listener = tokio::net::TcpListener::bind(config.listen_addr())
        .await
        .context("failed to bind HTTP listener")?;

    info!(address = %config.listen_addr(), "bridge listening");

    axum::serve(listener, app)
        .await
        .context("HTTP server stopped unexpectedly")
}

fn cors_layer(config: &Config) -> Result<CorsLayer> {
    let layer = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(tower_http::cors::Any);
    if config.bridge_cors_allow_origin.trim() == "*" {
        Ok(layer.allow_origin(tower_http::cors::Any))
    } else {
        Ok(layer.allow_origin(
            HeaderValue::from_str(&config.bridge_cors_allow_origin)
                .context("BRIDGE_CORS_ALLOW_ORIGIN must be a valid header value")?,
        ))
    }
}
