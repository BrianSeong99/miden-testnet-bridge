use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    str::FromStr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use alloy::{
    network::TransactionBuilder as _,
    primitives::{Address, B256, U256},
    providers::{Provider, ProviderBuilder},
    rpc::types::eth::TransactionRequest,
    signers::{Signer, local::PrivateKeySigner},
};
use anyhow::{Context, Result, anyhow, ensure};
use miden_client::{
    account::Account, auth::AuthSecretKey, keystore::Keystore,
    transaction::TransactionRequestBuilder,
};
use miden_testnet_bridge::{
    chains::{
        miden::{MidenClient, parse_account_id},
        miden_bootstrap::{bootstrap_state_from_record, sync_with_retry, wait_for_tx},
        miden_bridge_note::{BridgeOutDepositMemo, build_bridge_out_note},
        miden_deposit_account::build_wallet_account,
    },
    core::state::{PostgresStateStore, StateStore, connect_pool},
    types::{
        DepositMode, DepositType, QuoteRequest, QuoteResponse, RecipientType, RefundType,
        StatusResponse, SubmitDepositTxRequest, SubmitDepositTxResponse, SwapStatus, SwapType,
    },
};
use rand::SeedableRng;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::time::{Instant, sleep};
use uuid::Uuid;

const DEFAULT_AMOUNT_WEI: &str = "1000000000000";

#[derive(Debug, Clone)]
struct Config {
    bridge_url: String,
    database_url: String,
    miden_rpc_url: String,
    miden_remote_prover_url: Option<String>,
    miden_remote_prover_timeout: Duration,
    evm_rpc_url: String,
    evm_chain_id: u64,
    evm_required_confirmations: u64,
    user_private_key: String,
    solver_private_key: String,
    amount_wei: String,
    work_dir: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EvidenceReport {
    generated_at: String,
    bridge_url: String,
    evm_rpc_url: String,
    evm_chain_id: u64,
    miden_rpc_url: String,
    amount_wei: String,
    solver_address: String,
    test_user_address: String,
    inbound: InboundEvidence,
    outbound: OutboundEvidence,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InboundEvidence {
    correlation_id: String,
    recipient_account_id: String,
    recipient_address: String,
    deposit_address: String,
    evm_deposit_tx_hash: String,
    miden_mint_tx_ids: Vec<String>,
    claim_tx_id: String,
    final_status: SwapStatus,
    lifecycle: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OutboundEvidence {
    funding_correlation_id: String,
    funding_deposit_address: String,
    funding_evm_deposit_tx_hash: String,
    funding_miden_mint_tx_ids: Vec<String>,
    funding_claim_tx_id: String,
    sender_account_id: String,
    sender_address: String,
    outbound_correlation_id: String,
    bridge_account_id: String,
    deposit_memo: String,
    quote_hash: String,
    bridge_out_note_tx_id: String,
    miden_consume_tx_ids: Vec<String>,
    evm_release_tx_hashes: Vec<String>,
    evm_recipient_balance_before_wei: String,
    evm_recipient_balance_after_wei: String,
    evm_recipient_balance_delta_wei: String,
    final_status: SwapStatus,
    lifecycle: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load()?;
    let user_signer: PrivateKeySigner = config
        .user_private_key
        .parse::<PrivateKeySigner>()
        .context("invalid DEMO_EVM_FUNDED_PRIVATE_KEY")?
        .with_chain_id(Some(config.evm_chain_id));
    let solver_signer: PrivateKeySigner = config
        .solver_private_key
        .parse::<PrivateKeySigner>()
        .context("invalid SOLVER_PRIVATE_KEY")?
        .with_chain_id(Some(config.evm_chain_id));

    let observed_chain_id = read_chain_id(&config).await?;
    ensure!(
        observed_chain_id == config.evm_chain_id,
        "configured EVM_CHAIN_ID={} but RPC returned {}",
        config.evm_chain_id,
        observed_chain_id
    );
    ensure_ready(&config.bridge_url).await?;

    let pool = connect_pool(&config.database_url, 5)
        .await
        .context("failed to connect to host Postgres; set LIVE_E2E_DATABASE_URL if needed")?;
    let store = PostgresStateStore::new(pool);
    let bootstrap_record = store
        .get_miden_bootstrap()
        .await?
        .ok_or_else(|| anyhow!("miden bootstrap record is missing"))?;
    let bootstrap = bootstrap_state_from_record(&bootstrap_record)?;

    let miden = MidenClient::new_with_remote_prover(
        &config.miden_rpc_url,
        &config.work_dir.join("miden-store"),
        config.miden_remote_prover_url.clone(),
        config.miden_remote_prover_timeout,
    )
    .await?;
    miden.sync_state().await?;

    let inbound_wallet = create_wallet(&miden, "sepolia-inbound-recipient").await?;
    let inbound_recipient = miden.encode_basic_wallet_address(inbound_wallet.id());
    let inbound_quote = make_quote(
        &config,
        "eth-sepolia:eth",
        "miden-testnet:eth",
        &config.amount_wei,
        &inbound_recipient,
        &user_signer.address().to_string(),
    )
    .await?;
    let inbound_deposit = inbound_quote
        .quote
        .deposit_address
        .clone()
        .ok_or_else(|| anyhow!("inbound quote missing deposit address"))?;
    let inbound_deposit_tx =
        send_native_eth(&config, user_signer.clone(), &inbound_deposit).await?;
    submit_deposit(&config, &inbound_deposit, &inbound_deposit_tx, None).await?;
    let inbound_status = poll_status_until(
        &config,
        &inbound_deposit,
        None,
        SwapStatus::Success,
        Duration::from_secs(600),
    )
    .await?;
    let inbound_correlation = Uuid::parse_str(&inbound_quote.correlation_id)?;
    let inbound_record = store
        .get_quote_by_correlation_id(inbound_correlation)
        .await?
        .ok_or_else(|| anyhow!("inbound quote record missing"))?;
    let inbound_claim = wait_and_consume_notes(&miden, &inbound_wallet, Duration::from_secs(240))
        .await?
        .ok_or_else(|| anyhow!("inbound claim did not consume a note"))?;
    let inbound_lifecycle = lifecycle_statuses(&store, inbound_correlation).await?;

    let outbound_sender = create_wallet(&miden, "sepolia-outbound-sender").await?;
    let outbound_sender_address = miden.encode_basic_wallet_address(outbound_sender.id());
    let funding_quote = make_quote(
        &config,
        "eth-sepolia:eth",
        "miden-testnet:eth",
        &config.amount_wei,
        &outbound_sender_address,
        &user_signer.address().to_string(),
    )
    .await?;
    let funding_deposit = funding_quote
        .quote
        .deposit_address
        .clone()
        .ok_or_else(|| anyhow!("funding quote missing deposit address"))?;
    let funding_deposit_tx =
        send_native_eth(&config, user_signer.clone(), &funding_deposit).await?;
    submit_deposit(&config, &funding_deposit, &funding_deposit_tx, None).await?;
    poll_status_until(
        &config,
        &funding_deposit,
        None,
        SwapStatus::Success,
        Duration::from_secs(600),
    )
    .await?;
    let funding_correlation = Uuid::parse_str(&funding_quote.correlation_id)?;
    let funding_record = store
        .get_quote_by_correlation_id(funding_correlation)
        .await?
        .ok_or_else(|| anyhow!("funding quote record missing"))?;
    let funding_claim = wait_and_consume_notes(&miden, &outbound_sender, Duration::from_secs(240))
        .await?
        .ok_or_else(|| anyhow!("funding claim did not consume a note"))?;

    let recipient = user_signer.address();
    let recipient_balance_before = evm_balance(&config, recipient).await?;
    let outbound_quote = make_quote(
        &config,
        "miden-testnet:eth",
        "eth-sepolia:eth",
        &config.amount_wei,
        &recipient.to_string(),
        &outbound_sender_address,
    )
    .await?;
    let bridge_account_id = outbound_quote
        .quote
        .deposit_address
        .clone()
        .ok_or_else(|| anyhow!("outbound quote missing bridge account id"))?;
    let deposit_memo = outbound_quote
        .quote
        .deposit_memo
        .clone()
        .ok_or_else(|| anyhow!("outbound quote missing deposit memo"))?;
    let memo = BridgeOutDepositMemo::from_deposit_memo(&deposit_memo)?;
    ensure!(
        parse_account_id(&memo.bridge_account_id)? == parse_account_id(&bridge_account_id)?,
        "deposit address and memo bridge account differ"
    );
    let bridge_note_tx = submit_bridge_out_note(
        &miden,
        &bootstrap,
        &outbound_sender,
        &bridge_account_id,
        &deposit_memo,
        config.amount_wei.parse::<u64>()?,
    )
    .await?;
    let outbound_status = poll_status_until(
        &config,
        &bridge_account_id,
        Some(&deposit_memo),
        SwapStatus::Success,
        Duration::from_secs(600),
    )
    .await?;
    let outbound_correlation = Uuid::parse_str(&outbound_quote.correlation_id)?;
    let outbound_record = store
        .get_quote_by_correlation_id(outbound_correlation)
        .await?
        .ok_or_else(|| anyhow!("outbound quote record missing"))?;
    let outbound_lifecycle = lifecycle_statuses(&store, outbound_correlation).await?;
    let recipient_balance_after = evm_balance(&config, recipient).await?;
    let balance_delta = recipient_balance_after.saturating_sub(recipient_balance_before);

    let report = EvidenceReport {
        generated_at: miden_testnet_bridge::now_iso8601(),
        bridge_url: config.bridge_url.clone(),
        evm_rpc_url: config.evm_rpc_url.clone(),
        evm_chain_id: config.evm_chain_id,
        miden_rpc_url: config.miden_rpc_url.clone(),
        amount_wei: config.amount_wei.clone(),
        solver_address: solver_signer.address().to_string(),
        test_user_address: user_signer.address().to_string(),
        inbound: InboundEvidence {
            correlation_id: inbound_quote.correlation_id.clone(),
            recipient_account_id: inbound_wallet.id().to_hex(),
            recipient_address: inbound_recipient,
            deposit_address: inbound_deposit,
            evm_deposit_tx_hash: inbound_deposit_tx,
            miden_mint_tx_ids: inbound_record.miden_mint_tx_ids,
            claim_tx_id: inbound_claim,
            final_status: inbound_status.status,
            lifecycle: inbound_lifecycle,
        },
        outbound: OutboundEvidence {
            funding_correlation_id: funding_quote.correlation_id.clone(),
            funding_deposit_address: funding_deposit,
            funding_evm_deposit_tx_hash: funding_deposit_tx,
            funding_miden_mint_tx_ids: funding_record.miden_mint_tx_ids,
            funding_claim_tx_id: funding_claim,
            sender_account_id: outbound_sender.id().to_hex(),
            sender_address: outbound_sender_address,
            outbound_correlation_id: outbound_quote.correlation_id.clone(),
            bridge_account_id,
            deposit_memo,
            quote_hash: memo.storage.quote_hash,
            bridge_out_note_tx_id: bridge_note_tx,
            miden_consume_tx_ids: outbound_record.miden_consume_tx_ids,
            evm_release_tx_hashes: outbound_record.evm_release_tx_hashes,
            evm_recipient_balance_before_wei: recipient_balance_before.to_string(),
            evm_recipient_balance_after_wei: recipient_balance_after.to_string(),
            evm_recipient_balance_delta_wei: balance_delta.to_string(),
            final_status: outbound_status.status,
            lifecycle: outbound_lifecycle,
        },
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    println!(
        "SEPOLIA_E2E_EVIDENCE inbound correlation_id={} evm_deposit_tx_hash={} miden_mint_tx_ids={:?} claim_tx_id={}",
        report.inbound.correlation_id,
        report.inbound.evm_deposit_tx_hash,
        report.inbound.miden_mint_tx_ids,
        report.inbound.claim_tx_id
    );
    println!(
        "SEPOLIA_E2E_EVIDENCE outbound funding_correlation_id={} outbound_correlation_id={} funding_evm_deposit_tx_hash={} bridge_out_note_tx_id={} miden_consume_tx_ids={:?} evm_release_tx_hashes={:?} balance_delta_wei={}",
        report.outbound.funding_correlation_id,
        report.outbound.outbound_correlation_id,
        report.outbound.funding_evm_deposit_tx_hash,
        report.outbound.bridge_out_note_tx_id,
        report.outbound.miden_consume_tx_ids,
        report.outbound.evm_release_tx_hashes,
        report.outbound.evm_recipient_balance_delta_wei
    );

    Ok(())
}

impl Config {
    fn load() -> Result<Self> {
        let file_env = read_env_file(".env").unwrap_or_default();
        let get = |name: &str| -> Option<String> {
            std::env::var(name)
                .ok()
                .or_else(|| file_env.get(name).cloned())
        };
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let work_dir = PathBuf::from(
            get("SEPOLIA_E2E_WORK_DIR").unwrap_or_else(|| format!(".live-sepolia-e2e/{now}")),
        );
        fs::create_dir_all(&work_dir)
            .with_context(|| format!("failed to create {}", work_dir.display()))?;

        Ok(Self {
            bridge_url: get("BRIDGE_URL").unwrap_or_else(|| "http://localhost:8080".to_owned()),
            database_url: get("LIVE_E2E_DATABASE_URL").unwrap_or_else(|| {
                "postgres://postgres:postgres@localhost:5432/miden_bridge".to_owned()
            }),
            miden_rpc_url: get("MIDEN_RPC_URL")
                .unwrap_or_else(|| "https://rpc.testnet.miden.io".to_owned()),
            miden_remote_prover_url: get("MIDEN_REMOTE_PROVER_URL")
                .filter(|value| !value.trim().is_empty()),
            miden_remote_prover_timeout: Duration::from_secs(
                get("MIDEN_REMOTE_PROVER_TIMEOUT_SECS")
                    .unwrap_or_else(|| "180".to_owned())
                    .parse()
                    .context("MIDEN_REMOTE_PROVER_TIMEOUT_SECS must be a u64")?,
            ),
            evm_rpc_url: get("EVM_RPC_URL").context("EVM_RPC_URL is required")?,
            evm_chain_id: get("EVM_CHAIN_ID")
                .unwrap_or_else(|| "11155111".to_owned())
                .parse()
                .context("EVM_CHAIN_ID must be a u64")?,
            evm_required_confirmations: get("EVM_REQUIRED_CONFIRMATIONS")
                .unwrap_or_else(|| "2".to_owned())
                .parse()
                .context("EVM_REQUIRED_CONFIRMATIONS must be a u64")?,
            user_private_key: get("DEMO_EVM_FUNDED_PRIVATE_KEY")
                .context("DEMO_EVM_FUNDED_PRIVATE_KEY is required")?,
            solver_private_key: get("SOLVER_PRIVATE_KEY")
                .context("SOLVER_PRIVATE_KEY is required")?,
            amount_wei: get("SEPOLIA_E2E_AMOUNT_WEI")
                .unwrap_or_else(|| DEFAULT_AMOUNT_WEI.to_owned()),
            work_dir,
        })
    }
}

fn read_env_file(path: &str) -> Result<BTreeMap<String, String>> {
    let contents = fs::read_to_string(path)?;
    let mut env = BTreeMap::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        env.insert(key.trim().to_owned(), value.trim().to_owned());
    }
    Ok(env)
}

async fn ensure_ready(bridge_url: &str) -> Result<()> {
    let client = reqwest::Client::new();
    for path in ["/healthz", "/readyz"] {
        let response = client
            .get(format!("{bridge_url}{path}"))
            .send()
            .await
            .with_context(|| format!("failed to call {path}"))?;
        ensure!(
            response.status().is_success(),
            "{path} returned {}",
            response.status()
        );
    }
    Ok(())
}

async fn create_wallet(miden: &MidenClient, label: &str) -> Result<Account> {
    let unique = Uuid::new_v4();
    let init_seed = seed32(&format!("{label}:{unique}:init"));
    let auth_seed = seed32(&format!("{label}:{unique}:auth"));
    let mut rng = rand::rngs::StdRng::from_seed(auth_seed);
    let secret_key = AuthSecretKey::new_falcon512_poseidon2_with_rng(&mut rng);
    let account = build_wallet_account(init_seed, &secret_key)?;
    let keystore = miden.open_keystore()?;
    let mut inner = miden.open().await?;
    keystore.add_key(&secret_key, account.id()).await?;
    inner.add_account(&account, false).await?;
    Ok(account)
}

async fn make_quote(
    config: &Config,
    origin_asset: &str,
    destination_asset: &str,
    amount: &str,
    recipient: &str,
    refund_to: &str,
) -> Result<QuoteResponse> {
    let request = QuoteRequest {
        dry: false,
        deposit_mode: Some(DepositMode::Simple),
        swap_type: SwapType::ExactInput,
        slippage_tolerance: 100.0,
        origin_asset: origin_asset.to_owned(),
        deposit_type: DepositType::OriginChain,
        destination_asset: destination_asset.to_owned(),
        amount: amount.to_owned(),
        refund_to: refund_to.to_owned(),
        refund_type: RefundType::OriginChain,
        recipient: recipient.to_owned(),
        connected_wallets: None,
        session_id: None,
        virtual_chain_recipient: None,
        virtual_chain_refund_recipient: None,
        custom_recipient_msg: None,
        recipient_type: RecipientType::DestinationChain,
        deadline: "2027-01-01T00:00:00Z".to_owned(),
        referral: Some("sepolia-live-e2e".to_owned()),
        quote_waiting_time_ms: None,
        app_fees: None,
    };
    let response = reqwest::Client::new()
        .post(format!("{}/v0/quote", config.bridge_url))
        .json(&request)
        .send()
        .await?;
    ensure!(
        response.status().is_success(),
        "quote failed with {}: {}",
        response.status(),
        response.text().await.unwrap_or_default()
    );
    response.json::<QuoteResponse>().await.map_err(Into::into)
}

async fn send_native_eth(config: &Config, signer: PrivateKeySigner, to: &str) -> Result<String> {
    let provider = ProviderBuilder::new()
        .wallet(signer)
        .connect_http(config.evm_rpc_url.parse()?);
    let amount = U256::from_str(&config.amount_wei)?;
    let tx = TransactionRequest::default()
        .with_to(Address::from_str(to)?)
        .with_value(amount);
    let mut last_error = None;
    let pending = 'send: {
        for attempt in 0..8 {
            match provider.send_transaction(tx.clone()).await {
                Ok(pending) => break 'send pending,
                Err(error) => {
                    last_error = Some(error);
                    sleep(retry_delay(attempt)).await;
                }
            }
        }
        return Err(last_error
            .map(anyhow::Error::from)
            .unwrap_or_else(|| anyhow!("send transaction failed without error")));
    };
    let tx_hash = format!("{:#x}", pending.tx_hash());
    wait_for_evm_confirmations(config, *pending.tx_hash()).await?;
    Ok(tx_hash)
}

async fn submit_deposit(
    config: &Config,
    deposit_address: &str,
    tx_hash: &str,
    memo: Option<&str>,
) -> Result<SubmitDepositTxResponse> {
    let request = SubmitDepositTxRequest {
        tx_hash: tx_hash.to_owned(),
        deposit_address: deposit_address.to_owned(),
        near_sender_account: None,
        memo: memo.map(str::to_owned),
    };
    let response = reqwest::Client::new()
        .post(format!("{}/v0/deposit/submit", config.bridge_url))
        .json(&request)
        .send()
        .await?;
    ensure!(
        response.status().is_success(),
        "deposit submit failed with {}: {}",
        response.status(),
        response.text().await.unwrap_or_default()
    );
    response
        .json::<SubmitDepositTxResponse>()
        .await
        .map_err(Into::into)
}

async fn poll_status_until(
    config: &Config,
    deposit_address: &str,
    deposit_memo: Option<&str>,
    target_status: SwapStatus,
    timeout: Duration,
) -> Result<StatusResponse> {
    let client = reqwest::Client::new();
    let deadline = Instant::now() + timeout;
    loop {
        let mut url = url::Url::parse(&format!("{}/v0/status", config.bridge_url))?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("depositAddress", deposit_address);
            if let Some(deposit_memo) = deposit_memo {
                query.append_pair("depositMemo", deposit_memo);
            }
        }
        let response = client.get(url).send().await?;
        ensure!(
            response.status().is_success(),
            "status failed with {}",
            response.status()
        );
        let status = response.json::<StatusResponse>().await?;
        if status.status == target_status {
            return Ok(status);
        }
        ensure!(
            Instant::now() < deadline,
            "timed out waiting for {:?}; last status {:?}",
            target_status,
            status.status
        );
        sleep(Duration::from_secs(3)).await;
    }
}

async fn wait_and_consume_notes(
    miden: &MidenClient,
    account: &Account,
    timeout: Duration,
) -> Result<Option<String>> {
    let deadline = Instant::now() + timeout;
    loop {
        let mut inner = miden.open().await?;
        sync_with_retry(&mut inner).await?;
        let notes: Vec<_> = inner
            .get_consumable_notes(Some(account.id()))
            .await?
            .into_iter()
            .filter_map(|(record, _)| record.try_into().ok())
            .collect();
        if notes.is_empty() {
            ensure!(
                Instant::now() < deadline,
                "timed out waiting for Miden note"
            );
            sleep(Duration::from_secs(3)).await;
            continue;
        }
        let request = TransactionRequestBuilder::new().build_consume_notes(notes)?;
        let tx_id = inner.submit_new_transaction(account.id(), request).await?;
        wait_for_tx(&mut inner, tx_id).await?;
        return Ok(Some(tx_id.to_string()));
    }
}

async fn submit_bridge_out_note(
    miden: &MidenClient,
    bootstrap: &miden_testnet_bridge::chains::miden_bootstrap::BootstrapState,
    sender: &Account,
    deposit_address: &str,
    deposit_memo: &str,
    amount: u64,
) -> Result<String> {
    let memo = BridgeOutDepositMemo::from_deposit_memo(deposit_memo)?;
    ensure!(
        parse_account_id(&memo.bridge_account_id)? == parse_account_id(deposit_address)?,
        "deposit address and memo bridge account differ"
    );
    let mut inner = miden.open().await?;
    sync_with_retry(&mut inner).await?;
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
    wait_for_tx(&mut inner, tx_id).await?;
    Ok(tx_id.to_string())
}

async fn evm_balance(config: &Config, address: Address) -> Result<U256> {
    let provider = ProviderBuilder::new().connect_http(config.evm_rpc_url.parse()?);
    let mut last_error = None;
    for attempt in 0..8 {
        match provider.get_balance(address).await {
            Ok(balance) => return Ok(balance),
            Err(error) => {
                last_error = Some(error);
                sleep(retry_delay(attempt)).await;
            }
        }
    }
    Err(last_error
        .map(anyhow::Error::from)
        .unwrap_or_else(|| anyhow!("balance read failed without error")))
}

async fn lifecycle_statuses(
    store: &PostgresStateStore,
    correlation_id: Uuid,
) -> Result<Vec<String>> {
    Ok(store
        .list_lifecycle_events(correlation_id)
        .await?
        .into_iter()
        .map(|event| event.to_status)
        .collect())
}

fn seed32(label: &str) -> [u8; 32] {
    Sha256::digest(label.as_bytes()).into()
}

async fn read_chain_id(config: &Config) -> Result<u64> {
    let provider = ProviderBuilder::new().connect_http(config.evm_rpc_url.parse()?);
    let mut last_error = None;
    for attempt in 0..8 {
        match provider.get_chain_id().await {
            Ok(chain_id) => return Ok(chain_id),
            Err(error) => {
                last_error = Some(error);
                sleep(retry_delay(attempt)).await;
            }
        }
    }
    Err(last_error
        .map(anyhow::Error::from)
        .unwrap_or_else(|| anyhow!("chain id read failed without error")))
}

async fn wait_for_evm_confirmations(config: &Config, tx_hash: B256) -> Result<()> {
    let provider = ProviderBuilder::new().connect_http(config.evm_rpc_url.parse()?);
    let deadline = Instant::now() + Duration::from_secs(300);
    let mut attempt = 0usize;
    loop {
        match provider.get_transaction_receipt(tx_hash).await {
            Ok(Some(receipt)) => {
                ensure!(receipt.status(), "transaction {tx_hash:#x} reverted");
                if let Some(block_number) = receipt.block_number {
                    let latest = read_block_number(config).await?;
                    if latest >= block_number.saturating_add(config.evm_required_confirmations) {
                        return Ok(());
                    }
                }
            }
            Ok(None) => {}
            Err(_) if attempt < 8 => {}
            Err(error) => return Err(error.into()),
        }
        ensure!(
            Instant::now() < deadline,
            "timed out waiting for Sepolia tx {tx_hash:#x} confirmations"
        );
        sleep(retry_delay(attempt.min(7))).await;
        attempt = attempt.saturating_add(1);
    }
}

async fn read_block_number(config: &Config) -> Result<u64> {
    let provider = ProviderBuilder::new().connect_http(config.evm_rpc_url.parse()?);
    let mut last_error = None;
    for attempt in 0..8 {
        match provider.get_block_number().await {
            Ok(block_number) => return Ok(block_number),
            Err(error) => {
                last_error = Some(error);
                sleep(retry_delay(attempt)).await;
            }
        }
    }
    Err(last_error
        .map(anyhow::Error::from)
        .unwrap_or_else(|| anyhow!("block number read failed without error")))
}

fn retry_delay(attempt: usize) -> Duration {
    Duration::from_secs(match attempt {
        0 => 2,
        1 => 4,
        2 => 8,
        3 => 12,
        4 => 20,
        _ => 30,
    })
}
