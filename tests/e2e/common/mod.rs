use std::{
    process::Command,
    str::FromStr,
    sync::OnceLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use alloy::{
    network::TransactionBuilder as _,
    primitives::{Address, U256},
    providers::{Provider, ProviderBuilder},
    rpc::types::eth::TransactionRequest,
    signers::{Signer, local::PrivateKeySigner},
};
use anyhow::{Context, Result, anyhow, bail, ensure};
use miden_client::{
    account::Account, auth::AuthSecretKey, keystore::Keystore,
    transaction::TransactionRequestBuilder,
};
use miden_testnet_bridge::{
    chains::{
        miden::{MidenClient, parse_account_id},
        miden_bootstrap::{
            BootstrapState, bootstrap_state_from_record, sync_with_retry, wait_for_tx,
        },
        miden_bridge_note::{BridgeOutDepositMemo, build_bridge_out_note},
        miden_deposit_account::build_wallet_account,
    },
    core::state::{PostgresStateStore, StateStore, connect_pool},
    types::{QuoteResponse, StatusResponse, SwapStatus},
};
use rand::SeedableRng;
use reqwest::StatusCode;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{query::query, row::Row};
use sqlx_postgres::PgPool;
use tempfile::{TempDir, tempdir};
use tokio::time::{Instant, sleep};
use uuid::Uuid;

const DEFAULT_FUNDED_PRIVATE_KEY: &str =
    "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
const DEFAULT_EVM_REFUND_ADDRESS: &str = "0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc";
pub const LOCAL_ETH_E2E_AMOUNT: u128 = 1_000_000_000_000;
pub const LOCAL_ETH_E2E_AMOUNT_STR: &str = "1000000000000";

#[derive(Clone, Copy)]
pub enum Direction {
    Inbound,
    Outbound,
}

pub struct ComposeGuard {
    envs: Vec<(String, String)>,
}

impl ComposeGuard {
    pub fn new(envs: Vec<(String, String)>) -> Self {
        Self { envs }
    }
}

impl Drop for ComposeGuard {
    fn drop(&mut self) {
        let _ = compose_down_with_env(&self.envs);
    }
}

pub struct TestContext {
    _guard: ComposeGuard,
    pub db_pool: PgPool,
    pub miden: MidenClient,
    pub _miden_store: TempDir,
    envs: Vec<(String, String)>,
    seed_namespace: String,
}

#[allow(dead_code)]
pub struct ChainArtifacts {
    pub evm_deposit_tx_hashes: Vec<String>,
    pub evm_release_tx_hashes: Vec<String>,
    pub miden_mint_tx_ids: Vec<String>,
    pub miden_consume_tx_ids: Vec<String>,
    pub evm_refund_tx_hashes: Vec<String>,
    pub miden_refund_tx_ids: Vec<String>,
}

pub fn run_e2e_enabled() -> bool {
    std::env::var("RUN_E2E").ok().as_deref() == Some("1")
}

pub fn skip_e2e_reason() -> Option<String> {
    if !run_e2e_enabled() {
        return Some("set RUN_E2E=1".to_owned());
    }

    static DOCKER_CHECK: OnceLock<Option<String>> = OnceLock::new();
    DOCKER_CHECK
        .get_or_init(|| {
            docker_access_check()
                .err()
                .map(|message| message.to_string())
        })
        .clone()
}

pub fn require_e2e(test_name: &str) {
    if let Some(reason) = skip_e2e_reason() {
        panic!("skip: {test_name} requires live e2e prerequisites; {reason}");
    }
}

pub async fn start_test(test_name: &str) -> Result<TestContext> {
    let attempts = std::env::var("E2E_STARTUP_ATTEMPTS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(2)
        .max(1);
    let mut last_error = None;

    for attempt in 1..=attempts {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let seed_namespace = format!("{test_name}:{unique}:attempt-{attempt}");
        let miden_store_dir = format!("/var/lib/bridge/miden-store/{test_name}-{unique}-{attempt}");
        let miden_master_seed_hex = hex_seed32(&format!("{seed_namespace}:bridge-master"));
        let envs = vec![
            ("MIDEN_STORE_DIR".to_owned(), miden_store_dir),
            ("MIDEN_MASTER_SEED_HEX".to_owned(), miden_master_seed_hex),
        ];

        if let Err(err) = compose_down_with_env(&envs) {
            eprintln!("E2E startup cleanup before attempt {attempt} failed: {err:#}");
        }

        let startup = async {
            compose_up_with_env(&envs)?;
            wait_for_healthz().await?;

            let db_pool = connect_pool(&database_url(), 5)
                .await
                .context("failed to connect to test postgres")?;
            let miden_store = tempdir().context("failed to create local miden store dir")?;
            let miden = MidenClient::new(&miden_rpc_url(), miden_store.path())
                .await
                .context("failed to initialize host miden client")?;
            miden
                .sync_state()
                .await
                .context("failed to sync host miden")?;

            Ok(TestContext {
                _guard: ComposeGuard::new(envs.clone()),
                db_pool,
                miden,
                _miden_store: miden_store,
                envs: envs.clone(),
                seed_namespace,
            })
        }
        .await;

        match startup {
            Ok(ctx) => return Ok(ctx),
            Err(err) if attempt < attempts => {
                eprintln!("E2E startup attempt {attempt}/{attempts} failed; retrying: {err:#}");
                if let Err(cleanup_err) = compose_down_with_env(&envs) {
                    eprintln!(
                        "E2E startup cleanup after attempt {attempt} failed: {cleanup_err:#}"
                    );
                }
                last_error = Some(err);
                sleep(Duration::from_secs(5)).await;
            }
            Err(err) => {
                if let Err(cleanup_err) = compose_down_with_env(&envs) {
                    eprintln!("E2E startup cleanup after final attempt failed: {cleanup_err:#}");
                }
                return Err(err);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("E2E startup failed before running an attempt")))
}

impl TestContext {
    #[allow(dead_code)]
    pub async fn make_quote(
        &self,
        direction: Direction,
        asset: &str,
        amount: &str,
    ) -> Result<QuoteResponse> {
        let recipient = match direction {
            Direction::Inbound => DEFAULT_EVM_REFUND_ADDRESS.to_owned(),
            Direction::Outbound => DEFAULT_EVM_REFUND_ADDRESS.to_owned(),
        };
        let refund_to = match direction {
            Direction::Inbound => DEFAULT_EVM_REFUND_ADDRESS.to_owned(),
            Direction::Outbound => DEFAULT_EVM_REFUND_ADDRESS.to_owned(),
        };
        self.make_quote_with_parties(direction, asset, amount, &recipient, &refund_to)
            .await
    }

    pub async fn make_quote_with_parties(
        &self,
        direction: Direction,
        asset: &str,
        amount: &str,
        recipient: &str,
        refund_to: &str,
    ) -> Result<QuoteResponse> {
        let (origin_asset, destination_asset) = match direction {
            Direction::Inbound => (
                format!("eth-sepolia:{asset}"),
                format!("miden-testnet:{asset}"),
            ),
            Direction::Outbound => (
                format!("miden-testnet:{asset}"),
                format!("eth-sepolia:{asset}"),
            ),
        };
        let payload = json!({
            "dry": false,
            "depositMode": "SIMPLE",
            "swapType": "EXACT_INPUT",
            "slippageTolerance": 100.0,
            "originAsset": origin_asset,
            "depositType": "ORIGIN_CHAIN",
            "destinationAsset": destination_asset,
            "amount": amount,
            "refundTo": refund_to,
            "refundType": "ORIGIN_CHAIN",
            "recipient": recipient,
            "recipientType": "DESTINATION_CHAIN",
            "deadline": "2027-01-01T00:00:00Z"
        });
        make_quote(payload).await
    }

    pub async fn poll_status_until(
        &self,
        deposit_address: &str,
        deposit_memo: Option<&str>,
        target_status: SwapStatus,
        timeout: Duration,
    ) -> Result<StatusResponse> {
        poll_status_until(deposit_address, deposit_memo, target_status, timeout).await
    }

    pub async fn restart_bridge(&self) -> Result<()> {
        run_command(
            "docker",
            &["compose", "restart", "bridge"],
            &self.envs,
            Some("failed to restart bridge"),
        )?;
        wait_for_healthz_with_timeout(Duration::from_secs(600)).await
    }

    pub async fn bootstrap_state(&self) -> Result<BootstrapState> {
        let store = PostgresStateStore::new(self.db_pool.clone());
        let record = store
            .get_miden_bootstrap()
            .await
            .context("failed to read bootstrap record")?
            .ok_or_else(|| anyhow!("miden bootstrap record is missing"))?;
        bootstrap_state_from_record(&record)
    }

    pub async fn create_wallet(&self, label: &str) -> Result<Account> {
        let init_seed = seed32(&format!("{}:{label}:init", self.seed_namespace));
        let auth_seed = seed32(&format!("{}:{label}:auth", self.seed_namespace));
        create_wallet(&self.miden, init_seed, auth_seed).await
    }

    pub async fn send_outbound_note(
        &self,
        bootstrap: &BootstrapState,
        sender: &Account,
        deposit_address: &str,
        deposit_memo: &str,
        amount: u64,
    ) -> Result<()> {
        let memo = BridgeOutDepositMemo::from_deposit_memo(deposit_memo)?;
        ensure!(
            parse_account_id(&memo.bridge_account_id)? == parse_account_id(deposit_address)?,
            "deposit address and bridge-note memo target differ"
        );
        let mut inner = self.miden.open().await?;
        consume_notes(&mut inner, sender.id()).await?;
        let note = build_bridge_out_note(
            sender.id(),
            vec![
                miden_client::asset::FungibleAsset::new(bootstrap.eth_faucet_account_id, amount)?
                    .into(),
            ],
            &memo,
            inner.rng(),
        )?;
        let request = TransactionRequestBuilder::new()
            .own_output_notes(vec![note])
            .build()?;
        let tx_id = inner.submit_new_transaction(sender.id(), request).await?;
        wait_for_tx(&mut inner, tx_id).await
    }

    pub async fn wait_for_consumable_notes(
        &self,
        wallet: &Account,
        timeout: Duration,
    ) -> Result<usize> {
        let deadline = Instant::now() + timeout;
        loop {
            let mut inner = self.miden.open().await?;
            sync_with_retry(&mut inner).await?;
            let notes = inner.get_consumable_notes(Some(wallet.id())).await?;
            if !notes.is_empty() {
                return Ok(notes.len());
            }
            ensure!(Instant::now() < deadline, "timed out waiting for notes");
            sleep(Duration::from_secs(2)).await;
        }
    }

    pub async fn lifecycle_statuses(&self, correlation_id: Uuid) -> Result<Vec<String>> {
        let rows = query::<sqlx_postgres::Postgres>(
            r#"
            SELECT to_status
            FROM lifecycle_events
            WHERE correlation_id = $1
            ORDER BY id ASC
            "#,
        )
        .bind(correlation_id)
        .fetch_all(&self.db_pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| row.get::<String, _>("to_status"))
            .collect())
    }

    pub async fn chain_artifacts(&self, correlation_id: Uuid) -> Result<ChainArtifacts> {
        let row = query::<sqlx_postgres::Postgres>(
            r#"
            SELECT
                evm_deposit_tx_hashes,
                evm_release_tx_hashes,
                miden_mint_tx_ids,
                miden_consume_tx_ids,
                evm_refund_tx_hashes,
                miden_refund_tx_ids
            FROM chain_artifacts
            WHERE correlation_id = $1
            "#,
        )
        .bind(correlation_id)
        .fetch_one(&self.db_pool)
        .await?;
        Ok(ChainArtifacts {
            evm_deposit_tx_hashes: json_value_to_vec(row.get("evm_deposit_tx_hashes"))?,
            evm_release_tx_hashes: json_value_to_vec(row.get("evm_release_tx_hashes"))?,
            miden_mint_tx_ids: json_value_to_vec(row.get("miden_mint_tx_ids"))?,
            miden_consume_tx_ids: json_value_to_vec(row.get("miden_consume_tx_ids"))?,
            evm_refund_tx_hashes: json_value_to_vec(row.get("evm_refund_tx_hashes"))?,
            miden_refund_tx_ids: json_value_to_vec(row.get("miden_refund_tx_ids"))?,
        })
    }

    pub async fn force_min_amount_out(
        &self,
        correlation_id: Uuid,
        min_amount_out: &str,
    ) -> Result<()> {
        query::<sqlx_postgres::Postgres>(
            r#"
            UPDATE quotes
            SET quote_response_json = jsonb_set(
                    quote_response_json,
                    '{quote,minAmountOut}',
                    to_jsonb($2::text),
                    false
                ),
                updated_at = NOW()
            WHERE correlation_id = $1
            "#,
        )
        .bind(correlation_id)
        .bind(min_amount_out)
        .execute(&self.db_pool)
        .await?;
        Ok(())
    }
}

pub fn bridge_url() -> String {
    std::env::var("BRIDGE_URL").unwrap_or_else(|_| "http://localhost:8080".to_owned())
}

pub fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/miden_bridge".to_owned())
}

pub fn miden_rpc_url() -> String {
    std::env::var("MIDEN_RPC_URL").unwrap_or_else(|_| "https://rpc.testnet.miden.io".to_owned())
}

pub fn evm_rpc_url() -> String {
    std::env::var("EVM_RPC_URL")
        .unwrap_or_else(|_| "https://gateway.tenderly.co/public/sepolia".to_owned())
}

pub fn evm_chain_id() -> u64 {
    std::env::var("EVM_CHAIN_ID")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(11155111)
}

pub fn default_evm_address() -> Address {
    Address::from_str(DEFAULT_EVM_REFUND_ADDRESS).expect("valid default EVM address")
}

#[allow(dead_code)]
pub async fn compose_up() -> Result<()> {
    compose_up_with_env(&[])
}

#[allow(dead_code)]
pub fn compose_down() -> Result<()> {
    compose_down_with_env(&[])
}

pub async fn wait_for_healthz() -> Result<()> {
    wait_for_healthz_with_timeout(Duration::from_secs(300)).await
}

async fn wait_for_healthz_with_timeout(timeout: Duration) -> Result<()> {
    let client = reqwest::Client::new();
    // Bridge bootstrap (faucet deploys + solver liquidity mints + consumes)
    // takes ~60-90s on cold boot. Compose --wait already gates on the bridge
    // healthcheck (5min start_period), but the test harness then double-checks
    // by hitting /healthz + /v0/tokens itself. 5min cap matches compose.
    let deadline = Instant::now() + timeout;
    let health_url = format!("{}/healthz", bridge_url());
    let tokens_url = format!("{}/v0/tokens", bridge_url());
    loop {
        let health_ok = match client.get(&health_url).send().await {
            Ok(response) if response.status() == StatusCode::OK => {
                response.text().await.unwrap_or_default().trim() == "ok"
            }
            Ok(_) | Err(_) => false,
        };
        let tokens_ok = match client.get(&tokens_url).send().await {
            Ok(response) => response.status() == StatusCode::OK,
            Err(_) => false,
        };
        if health_ok && tokens_ok {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("timed out waiting for bridge health and routes");
        }
        sleep(Duration::from_secs(2)).await;
    }
}

pub async fn make_quote(payload: Value) -> Result<QuoteResponse> {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/v0/quote", bridge_url()))
        .json(&payload)
        .send()
        .await
        .context("quote request failed")?;
    ensure!(
        response.status() == StatusCode::OK,
        "quote request failed with {}",
        response.status()
    );
    response
        .json::<QuoteResponse>()
        .await
        .context("failed to decode quote response")
}

pub async fn poll_status_until(
    deposit_address: &str,
    deposit_memo: Option<&str>,
    target_status: SwapStatus,
    timeout: Duration,
) -> Result<StatusResponse> {
    let client = reqwest::Client::new();
    let deadline = Instant::now() + timeout;
    loop {
        let mut url = url::Url::parse(&format!("{}/v0/status", bridge_url()))
            .context("failed to build status URL")?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("depositAddress", deposit_address);
            if let Some(deposit_memo) = deposit_memo {
                query.append_pair("depositMemo", deposit_memo);
            }
        }
        let response = client
            .get(url)
            .send()
            .await
            .context("status request failed")?;
        ensure!(
            response.status() == StatusCode::OK,
            "status request failed with {}",
            response.status()
        );
        let status = response
            .json::<StatusResponse>()
            .await
            .context("failed to decode status response")?;
        if status.status == target_status {
            return Ok(status);
        }
        ensure!(
            Instant::now() < deadline,
            "timed out waiting for {:?}, last status {:?}",
            target_status,
            status.status
        );
        sleep(Duration::from_secs(2)).await;
    }
}

pub async fn send_native_eth(to: &str, amount: u128) -> Result<String> {
    let private_key = std::env::var("DEMO_EVM_FUNDED_PRIVATE_KEY")
        .unwrap_or_else(|_| DEFAULT_FUNDED_PRIVATE_KEY.to_owned());
    let signer: PrivateKeySigner = private_key
        .parse::<PrivateKeySigner>()
        .context("invalid DEMO_EVM_FUNDED_PRIVATE_KEY")?
        .with_chain_id(Some(evm_chain_id()));
    let provider = ProviderBuilder::new()
        .wallet(signer)
        .connect_http(evm_rpc_url().parse()?);
    let pending = provider
        .send_transaction(
            TransactionRequest::default()
                .with_to(Address::from_str(to)?)
                .with_value(U256::from(amount)),
        )
        .await?;
    let tx_hash = format!("{:#x}", pending.tx_hash());
    pending.watch().await?;
    submit_deposit_tx(to, &tx_hash).await?;
    Ok(tx_hash)
}

async fn submit_deposit_tx(deposit_address: &str, tx_hash: &str) -> Result<()> {
    let response = reqwest::Client::new()
        .post(format!("{}/v0/deposit/submit", bridge_url()))
        .json(&json!({
            "depositAddress": deposit_address,
            "txHash": tx_hash,
        }))
        .send()
        .await
        .context("deposit submit request failed")?;
    ensure!(
        response.status().is_success(),
        "deposit submit failed with {}: {}",
        response.status(),
        response.text().await.unwrap_or_default()
    );
    Ok(())
}

pub async fn evm_balance(address: Address) -> Result<U256> {
    ProviderBuilder::new()
        .connect_http(evm_rpc_url().parse()?)
        .get_balance(address)
        .await
        .map_err(Into::into)
}

pub fn assert_status_subsequence(actual: &[String], expected: &[&str]) {
    let mut offset = 0usize;
    for status in actual {
        if offset < expected.len() && status == expected[offset] {
            offset += 1;
        }
    }
    assert_eq!(
        offset,
        expected.len(),
        "missing expected status subsequence {expected:?} in {actual:?}"
    );
}

fn compose_up_with_env(envs: &[(String, String)]) -> Result<()> {
    let prover_timeout_secs =
        std::env::var("MIDEN_REMOTE_PROVER_TIMEOUT_SECS").unwrap_or_else(|_| "180".to_owned());
    let mut compose_envs = vec![
        ("BRIDGE_PRICER".to_owned(), "mock".to_owned()),
        (
            "MIDEN_REMOTE_PROVER_TIMEOUT_SECS".to_owned(),
            prover_timeout_secs,
        ),
    ];
    compose_envs.extend_from_slice(envs);

    // Testnet bootstrap submits several Miden transactions. The remote prover
    // removes local proving cost, but public testnet confirmation can still
    // exceed Docker Compose's default 60s --wait timeout.
    run_command(
        "docker",
        &[
            "compose",
            "-f",
            "compose.yaml",
            "up",
            "-d",
            "--build",
            "--wait",
            "--wait-timeout",
            "600",
        ],
        &compose_envs,
        Some("docker compose up failed"),
    )
    .map_err(|err| anyhow!("{err:#}\n\n{}", compose_diagnostics(&compose_envs)))
}

fn compose_down_with_env(envs: &[(String, String)]) -> Result<()> {
    run_command(
        "docker",
        &["compose", "down", "--volumes", "--remove-orphans"],
        envs,
        None,
    )
}

fn run_command(
    program: &str,
    args: &[&str],
    envs: &[(String, String)],
    failure_context: Option<&str>,
) -> Result<()> {
    let mut command = Command::new(program);
    command.args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = command
        .output()
        .with_context(|| format!("failed to spawn command: {program} {}", args.join(" ")))?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Err(anyhow!(
        "{}: `{program} {}` exited {}.\nstdout:\n{}\nstderr:\n{}",
        failure_context.unwrap_or("command failed"),
        args.join(" "),
        output.status,
        stdout.trim(),
        stderr.trim()
    ))
}

fn compose_diagnostics(envs: &[(String, String)]) -> String {
    let ps = run_command_capture(
        "docker",
        &["compose", "-f", "compose.yaml", "ps", "-a"],
        envs,
    );
    let logs = run_command_capture(
        "docker",
        &[
            "compose",
            "-f",
            "compose.yaml",
            "logs",
            "--no-color",
            "--tail=300",
            "bridge",
            "postgres",
        ],
        envs,
    );

    format!(
        "compose diagnostics\nps:\n{}\n\nlogs:\n{}",
        ps.trim(),
        logs.trim()
    )
}

fn run_command_capture(program: &str, args: &[&str], envs: &[(String, String)]) -> String {
    let mut command = Command::new(program);
    command.args(args);
    for (key, value) in envs {
        command.env(key, value);
    }

    match command.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            format!(
                "`{program} {}` exited {}\nstdout:\n{}\nstderr:\n{}",
                args.join(" "),
                output.status,
                stdout.trim(),
                stderr.trim()
            )
        }
        Err(err) => format!("failed to spawn `{program} {}`: {err}", args.join(" ")),
    }
}

fn docker_access_check() -> Result<()> {
    let output = Command::new("docker")
        .args(["info"])
        .output()
        .context("failed to spawn docker info")?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = stderr
        .lines()
        .chain(stdout.lines())
        .find(|line| !line.trim().is_empty())
        .unwrap_or("docker daemon unavailable");
    Err(anyhow!("requires Docker daemon access: {detail}"))
}

async fn create_wallet(
    client: &MidenClient,
    init_seed: [u8; 32],
    auth_seed: [u8; 32],
) -> Result<Account> {
    let mut rng = rand::rngs::StdRng::from_seed(auth_seed);
    let secret_key = AuthSecretKey::new_falcon512_poseidon2_with_rng(&mut rng);
    let account = build_wallet_account(init_seed, &secret_key)?;
    let keystore = client.open_keystore()?;
    let mut inner = client.open().await?;
    if inner.get_account(account.id()).await?.is_none() {
        keystore.add_key(&secret_key, account.id()).await?;
        inner.add_account(&account, false).await?;
    }
    Ok(account)
}

async fn consume_notes(
    inner: &mut miden_client::Client<miden_client::keystore::FilesystemKeyStore>,
    account_id: miden_client::account::AccountId,
) -> Result<()> {
    sync_with_retry(inner).await?;
    let notes: Vec<_> = inner
        .get_consumable_notes(Some(account_id))
        .await?
        .into_iter()
        .filter_map(|(record, _)| record.try_into().ok())
        .collect();
    if notes.is_empty() {
        return Ok(());
    }

    let request = TransactionRequestBuilder::new().build_consume_notes(notes)?;
    let tx_id = inner.submit_new_transaction(account_id, request).await?;
    wait_for_tx(inner, tx_id).await
}

fn seed32(label: &str) -> [u8; 32] {
    let digest = Sha256::digest(label.as_bytes());
    digest.into()
}

fn hex_seed32(label: &str) -> String {
    alloy::hex::encode(seed32(label))
}

fn json_value_to_vec(value: Value) -> Result<Vec<String>> {
    serde_json::from_value(value).context("failed to decode JSON string vector")
}
