use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use miden_client::{
    Client, DebugMode, RemoteTransactionProver,
    account::{AccountId, Address, AddressInterface},
    builder::ClientBuilder,
    grpc_support::{DEVNET_PROVER_ENDPOINT, TESTNET_PROVER_ENDPOINT},
    keystore::FilesystemKeyStore,
    rpc::{Endpoint, EndpointError, GrpcClient, NodeRpcClient},
};
use miden_client_sqlite_store::ClientBuilderSqliteExt;
use miden_protocol::Word;
use tokio::{runtime::Builder as RuntimeBuilder, task, time::sleep};
use tracing::warn;

type InnerClient = Client<FilesystemKeyStore>;

#[async_trait]
pub trait MidenHealthCheck: Send + Sync {
    async fn tip_block_height(&self) -> Result<u32>;
}

#[derive(Clone)]
pub struct MidenClient {
    endpoint: Endpoint,
    store_dir: PathBuf,
    remote_prover_url: Option<String>,
    remote_prover_timeout: Duration,
}

impl MidenClient {
    pub async fn new(endpoint: &str, store_dir: &Path) -> Result<Self> {
        Self::new_with_remote_prover(endpoint, store_dir, None, Duration::from_secs(10)).await
    }

    pub async fn new_with_remote_prover(
        endpoint: &str,
        store_dir: &Path,
        remote_prover_url: Option<String>,
        remote_prover_timeout: Duration,
    ) -> Result<Self> {
        std::fs::create_dir_all(store_dir)
            .with_context(|| format!("failed to create miden store dir {}", store_dir.display()))?;

        let endpoint = Endpoint::try_from(endpoint)
            .map_err(|err| anyhow!("invalid MIDEN_RPC_URL {endpoint}: {err}"))?;

        Ok(Self {
            endpoint,
            store_dir: store_dir.to_path_buf(),
            remote_prover_url,
            remote_prover_timeout,
        })
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    pub fn network_id(&self) -> miden_client::address::NetworkId {
        self.endpoint.to_network_id()
    }

    pub fn open_keystore(&self) -> Result<FilesystemKeyStore> {
        let keystore_path = self.store_dir.join("keystore");
        FilesystemKeyStore::new(keystore_path.clone()).with_context(|| {
            format!(
                "failed to initialize miden keystore at {}",
                keystore_path.display()
            )
        })
    }

    pub async fn open(&self) -> Result<InnerClient> {
        build_client(
            &self.endpoint,
            &self.store_dir,
            self.remote_prover_url.as_deref(),
            self.remote_prover_timeout,
        )
        .await
    }

    pub async fn account_commitment(&self, account_id: AccountId) -> Result<Option<Word>> {
        let rpc = GrpcClient::new(
            &self.endpoint,
            self.remote_prover_timeout.as_millis() as u64,
        );
        match rpc.get_account_details(account_id).await {
            Ok(account) => {
                let commitment = account.commitment();
                if commitment == Word::empty() {
                    Ok(None)
                } else {
                    Ok(Some(commitment))
                }
            }
            Err(err) => match err.endpoint_error() {
                Some(EndpointError::GetAccount(error))
                    if error.to_string() == "account not found" =>
                {
                    Ok(None)
                }
                _ => Err(err).with_context(|| {
                    format!("failed to fetch Miden account details for {account_id}")
                }),
            },
        }
    }

    pub fn encode_basic_wallet_address(&self, account_id: AccountId) -> String {
        Address::new(account_id)
            .with_routing_parameters(miden_client::address::RoutingParameters::new(
                AddressInterface::BasicWallet,
            ))
            .encode(self.network_id())
    }

    pub async fn sync_state(&self) -> Result<()> {
        let endpoint = self.endpoint.clone();
        let store_dir = self.store_dir.clone();
        let remote_prover_url = self.remote_prover_url.clone();
        let remote_prover_timeout = self.remote_prover_timeout;

        task::spawn_blocking(move || {
            let runtime = RuntimeBuilder::new_current_thread()
                .enable_all()
                .build()
                .context("failed to build tokio runtime for miden sync")?;

            runtime.block_on(async move {
                let mut client = build_client(
                    &endpoint,
                    &store_dir,
                    remote_prover_url.as_deref(),
                    remote_prover_timeout,
                )
                .await?;

                for attempt in 0..5 {
                    match client.sync_state().await {
                        Ok(_) => return Ok(()),
                        Err(err) if attempt < 4 => {
                            warn!(
                                attempt = attempt + 1,
                                error = %err,
                                "miden sync_state failed, retrying"
                            );
                            sleep(Duration::from_secs(2)).await;
                        }
                        Err(err) => {
                            return Err(err).context("miden sync_state failed after retries");
                        }
                    }
                }

                unreachable!("retry loop must return")
            })
        })
        .await
        .context("miden sync task join failure")?
    }

    pub async fn tip_block_height(&self) -> Result<u32> {
        let endpoint = self.endpoint.clone();
        let store_dir = self.store_dir.clone();
        let remote_prover_url = self.remote_prover_url.clone();
        let remote_prover_timeout = self.remote_prover_timeout;

        task::spawn_blocking(move || {
            let runtime = RuntimeBuilder::new_current_thread()
                .enable_all()
                .build()
                .context("failed to build tokio runtime for miden tip")?;

            runtime.block_on(async move {
                let mut client = build_client(
                    &endpoint,
                    &store_dir,
                    remote_prover_url.as_deref(),
                    remote_prover_timeout,
                )
                .await?;

                for attempt in 0..5 {
                    match client.sync_state().await {
                        Ok(_) => break,
                        Err(err) if attempt < 4 => {
                            warn!(
                                attempt = attempt + 1,
                                error = %err,
                                "miden sync_state failed, retrying"
                            );
                            sleep(Duration::from_secs(2)).await;
                        }
                        Err(err) => {
                            return Err(err).context("miden sync_state failed after retries");
                        }
                    }
                }

                let block_num = client
                    .get_sync_height()
                    .await
                    .context("failed to read miden sync height")?;

                Ok(block_num.as_u32())
            })
        })
        .await
        .context("miden tip task join failure")?
    }
}

pub fn parse_account_id(value: &str) -> Result<AccountId> {
    if value.starts_with("0x") {
        return AccountId::from_hex(value)
            .map_err(|err| anyhow!("invalid Miden account id {value}: {err}"));
    }

    let (_, address) = Address::decode(value)
        .map_err(|err| anyhow!("invalid Miden bech32 address {value}: {err}"))?;
    match address.id() {
        miden_client::address::AddressId::AccountId(account_id) => Ok(account_id),
        _ => Err(anyhow!("address {value} is not an account ID address")),
    }
}

pub fn is_miden_asset_id(asset_id: &str) -> bool {
    crate::chains::profile::is_miden_asset_id(asset_id)
}

pub fn is_evm_asset_id(asset_id: &str) -> bool {
    crate::chains::profile::is_evm_asset_id(asset_id)
}

pub fn miden_quote_requires_deposit_address(origin_asset: &str) -> bool {
    is_miden_asset_id(origin_asset)
}

pub fn asset_symbol(asset_id: &str) -> Result<&'static str> {
    crate::chains::profile::asset_symbol(asset_id)
}

// Miden's BasicFungibleFaucet caps decimals at 12. ETH is 18-decimal on EVM,
// so on the Miden side we represent it at 12 decimals; the bridge scales by
// 10^6 when minting/consuming to keep amounts consistent across chains.
pub fn asset_decimals(asset_id: &str) -> Result<u8> {
    crate::chains::profile::miden_asset_decimals(asset_id)
}

pub fn solver_liquidity_for_asset(asset_id: &str) -> Result<u64> {
    crate::chains::profile::solver_liquidity_for_asset(asset_id)
}

async fn build_client(
    endpoint: &Endpoint,
    store_dir: &Path,
    remote_prover_url: Option<&str>,
    remote_prover_timeout: Duration,
) -> Result<InnerClient> {
    let store_path = store_dir.join("store.sqlite3");
    let keystore_path = store_dir.join("keystore");
    let keystore = FilesystemKeyStore::new(keystore_path.clone()).with_context(|| {
        format!(
            "failed to initialize miden keystore at {}",
            keystore_path.display()
        )
    })?;

    let mut builder = client_builder_for_endpoint(endpoint);
    let remote_prover_url = remote_prover_url.or_else(|| native_remote_prover_endpoint(endpoint));
    if let Some(remote_prover_url) = remote_prover_url {
        builder = builder.prover(Arc::new(
            RemoteTransactionProver::new(remote_prover_url.to_owned())
                .with_timeout(remote_prover_timeout),
        ));
    }

    builder
        .sqlite_store(store_path)
        .authenticator(Arc::new(keystore))
        .in_debug_mode(DebugMode::Disabled)
        .build()
        .await
        .context("failed to build miden client")
}

fn native_remote_prover_endpoint(endpoint: &Endpoint) -> Option<&'static str> {
    if endpoint == &Endpoint::testnet() {
        Some(TESTNET_PROVER_ENDPOINT)
    } else if endpoint == &Endpoint::devnet() {
        Some(DEVNET_PROVER_ENDPOINT)
    } else {
        None
    }
}

fn client_builder_for_endpoint(endpoint: &Endpoint) -> ClientBuilder<FilesystemKeyStore> {
    if endpoint == &Endpoint::testnet() {
        ClientBuilder::for_testnet()
    } else if endpoint == &Endpoint::devnet() {
        ClientBuilder::for_devnet()
    } else if endpoint == &Endpoint::localhost() {
        ClientBuilder::for_localhost()
    } else {
        ClientBuilder::new().grpc_client(endpoint, Some(10_000))
    }
}

#[async_trait]
impl MidenHealthCheck for MidenClient {
    async fn tip_block_height(&self) -> Result<u32> {
        MidenClient::tip_block_height(self).await
    }
}
