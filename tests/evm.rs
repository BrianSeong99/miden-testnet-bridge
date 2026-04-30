use std::{path::PathBuf, process::Command, str::FromStr, sync::Arc, time::Duration};

use alloy::{
    network::TransactionBuilder as _,
    primitives::{Address, B256, U256},
    providers::{Provider, ProviderBuilder},
    rpc::types::eth::TransactionRequest,
    signers::local::PrivateKeySigner,
    sol,
    sol_types::SolCall,
};
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use miden_testnet_bridge::{
    AppState, app,
    chains::evm::{
        EvmAsset, EvmClient, EvmConfig, derivation_path, derive_address_from_mnemonic,
        load_token_address_file,
    },
    test_support::memory_state,
    types::{QuoteResponse, StatusResponse},
};
use tempfile::TempDir;
use tower::ServiceExt;
use uuid::Uuid;

const DEFAULT_MASTER_MNEMONIC: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const DEFAULT_SOLVER_PRIVATE_KEY: &str =
    "0x59c6995e998f97a5a0044966f0945382dbb7d2745078b2336b91c60d50d6b6d7";
const DEFAULT_FUNDED_PRIVATE_KEY: &str =
    "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

sol! {
    function transfer(address to, uint256 amount) external returns (bool);
    function balanceOf(address owner) external view returns (uint256);
}

struct LiveHarness {
    rpc_url: String,
    token_file: PathBuf,
    app: axum::Router,
    store: miden_testnet_bridge::core::state::DynStateStore,
    evm: Arc<EvmClient>,
    _temp_dir: TempDir,
}

impl LiveHarness {
    async fn start() -> Option<Self> {
        let rpc_url = std::env::var("EVM_RPC_URL").ok()?;
        let temp_dir = TempDir::new().expect("temp dir");
        bootstrap_anvil(&rpc_url, temp_dir.path()).await;
        let token_file = temp_dir.path().join("token-addresses.json");

        let store = memory_state();
        let evm = Arc::new(
            EvmClient::new(
                store.clone(),
                EvmConfig {
                    rpc_url: rpc_url.clone(),
                    master_mnemonic: DEFAULT_MASTER_MNEMONIC.to_owned(),
                    solver_private_key: std::env::var("SOLVER_PRIVATE_KEY")
                        .unwrap_or_else(|_| DEFAULT_SOLVER_PRIVATE_KEY.to_owned()),
                    token_addresses_path: token_file.clone(),
                    chain_id: std::env::var("EVM_CHAIN_ID")
                        .ok()
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(271828),
                },
            )
            .expect("EVM client"),
        );
        tokio::spawn(evm.clone().watch_deposits());

        let app = app(AppState::with_evm(store.clone(), evm.clone()));

        Some(Self {
            rpc_url,
            token_file,
            app,
            store,
            evm,
            _temp_dir: temp_dir,
        })
    }

    async fn create_quote(&self, origin_asset: &str) -> QuoteResponse {
        let response = self
            .app
            .clone()
            .oneshot(
                Request::post("/v0/quote")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "dry": false,
                            "depositMode": "SIMPLE",
                            "swapType": "EXACT_INPUT",
                            "slippageTolerance": 100.0,
                            "originAsset": origin_asset,
                            "depositType": "ORIGIN_CHAIN",
                            "destinationAsset": "miden-local:eth",
                            "amount": "1000000",
                            "refundTo": "0xfeed",
                            "refundType": "ORIGIN_CHAIN",
                            "recipient": "recipient",
                            "recipientType": "DESTINATION_CHAIN",
                            "deadline": "2026-06-12T00:00:00Z"
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("quote response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("quote body");
        serde_json::from_slice(&body).expect("quote json")
    }

    async fn status(&self, deposit_address: &str) -> StatusResponse {
        let response = self
            .app
            .clone()
            .oneshot(
                Request::get(format!("/v0/status?depositAddress={deposit_address}"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("status response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("status body");
        serde_json::from_slice(&body).expect("status json")
    }
}

#[tokio::test]
async fn hd_derivation_is_deterministic_for_same_correlation_id() {
    let correlation_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
    let first = derive_address_from_mnemonic(DEFAULT_MASTER_MNEMONIC, correlation_id).unwrap();
    let second = derive_address_from_mnemonic(DEFAULT_MASTER_MNEMONIC, correlation_id).unwrap();

    assert_eq!(first, second);
    assert_eq!(
        derivation_path(correlation_id),
        derivation_path(correlation_id)
    );
}

#[tokio::test]
async fn detects_native_eth_and_advances_to_success() {
    let Some(harness) = LiveHarness::start().await else {
        eprintln!("skipping: EVM_RPC_URL is not set");
        return;
    };
    let quote = harness.create_quote("eth-anvil:eth").await;
    let deposit_address = quote
        .quote
        .deposit_address
        .clone()
        .expect("deposit address");

    send_native_eth(&harness.rpc_url, &deposit_address, U256::from(1_000_000u64)).await;

    let (statuses, final_status) = wait_for_success(&harness, &deposit_address).await;
    assert!(statuses.iter().any(|status| status == "KNOWN_DEPOSIT_TX"));
    assert!(statuses.iter().any(|status| status == "PENDING_DEPOSIT"));
    assert!(statuses.iter().any(|status| status == "PROCESSING"));
    assert_eq!(
        final_status.status,
        miden_testnet_bridge::types::SwapStatus::Success
    );
    assert_eq!(final_status.swap_details.origin_chain_tx_hashes.len(), 1);
}

#[tokio::test]
async fn detects_usdc_transfer_and_release_paths_work() {
    let Some(harness) = LiveHarness::start().await else {
        eprintln!("skipping: EVM_RPC_URL is not set");
        return;
    };
    let quote = harness.create_quote("eth-anvil:usdc").await;
    let deposit_address = Address::from_str(
        quote
            .quote
            .deposit_address
            .as_deref()
            .expect("deposit address"),
    )
    .unwrap();

    let token_file = load_token_address_file(&harness.token_file).unwrap();
    let usdc = Address::from_str(token_file.usdc.as_deref().expect("USDC address")).unwrap();
    send_erc20(
        &harness.rpc_url,
        usdc,
        deposit_address,
        U256::from(1_000_000u64),
    )
    .await;

    let (_, final_status) =
        wait_for_success(&harness, quote.quote.deposit_address.as_deref().unwrap()).await;
    assert_eq!(
        final_status.status,
        miden_testnet_bridge::types::SwapStatus::Success
    );

    let release_quote = harness.create_quote("eth-anvil:eth").await;
    let release_correlation_id = Uuid::parse_str(&release_quote.correlation_id).unwrap();
    let recipient = Address::from_str("0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc").unwrap();
    let eth_before = balance(&harness.rpc_url, recipient).await;
    harness
        .evm
        .release(
            release_correlation_id,
            recipient,
            EvmAsset::NativeEth,
            U256::from(12345u64),
        )
        .await
        .expect("ETH release");
    let eth_after = balance(&harness.rpc_url, recipient).await;
    assert_eq!(eth_after - eth_before, U256::from(12345u64));

    let usdc_before = erc20_balance(&harness.rpc_url, usdc, recipient).await;
    harness
        .evm
        .release(
            release_correlation_id,
            recipient,
            EvmAsset::Erc20(usdc),
            U256::from(54321u64),
        )
        .await
        .expect("USDC release");
    let usdc_after = erc20_balance(&harness.rpc_url, usdc, recipient).await;
    assert_eq!(usdc_after - usdc_before, U256::from(54321u64));
}

#[tokio::test]
async fn deposit_detection_is_idempotent_for_same_tx_hash() {
    let Some(harness) = LiveHarness::start().await else {
        eprintln!("skipping: EVM_RPC_URL is not set");
        return;
    };
    let quote = harness.create_quote("eth-anvil:eth").await;
    let deposit_address = quote.quote.deposit_address.clone().unwrap();
    let record = harness
        .store
        .list_evm_tracked_quotes()
        .await
        .expect("tracked quotes")
        .into_iter()
        .find(|quote| quote.deposit_address == deposit_address)
        .expect("tracked quote");
    let tx_hash =
        B256::from_str("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            .unwrap();

    let first = harness
        .evm
        .record_detected_deposit(&record, tx_hash, 1)
        .await
        .expect("first detection");
    let second = harness
        .evm
        .record_detected_deposit(&record, tx_hash, 1)
        .await
        .expect("second detection");

    let stored = harness
        .store
        .get_quote_by_deposit(&deposit_address, None)
        .await
        .expect("stored quote")
        .expect("quote record");
    assert!(first);
    assert!(!second);
    assert_eq!(stored.evm_deposit_tx_hashes.len(), 1);
    assert_eq!(stored.status, "KNOWN_DEPOSIT_TX");
}

async fn bootstrap_anvil(rpc_url: &str, state_dir: &std::path::Path) {
    let status = Command::new("bash")
        .arg("scripts/anvil_bootstrap.sh")
        .env("RPC_URL", rpc_url)
        .env("STATE_DIR", state_dir)
        .env(
            "SOLVER_PRIVATE_KEY",
            std::env::var("SOLVER_PRIVATE_KEY")
                .unwrap_or_else(|_| DEFAULT_SOLVER_PRIVATE_KEY.to_owned()),
        )
        .env("PROJECT_ROOT", std::env::current_dir().unwrap())
        .status()
        .expect("bootstrap command");
    assert!(status.success(), "bootstrap script failed");
}

async fn send_native_eth(rpc_url: &str, to: &str, amount: U256) {
    let signer: PrivateKeySigner = DEFAULT_FUNDED_PRIVATE_KEY.parse().unwrap();
    let provider = ProviderBuilder::new()
        .wallet(signer)
        .connect_http(rpc_url.parse().unwrap());
    provider
        .send_transaction(
            TransactionRequest::default()
                .with_to(Address::from_str(to).unwrap())
                .with_value(amount),
        )
        .await
        .unwrap()
        .watch()
        .await
        .unwrap();
}

async fn send_erc20(rpc_url: &str, token: Address, to: Address, amount: U256) {
    let signer: PrivateKeySigner = DEFAULT_FUNDED_PRIVATE_KEY.parse().unwrap();
    let provider = ProviderBuilder::new()
        .wallet(signer)
        .connect_http(rpc_url.parse().unwrap());
    provider
        .send_transaction(
            TransactionRequest::default()
                .with_to(token)
                .with_input(transferCall::new((to, amount)).abi_encode()),
        )
        .await
        .unwrap()
        .watch()
        .await
        .unwrap();
}

async fn balance(rpc_url: &str, address: Address) -> U256 {
    ProviderBuilder::new()
        .connect_http(rpc_url.parse().unwrap())
        .get_balance(address)
        .await
        .unwrap()
}

async fn erc20_balance(rpc_url: &str, token: Address, owner: Address) -> U256 {
    let provider = ProviderBuilder::new().connect_http(rpc_url.parse().unwrap());
    let result = provider
        .call(
            TransactionRequest::default()
                .with_to(token)
                .with_input(balanceOfCall::new((owner,)).abi_encode()),
        )
        .await
        .unwrap();
    balanceOfCall::abi_decode_returns(&result).unwrap()
}

async fn wait_for_success(
    harness: &LiveHarness,
    deposit_address: &str,
) -> (Vec<String>, StatusResponse) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let mut statuses = Vec::new();
    loop {
        let status = harness.status(deposit_address).await;
        let current = harness
            .store
            .get_quote_by_deposit(deposit_address, None)
            .await
            .expect("stored quote")
            .expect("quote record")
            .status;
        if statuses.last() != Some(&current) {
            statuses.push(current);
        }
        if status.status == miden_testnet_bridge::types::SwapStatus::Success {
            return (statuses, status);
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for success"
        );
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
}
