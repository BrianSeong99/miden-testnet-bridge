use std::{
    collections::HashMap,
    env,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use alloy::{
    consensus::Transaction as _,
    network::{TransactionBuilder as _, TransactionResponse as _},
    primitives::{Address, B256, U256},
    providers::{DynProvider, Provider, ProviderBuilder},
    rpc::types::eth::{BlockNumberOrTag, Filter, TransactionRequest},
    signers::{
        Signer,
        local::{MnemonicBuilder, PrivateKeySigner, coins_bip39::English},
    },
    sol,
    sol_types::{SolCall, SolEvent},
};
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::time::interval;
use tracing::{error, warn};
use url::Url;
use uuid::Uuid;

use crate::core::state::{DynStateStore, EvmTrackedQuote, TxHashColumn};

sol! {
    event Transfer(address indexed from, address indexed to, uint256 value);
    function transfer(address to, uint256 amount) external returns (bool);
    function balanceOf(address owner) external view returns (uint256);
}

#[derive(Clone, Debug)]
pub struct EvmConfig {
    pub rpc_url: String,
    pub master_mnemonic: String,
    pub solver_private_key: String,
    pub token_addresses_path: PathBuf,
    pub chain_id: u64,
}

#[derive(Clone)]
pub struct EvmClient {
    provider: DynProvider,
    signer_provider: DynProvider,
    store: DynStateStore,
    master_mnemonic: String,
    token_contracts: TokenContracts,
}

#[derive(Clone, Debug)]
pub enum EvmAsset {
    NativeEth,
    Erc20(Address),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TokenAddressFile {
    pub usdc: Option<String>,
    pub usdt: Option<String>,
    pub btc: Option<String>,
}

#[derive(Clone, Debug)]
struct TokenContracts {
    by_asset_id: HashMap<String, Address>,
}

impl EvmClient {
    pub fn new(store: DynStateStore, config: EvmConfig) -> Result<Self> {
        let url = Url::parse(&config.rpc_url).context("invalid EVM_RPC_URL")?;
        let provider = ProviderBuilder::new().connect_http(url.clone()).erased();
        let signer: PrivateKeySigner = config
            .solver_private_key
            .parse()
            .context("invalid SOLVER_PRIVATE_KEY")?;
        let signer_provider = ProviderBuilder::new()
            .wallet(signer.with_chain_id(Some(config.chain_id)))
            .connect_http(url)
            .erased();

        Ok(Self {
            provider,
            signer_provider,
            store,
            master_mnemonic: config.master_mnemonic,
            token_contracts: TokenContracts::load(&config.token_addresses_path)?,
        })
    }

    pub async fn derive_deposit_address(&self, correlation_id: Uuid) -> Result<(Address, String)> {
        let path = derivation_path(correlation_id);
        let address = derive_address_from_mnemonic(&self.master_mnemonic, correlation_id)?;
        Ok((address, path))
    }

    pub async fn persist_deposit_derivation_path(
        &self,
        correlation_id: Uuid,
        derivation_path: &str,
    ) -> Result<()> {
        self.store
            .set_evm_deposit_derivation_path(correlation_id, derivation_path)
            .await
            .context("failed to persist EVM derivation path")
    }

    pub async fn watch_deposits(self: Arc<Self>) {
        let mut poller = interval(Duration::from_secs(2));
        loop {
            poller.tick().await;
            if let Err(error) = self.poll_once().await {
                error!(?error, "EVM deposit poll failed");
            }
        }
    }

    pub async fn release(
        &self,
        correlation_id: Uuid,
        to: Address,
        asset: EvmAsset,
        amount: U256,
    ) -> Result<B256> {
        let tx = match asset {
            EvmAsset::NativeEth => TransactionRequest::default().with_to(to).with_value(amount),
            EvmAsset::Erc20(token) => TransactionRequest::default()
                .with_to(token)
                .with_input(transferCall::new((to, amount)).abi_encode()),
        };

        let pending = self
            .signer_provider
            .send_transaction(tx)
            .await
            .context("failed to send EVM release transaction")?;
        let tx_hash = *pending.tx_hash();
        let idempotency_key = format!("evm_release_{tx_hash:#x}");
        if self
            .store
            .record_idempotency_key(correlation_id, &idempotency_key)
            .await
            .context("failed to persist EVM release idempotency key")?
        {
            self.store
                .append_tx_hash(
                    correlation_id,
                    TxHashColumn::EvmReleaseTxHashes,
                    &format!("{tx_hash:#x}"),
                )
                .await
                .context("failed to persist EVM release tx hash")?;
        }
        pending
            .with_required_confirmations(1)
            .watch()
            .await
            .context("failed waiting for EVM release confirmation")?;
        Ok(tx_hash)
    }

    pub fn token_address(&self, asset_id: &str) -> Option<Address> {
        self.token_contracts.by_asset_id.get(asset_id).copied()
    }

    pub async fn record_detected_deposit(
        &self,
        quote: &EvmTrackedQuote,
        tx_hash: B256,
        detected_block: u64,
    ) -> Result<bool> {
        let tx_hash_string = format!("{tx_hash:#x}");
        let idempotency_key = format!("evm_deposit_{tx_hash_string}");
        if !self
            .store
            .record_idempotency_key(quote.correlation_id, &idempotency_key)
            .await
            .context("failed to record EVM deposit idempotency key")?
        {
            return Ok(false);
        }

        self.store
            .append_tx_hash(
                quote.correlation_id,
                TxHashColumn::EvmDepositTxHashes,
                &tx_hash_string,
            )
            .await
            .context("failed to persist EVM deposit tx hash")?;
        self.store
            .record_event(
                quote.correlation_id,
                Some("PENDING_DEPOSIT"),
                "KNOWN_DEPOSIT_TX",
                "EVM_DEPOSIT_DETECTED",
                None,
                Some(json!({
                    "txHash": tx_hash_string,
                    "detectedBlock": detected_block,
                })),
            )
            .await
            .context("failed to record EVM deposit detection")?;
        Ok(true)
    }

    async fn poll_once(&self) -> Result<()> {
        let latest_block = self
            .provider
            .get_block_number()
            .await
            .context("failed to read EVM block tip")?;
        let tracked_quotes = self
            .store
            .list_evm_tracked_quotes()
            .await
            .context("failed to list EVM tracked quotes")?;

        for quote in tracked_quotes {
            if !quote.origin_asset.starts_with("eth-anvil:") {
                continue;
            }

            match quote.status.as_str() {
                "PENDING_DEPOSIT" => {
                    if let Some((tx_hash, detected_block)) =
                        self.detect_deposit(&quote, latest_block).await?
                    {
                        self.handle_detected_deposit(&quote, tx_hash, detected_block)
                            .await?;
                    }
                }
                "KNOWN_DEPOSIT_TX" => {
                    self.handle_confirmed_deposit(&quote, latest_block).await?;
                }
                "PROCESSING" => {
                    self.complete_processing(&quote).await?;
                }
                _ => {}
            }
        }

        Ok(())
    }

    async fn detect_deposit(
        &self,
        quote: &EvmTrackedQuote,
        latest_block: u64,
    ) -> Result<Option<(B256, u64)>> {
        let deposit_address = Address::from_str(&quote.deposit_address)
            .with_context(|| format!("invalid deposit address {}", quote.deposit_address))?;
        let threshold = U256::from_str(&quote.amount_in)
            .with_context(|| format!("invalid quote amount {}", quote.amount_in))?;

        if quote.origin_asset == "eth-anvil:eth" {
            let balance = self
                .provider
                .get_balance(deposit_address)
                .await
                .context("failed to read deposit balance")?;
            if balance < threshold {
                return Ok(None);
            }
            return self
                .find_native_transfer(deposit_address, threshold, latest_block)
                .await;
        }

        let token = match self.token_address(&quote.origin_asset) {
            Some(token) => token,
            None => {
                warn!(asset = %quote.origin_asset, "missing token address for EVM asset");
                return Ok(None);
            }
        };
        let filter = Filter::new()
            .address(token)
            .from_block(0u64)
            .to_block(latest_block)
            .event_signature(Transfer::SIGNATURE_HASH)
            .topic2(deposit_address.into_word());
        let logs = self
            .provider
            .get_logs(&filter)
            .await
            .context("failed to read ERC20 transfer logs")?;

        for log in logs {
            let decoded = log
                .log_decode::<Transfer>()
                .context("failed to decode ERC20 transfer log")?;
            if decoded.inner.data.value < threshold {
                continue;
            }
            if let (Some(tx_hash), Some(block_number)) =
                (decoded.transaction_hash, decoded.block_number)
            {
                return Ok(Some((tx_hash, block_number)));
            }
        }

        Ok(None)
    }

    async fn find_native_transfer(
        &self,
        deposit_address: Address,
        threshold: U256,
        latest_block: u64,
    ) -> Result<Option<(B256, u64)>> {
        for block_number in 0..=latest_block {
            let Some(block) = self
                .provider
                .get_block_by_number(BlockNumberOrTag::Number(block_number))
                .full()
                .await
                .with_context(|| format!("failed to fetch block {block_number}"))?
            else {
                continue;
            };
            for tx in block.transactions.into_transactions_vec() {
                if tx.to() == Some(deposit_address) && tx.value() >= threshold {
                    return Ok(Some((tx.tx_hash(), block_number)));
                }
            }
        }

        Ok(None)
    }

    async fn handle_detected_deposit(
        &self,
        quote: &EvmTrackedQuote,
        tx_hash: B256,
        detected_block: u64,
    ) -> Result<()> {
        self.record_detected_deposit(quote, tx_hash, detected_block)
            .await?;
        Ok(())
    }

    async fn handle_confirmed_deposit(
        &self,
        quote: &EvmTrackedQuote,
        latest_block: u64,
    ) -> Result<()> {
        let record = self
            .store
            .get_quote_by_deposit(&quote.deposit_address, None)
            .await
            .context("failed to fetch detected EVM quote")?
            .ok_or_else(|| anyhow!("tracked quote disappeared for {}", quote.deposit_address))?;
        let Some(tx_hash_string) = record.evm_deposit_tx_hashes.first() else {
            return Ok(());
        };
        let tx_hash = B256::from_str(tx_hash_string)
            .with_context(|| format!("invalid deposit tx hash {tx_hash_string}"))?;
        let receipt = self
            .provider
            .get_transaction_receipt(tx_hash)
            .await
            .context("failed to fetch detected deposit receipt")?;
        let Some(receipt) = receipt else {
            return Ok(());
        };
        let Some(block_number) = receipt.block_number else {
            return Ok(());
        };
        if latest_block <= block_number {
            return Ok(());
        }

        let confirm_key = format!("evm_deposit_confirmed_{tx_hash_string}");
        if self
            .store
            .record_idempotency_key(quote.correlation_id, &confirm_key)
            .await
            .context("failed to record EVM deposit confirmation idempotency key")?
        {
            self.store
                .record_event(
                    quote.correlation_id,
                    Some("KNOWN_DEPOSIT_TX"),
                    "PENDING_DEPOSIT",
                    "EVM_DEPOSIT_CONFIRMED",
                    None,
                    Some(json!({ "txHash": tx_hash_string })),
                )
                .await
                .context("failed to record EVM deposit confirmation")?;
        }

        self.advance_to_processing_and_success(quote.correlation_id)
            .await
    }

    async fn advance_to_processing_and_success(&self, correlation_id: Uuid) -> Result<()> {
        let processing_key = format!("evm_processing_{correlation_id}");
        if self
            .store
            .record_idempotency_key(correlation_id, &processing_key)
            .await
            .context("failed to record processing idempotency key")?
        {
            self.store
                .record_event(
                    correlation_id,
                    Some("PENDING_DEPOSIT"),
                    "PROCESSING",
                    "EVM_RELEASE_INITIATED",
                    None,
                    None,
                )
                .await
                .context("failed to record processing transition")?;
        }
        self.complete_processing_by_id(correlation_id).await
    }

    async fn complete_processing(&self, quote: &EvmTrackedQuote) -> Result<()> {
        self.complete_processing_by_id(quote.correlation_id).await
    }

    async fn complete_processing_by_id(&self, correlation_id: Uuid) -> Result<()> {
        let success_key = format!("evm_success_{correlation_id}");
        if !self
            .store
            .record_idempotency_key(correlation_id, &success_key)
            .await
            .context("failed to record success idempotency key")?
        {
            return Ok(());
        }

        let mint_tx_id = self
            .mock_miden_mint(correlation_id)
            .await
            .context("failed to complete Miden mint stub")?;
        self.store
            .append_tx_hash(correlation_id, TxHashColumn::MidenMintTxIds, &mint_tx_id)
            .await
            .context("failed to persist Miden mint stub tx id")?;
        self.store
            .record_event(
                correlation_id,
                Some("PROCESSING"),
                "SUCCESS",
                "MIDEN_MINT_CONFIRMED",
                None,
                Some(json!({ "midenMintTxId": mint_tx_id })),
            )
            .await
            .context("failed to record success transition")?;
        Ok(())
    }

    async fn mock_miden_mint(&self, correlation_id: Uuid) -> Result<String> {
        let mint_future: Pin<Box<dyn Future<Output = Result<String>> + Send>> =
            Box::pin(async move { Ok(format!("stub-miden-mint-{correlation_id}")) });
        // TODO: Replace this stub with the real Miden mint flow in PR #7.
        mint_future.await
    }
}

impl TokenContracts {
    fn load(path: &Path) -> Result<Self> {
        let file = load_token_address_file(path)?;
        let mut by_asset_id = HashMap::new();
        if let Some(address) = file.usdc {
            by_asset_id.insert(
                "eth-anvil:usdc".to_owned(),
                Address::from_str(&address).context("invalid USDC contract address")?,
            );
        }
        if let Some(address) = file.usdt {
            by_asset_id.insert(
                "eth-anvil:usdt".to_owned(),
                Address::from_str(&address).context("invalid USDT contract address")?,
            );
        }
        if let Some(address) = file.btc {
            by_asset_id.insert(
                "eth-anvil:btc".to_owned(),
                Address::from_str(&address).context("invalid BTC contract address")?,
            );
        }

        Ok(Self { by_asset_id })
    }
}

pub fn token_addresses_path_from_env() -> PathBuf {
    env::var("EVM_TOKEN_ADDRESSES_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/state/token-addresses.json"))
}

pub fn load_token_address_file(path: &Path) -> Result<TokenAddressFile> {
    if !path.exists() {
        return Ok(TokenAddressFile::default());
    }
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read token address file {}", path.display()))?;
    serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse token address file {}", path.display()))
}

pub fn derivation_index(correlation_id: Uuid) -> u32 {
    (correlation_id.as_u128() & u128::from(u32::MAX)) as u32
}

pub fn derivation_path(correlation_id: Uuid) -> String {
    format!("m/44'/60'/0'/0/{}", derivation_index(correlation_id))
}

pub fn derive_address_from_mnemonic(
    master_mnemonic: &str,
    correlation_id: Uuid,
) -> Result<Address> {
    let signer = MnemonicBuilder::<English>::default()
        .phrase(master_mnemonic)
        .index(derivation_index(correlation_id))
        .context("failed to derive EVM deposit index")?
        .build()
        .context("failed to build EVM deposit signer")?;
    Ok(signer.address())
}

pub fn evm_quote_requires_deposit_address(origin_asset: &str, destination_asset: &str) -> bool {
    origin_asset.starts_with("eth-anvil:") && destination_asset.starts_with("miden-local:")
}

pub fn quote_origin_asset_is_supported(asset_id: &str) -> bool {
    matches!(
        asset_id,
        "eth-anvil:eth" | "eth-anvil:usdc" | "eth-anvil:usdt" | "eth-anvil:btc"
    )
}
