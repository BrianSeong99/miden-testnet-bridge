use std::{path::PathBuf, process::Command, str::FromStr, sync::Arc};

use alloy::{
    network::TransactionBuilder as _,
    primitives::{Address, U256},
    providers::{Provider, ProviderBuilder},
    rpc::types::eth::TransactionRequest,
    sol,
    sol_types::SolCall,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use miden_client::{
    account::Account,
    auth::AuthSecretKey,
    keystore::Keystore,
    transaction::{PaymentNoteDescription, TransactionRequestBuilder},
};
use miden_testnet_bridge::{
    AppState, app,
    chains::{
        evm::{EvmClient, EvmConfig, load_token_address_file},
        miden::MidenClient,
        miden_bootstrap::{bootstrap_miden, sync_with_retry, wait_for_tx},
        miden_deposit_account::{
            build_wallet_account, derive_auth_secret_key, derive_outbound_deposit_account,
            re_derive_outbound_deposit_account,
        },
        miden_inbound::mint_to_user,
        miden_outbound::{parse_persisted_miden_seed_hex, poll_outbound_deposits_once},
    },
    core::{lifecycle::DefaultLifecycle, pricer::MockPricer},
    test_support::memory_state,
};
use rand::SeedableRng;
use tempfile::{TempDir, tempdir};
use tower::ServiceExt;
use uuid::Uuid;

const DEFAULT_SOLVER_PRIVATE_KEY: &str =
    "0x59c6995e998f97a5a0044966f0945382dbb7d2745078b2336b91c60d50d6b6d7";
const DEFAULT_MASTER_MNEMONIC: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const TEST_MASTER_SEED: [u8; 32] = [7u8; 32];

sol! {
    function transfer(address to, uint256 amount) external returns (bool);
    function balanceOf(address owner) external view returns (uint256);
}

#[tokio::test]
async fn outbound_account_derivation_is_deterministic() {
    let correlation_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();

    let (first_account, first_key, first_init_seed, first_auth_seed) =
        derive_outbound_deposit_account(&TEST_MASTER_SEED, correlation_id).unwrap();
    let (second_account, second_key, second_init_seed, second_auth_seed) =
        derive_outbound_deposit_account(&TEST_MASTER_SEED, correlation_id).unwrap();

    assert_eq!(first_account.id(), second_account.id());
    assert_eq!(first_key, second_key);
    assert_eq!(first_init_seed, second_init_seed);
    assert_eq!(first_auth_seed, second_auth_seed);
}

#[tokio::test]
async fn outbound_account_restart_recovery_matches_persisted_seeds() {
    let correlation_id = Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap();
    let (account, secret_key, init_seed, auth_seed) =
        derive_outbound_deposit_account(&TEST_MASTER_SEED, correlation_id).unwrap();
    let persisted = format!(
        "{}:{}",
        alloy::hex::encode(init_seed),
        alloy::hex::encode(auth_seed)
    );

    let (persisted_init_seed, persisted_auth_seed) =
        parse_persisted_miden_seed_hex(&persisted).unwrap();
    let rebuilt_key = derive_auth_secret_key(persisted_auth_seed);
    let rebuilt_account = build_wallet_account(persisted_init_seed, &rebuilt_key).unwrap();
    let (rederived_account, rederived_key) =
        re_derive_outbound_deposit_account(&TEST_MASTER_SEED, correlation_id).unwrap();

    assert_eq!(rebuilt_account.id(), account.id());
    assert_eq!(rebuilt_key, secret_key);
    assert_eq!(rederived_account.id(), account.id());
    assert_eq!(rederived_key, secret_key);
}

#[tokio::test]
async fn bootstrap_is_idempotent() {
    let Some((miden, _temp_dir)) = live_miden_client().await else {
        eprintln!("skipping: MIDEN_RPC_URL is not set");
        return;
    };
    let store = memory_state();

    let first = bootstrap_miden(miden.as_ref(), store.clone(), &TEST_MASTER_SEED)
        .await
        .expect("first bootstrap");
    let second = bootstrap_miden(miden.as_ref(), store.clone(), &TEST_MASTER_SEED)
        .await
        .expect("second bootstrap");
    let persisted = store
        .get_miden_bootstrap()
        .await
        .expect("bootstrap state query")
        .expect("persisted bootstrap row");

    assert_eq!(first.solver_account_id, second.solver_account_id);
    assert_eq!(first.eth_faucet_account_id, second.eth_faucet_account_id);
    assert_eq!(
        persisted.solver_account_id,
        first.solver_account_id.to_hex()
    );
}

#[tokio::test]
async fn inbound_flow_mints_p2id_and_user_can_consume_it() {
    let Some((miden, _temp_dir)) = live_miden_client().await else {
        eprintln!("skipping: MIDEN_RPC_URL is not set");
        return;
    };
    let store = memory_state();
    let bootstrap = bootstrap_miden(miden.as_ref(), store, &TEST_MASTER_SEED)
        .await
        .expect("bootstrap");
    let user_wallet = create_wallet(miden.as_ref(), [9u8; 32], [10u8; 32])
        .await
        .expect("user wallet");

    let tx_id = mint_to_user(
        miden.as_ref(),
        bootstrap.solver_account_id,
        bootstrap.usdc_faucet_account_id,
        user_wallet.id(),
        1_000_000,
    )
    .await
    .expect("mint to user");
    assert!(!tx_id.to_string().is_empty());

    let mut inner = miden.open().await.expect("open miden client");
    sync_with_retry(&mut inner).await.expect("sync after mint");
    let consumable = inner
        .get_consumable_notes(Some(user_wallet.id()))
        .await
        .expect("consumable notes");
    assert!(
        !consumable.is_empty(),
        "expected minted note for user wallet"
    );

    consume_notes(&mut inner, user_wallet.id()).await;
    let balance = inner
        .account_reader(user_wallet.id())
        .get_balance(bootstrap.usdc_faucet_account_id)
        .await
        .expect("user balance");
    assert_eq!(balance, 1_000_000);
}

#[tokio::test]
async fn outbound_flow_consumes_note_and_releases_on_evm() {
    let Some((miden, _miden_dir)) = live_miden_client().await else {
        eprintln!("skipping: MIDEN_RPC_URL is not set");
        return;
    };
    let Some((evm, token_file, _evm_dir, store)) = live_evm_client().await else {
        eprintln!("skipping: EVM_RPC_URL is not set");
        return;
    };
    let bootstrap = bootstrap_miden(miden.as_ref(), store.clone(), &TEST_MASTER_SEED)
        .await
        .expect("bootstrap");

    let recipient = Address::from_str("0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc").unwrap();
    let app = app(AppState::with_clients(
        store.clone(),
        Arc::new(MockPricer),
        evm.clone(),
        miden.clone(),
        TEST_MASTER_SEED,
    ));
    let quote_response = app
        .oneshot(
            Request::post("/v0/quote")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "dry": false,
                        "depositMode": "SIMPLE",
                        "swapType": "EXACT_INPUT",
                        "slippageTolerance": 100.0,
                        "originAsset": "miden-local:usdc",
                        "depositType": "ORIGIN_CHAIN",
                        "destinationAsset": "eth-anvil:usdc",
                        "amount": "1000000",
                        "refundTo": "0xfeed",
                        "refundType": "ORIGIN_CHAIN",
                        "recipient": recipient.to_string(),
                        "recipientType": "DESTINATION_CHAIN",
                        "deadline": "2026-06-12T00:00:00Z"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("quote response");
    assert_eq!(quote_response.status(), StatusCode::OK);

    let quote = store
        .list_miden_tracked_quotes()
        .await
        .expect("tracked quotes")
        .into_iter()
        .next()
        .expect("tracked outbound quote");
    let deposit_account_id =
        miden_client::account::AccountId::from_hex(&quote.miden_deposit_account_id)
            .expect("deposit account id");

    let sender = create_wallet(miden.as_ref(), [11u8; 32], [12u8; 32])
        .await
        .expect("sender wallet");
    mint_to_user(
        miden.as_ref(),
        bootstrap.solver_account_id,
        bootstrap.usdc_faucet_account_id,
        sender.id(),
        1_000_000,
    )
    .await
    .expect("fund sender");

    let mut inner = miden.open().await.expect("open miden client");
    consume_notes(&mut inner, sender.id()).await;
    let send_request = TransactionRequestBuilder::new()
        .build_pay_to_id(
            PaymentNoteDescription::new(
                vec![
                    miden_client::asset::FungibleAsset::new(
                        bootstrap.usdc_faucet_account_id,
                        1_000_000,
                    )
                    .unwrap()
                    .into(),
                ],
                sender.id(),
                deposit_account_id,
            ),
            miden_client::note::NoteType::Private,
            inner.rng(),
        )
        .expect("pay to id request");
    let tx_id = inner
        .submit_new_transaction(sender.id(), send_request)
        .await
        .expect("submit pay to id");
    wait_for_tx(&mut inner, tx_id)
        .await
        .expect("wait pay to id");

    let token_file = load_token_address_file(&token_file).unwrap();
    let usdc = Address::from_str(token_file.usdc.as_deref().expect("USDC address")).unwrap();
    let balance_before =
        erc20_balance(&std::env::var("EVM_RPC_URL").unwrap(), usdc, recipient).await;
    let lifecycle = Arc::new(DefaultLifecycle::new(
        store.clone(),
        Arc::new(MockPricer),
        evm.clone(),
        miden.clone(),
    ));
    poll_outbound_deposits_once(
        miden.clone(),
        store.clone(),
        evm.clone(),
        TEST_MASTER_SEED,
        lifecycle,
    )
    .await
    .expect("poll outbound deposits");
    let balance_after =
        erc20_balance(&std::env::var("EVM_RPC_URL").unwrap(), usdc, recipient).await;
    let updated_quote = store
        .get_quote_by_correlation_id(quote.correlation_id)
        .await
        .expect("updated quote")
        .expect("quote record");

    assert_eq!(updated_quote.status, "SUCCESS");
    assert_eq!(updated_quote.miden_consume_tx_ids.len(), 1);
    assert!(!updated_quote.evm_release_tx_hashes.is_empty());
    assert_eq!(balance_after - balance_before, U256::from(1_000_000u64));
}

async fn live_miden_client() -> Option<(Arc<MidenClient>, TempDir)> {
    let rpc_url = std::env::var("MIDEN_RPC_URL").ok()?;
    let temp_dir = tempdir().expect("tempdir");
    let client = Arc::new(
        MidenClient::new(&rpc_url, temp_dir.path())
            .await
            .expect("miden client"),
    );
    client.sync_state().await.expect("initial sync");
    Some((client, temp_dir))
}

async fn live_evm_client() -> Option<(
    Arc<EvmClient>,
    PathBuf,
    TempDir,
    miden_testnet_bridge::core::state::DynStateStore,
)> {
    let rpc_url = std::env::var("EVM_RPC_URL").ok()?;
    let temp_dir = tempdir().expect("tempdir");
    bootstrap_anvil(&rpc_url, temp_dir.path()).await;
    let token_file = temp_dir.path().join("token-addresses.json");
    let store = memory_state();
    let evm = Arc::new(
        EvmClient::new(
            store.clone(),
            EvmConfig {
                rpc_url,
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
    Some((evm, token_file, temp_dir, store))
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

async fn create_wallet(
    client: &MidenClient,
    init_seed: [u8; 32],
    auth_seed: [u8; 32],
) -> anyhow::Result<Account> {
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
) {
    sync_with_retry(inner).await.expect("sync before consume");
    let notes: Vec<_> = inner
        .get_consumable_notes(Some(account_id))
        .await
        .expect("consumable notes")
        .into_iter()
        .filter_map(|(record, _)| record.try_into().ok())
        .collect();
    if notes.is_empty() {
        return;
    }
    let request = TransactionRequestBuilder::new()
        .build_consume_notes(notes)
        .expect("consume notes request");
    let tx_id = inner
        .submit_new_transaction(account_id, request)
        .await
        .expect("submit consume");
    wait_for_tx(inner, tx_id).await.expect("wait consume");
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
