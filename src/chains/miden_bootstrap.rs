use anyhow::{Context, Result, anyhow};
use miden_client::keystore::Keystore;
use miden_client::{
    account::component::{AuthControlled, BasicFungibleFaucet},
    account::{Account, AccountBuilder, AccountId, AccountStorageMode, AccountType},
    asset::{FungibleAsset, TokenSymbol},
    auth::{AuthSchemeId, AuthSecretKey, AuthSingleSig},
    note::NoteType,
    store::TransactionFilter,
    transaction::{TransactionRequestBuilder, TransactionStatus},
};
use miden_protocol::Felt;
use miden_standards::account::metadata::AccountBuilderSchemaCommitmentExt;
use rand::{RngCore, SeedableRng, rngs::StdRng};

use crate::{
    chains::miden::{MidenClient, asset_decimals, solver_liquidity_for_asset},
    chains::miden_deposit_account::{build_wallet_account, derive_seed},
    core::state::{DynStateStore, MidenBootstrapRecord},
};

const SUPPORTED_ASSETS: [(&str, &str); 4] = [
    ("miden-local:eth", "ETH"),
    ("miden-local:usdc", "USDC"),
    ("miden-local:usdt", "USDT"),
    ("miden-local:btc", "BTC"),
];

#[derive(Clone, Debug)]
pub struct BootstrapState {
    pub solver_account_id: AccountId,
    pub eth_faucet_account_id: AccountId,
    pub usdc_faucet_account_id: AccountId,
    pub usdt_faucet_account_id: AccountId,
    pub btc_faucet_account_id: AccountId,
}

impl BootstrapState {
    pub fn faucet_id_for_asset(&self, asset_id: &str) -> Result<AccountId> {
        match asset_id {
            "miden-local:eth" | "eth-anvil:eth" => Ok(self.eth_faucet_account_id),
            "miden-local:usdc" | "eth-anvil:usdc" => Ok(self.usdc_faucet_account_id),
            "miden-local:usdt" | "eth-anvil:usdt" => Ok(self.usdt_faucet_account_id),
            "miden-local:btc" | "eth-anvil:btc" => Ok(self.btc_faucet_account_id),
            _ => Err(anyhow!("unsupported asset id {asset_id}")),
        }
    }
}

pub async fn deploy_faucet(
    client: &MidenClient,
    asset_symbol: &str,
    decimals: u8,
    max_supply: u64,
) -> Result<Account> {
    let keystore = client.open_keystore()?;
    let mut inner = client.open().await?;

    let mut init_seed = [0u8; 32];
    inner.rng().fill_bytes(&mut init_seed);
    let secret_key = AuthSecretKey::new_falcon512_poseidon2_with_rng(inner.rng());
    let account = AccountBuilder::new(init_seed)
        .account_type(AccountType::FungibleFaucet)
        .storage_mode(AccountStorageMode::Private)
        .with_auth_component(AuthSingleSig::new(
            secret_key.public_key().to_commitment(),
            AuthSchemeId::Falcon512Poseidon2,
        ))
        .with_component(BasicFungibleFaucet::new(
            TokenSymbol::new(asset_symbol)?,
            decimals,
            Felt::new(max_supply),
        )?)
        .with_component(AuthControlled::allow_all())
        .build_with_schema_commitment()?;

    keystore
        .add_key(&secret_key, account.id())
        .await
        .context("failed to persist faucet signing key")?;
    inner
        .add_account(&account, false)
        .await
        .context("failed to add faucet account to client store")?;

    Ok(account)
}

pub async fn create_solver_wallet(client: &MidenClient, master_seed: &[u8; 32]) -> Result<Account> {
    let keystore = client.open_keystore()?;
    let mut inner = client.open().await?;

    let init_seed = derive_seed(master_seed, uuid::Uuid::nil(), "miden_solver_account_seed");
    let auth_seed = derive_seed(master_seed, uuid::Uuid::nil(), "miden_solver_auth_key");
    let mut rng = StdRng::from_seed(auth_seed);
    let secret_key = AuthSecretKey::new_falcon512_poseidon2_with_rng(&mut rng);
    let account = build_wallet_account(init_seed, &secret_key)?;

    if inner.get_account(account.id()).await?.is_none() {
        keystore
            .add_key(&secret_key, account.id())
            .await
            .context("failed to persist solver signing key")?;
        inner
            .add_account(&account, false)
            .await
            .context("failed to add solver account to client store")?;
    }

    Ok(account)
}

pub async fn bootstrap_miden(
    client: &MidenClient,
    state_store: DynStateStore,
    master_seed: &[u8; 32],
) -> Result<BootstrapState> {
    if let Some(existing) = state_store.get_miden_bootstrap().await? {
        let state = bootstrap_state_from_record(&existing)?;
        ensure_solver_liquidity(client, &state).await?;
        return Ok(state);
    }

    let solver = create_solver_wallet(client, master_seed).await?;
    let mut faucet_ids = Vec::new();
    for (asset_id, symbol) in SUPPORTED_ASSETS {
        let faucet = deploy_faucet(client, symbol, asset_decimals(asset_id)?, u64::MAX)
            .await
            .with_context(|| format!("failed to deploy {symbol} faucet"))?;
        faucet_ids.push(faucet.id());
    }

    let record = MidenBootstrapRecord {
        solver_account_id: solver.id().to_hex(),
        eth_faucet_account_id: faucet_ids[0].to_hex(),
        usdc_faucet_account_id: faucet_ids[1].to_hex(),
        usdt_faucet_account_id: faucet_ids[2].to_hex(),
        btc_faucet_account_id: faucet_ids[3].to_hex(),
    };
    state_store
        .upsert_miden_bootstrap(&record)
        .await
        .context("failed to persist miden bootstrap state")?;

    let state = bootstrap_state_from_record(&record)?;
    ensure_solver_liquidity(client, &state).await?;
    Ok(state)
}

pub fn bootstrap_state_from_record(record: &MidenBootstrapRecord) -> Result<BootstrapState> {
    Ok(BootstrapState {
        solver_account_id: AccountId::from_hex(&record.solver_account_id)?,
        eth_faucet_account_id: AccountId::from_hex(&record.eth_faucet_account_id)?,
        usdc_faucet_account_id: AccountId::from_hex(&record.usdc_faucet_account_id)?,
        usdt_faucet_account_id: AccountId::from_hex(&record.usdt_faucet_account_id)?,
        btc_faucet_account_id: AccountId::from_hex(&record.btc_faucet_account_id)?,
    })
}

async fn ensure_solver_liquidity(client: &MidenClient, state: &BootstrapState) -> Result<()> {
    let mut inner = client.open().await?;
    sync_with_retry(&mut inner).await?;
    consume_all_notes(&mut inner, state.solver_account_id).await?;

    for asset_id in SUPPORTED_ASSETS.map(|(asset_id, _)| asset_id) {
        let faucet_id = state.faucet_id_for_asset(asset_id)?;
        let target = solver_liquidity_for_asset(asset_id)?;
        let current = inner
            .account_reader(state.solver_account_id)
            .get_balance(faucet_id)
            .await
            .with_context(|| format!("failed to read solver balance for {asset_id}"))?;

        if current >= target {
            continue;
        }

        let mint_request = TransactionRequestBuilder::new().build_mint_fungible_asset(
            FungibleAsset::new(faucet_id, target - current)?,
            state.solver_account_id,
            NoteType::Private,
            inner.rng(),
        )?;
        let tx_id = inner
            .submit_new_transaction(faucet_id, mint_request)
            .await
            .with_context(|| format!("failed to mint bootstrap liquidity for {asset_id}"))?;
        wait_for_tx(&mut inner, tx_id).await?;
        consume_all_notes(&mut inner, state.solver_account_id).await?;
    }

    Ok(())
}

async fn consume_all_notes(
    inner: &mut miden_client::Client<miden_client::keystore::FilesystemKeyStore>,
    account_id: AccountId,
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

    let tx_request = TransactionRequestBuilder::new().build_consume_notes(notes)?;
    let tx_id = inner
        .submit_new_transaction(account_id, tx_request)
        .await
        .context("failed to consume bootstrap notes into solver wallet")?;
    wait_for_tx(inner, tx_id).await
}

pub async fn sync_with_retry(
    inner: &mut miden_client::Client<miden_client::keystore::FilesystemKeyStore>,
) -> Result<()> {
    for attempt in 0..5u32 {
        match inner.sync_state().await {
            Ok(_) => return Ok(()),
            Err(_err) if attempt < 4 => tokio::time::sleep(std::time::Duration::from_secs(2)).await,
            Err(err) => return Err(err).context("miden sync failed after retries"),
        }
    }

    unreachable!("retry loop must return")
}

pub async fn wait_for_tx(
    inner: &mut miden_client::Client<miden_client::keystore::FilesystemKeyStore>,
    tx_id: miden_client::transaction::TransactionId,
) -> Result<()> {
    loop {
        sync_with_retry(inner).await?;
        let tx = inner
            .get_transactions(TransactionFilter::Ids(vec![tx_id]))
            .await?
            .pop()
            .ok_or_else(|| anyhow!("transaction {tx_id} not found after submission"))?;

        match tx.status {
            TransactionStatus::Committed { .. } => return Ok(()),
            TransactionStatus::Pending => {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            TransactionStatus::Discarded(cause) => {
                return Err(anyhow!("transaction {tx_id} was discarded: {cause:?}"));
            }
        }
    }
}
