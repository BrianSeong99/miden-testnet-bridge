use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use miden_client::{
    Client, DebugMode, builder::ClientBuilder, keystore::FilesystemKeyStore, rpc::Endpoint,
};
use miden_client_sqlite_store::ClientBuilderSqliteExt;
use tokio::{
    runtime::Builder as RuntimeBuilder,
    task,
    time::{Duration, sleep},
};
use tracing::warn;

type InnerClient = Client<FilesystemKeyStore>;

#[async_trait]
pub trait MidenHealthCheck: Send + Sync {
    async fn tip_block_height(&self) -> Result<u32>;
}

pub struct MidenClient {
    endpoint: Endpoint,
    store_dir: PathBuf,
}

impl MidenClient {
    pub async fn new(endpoint: &str, store_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(store_dir)
            .with_context(|| format!("failed to create miden store dir {}", store_dir.display()))?;

        let endpoint = Endpoint::try_from(endpoint)
            .map_err(|err| anyhow!("invalid MIDEN_RPC_URL {endpoint}: {err}"))?;

        Ok(Self {
            endpoint,
            store_dir: store_dir.to_path_buf(),
        })
    }

    pub async fn sync_state(&self) -> Result<()> {
        let endpoint = self.endpoint.clone();
        let store_dir = self.store_dir.clone();

        task::spawn_blocking(move || {
            let runtime = RuntimeBuilder::new_current_thread()
                .enable_all()
                .build()
                .context("failed to build tokio runtime for miden sync")?;

            runtime.block_on(async move {
                let mut client = build_client(&endpoint, &store_dir).await?;

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

        task::spawn_blocking(move || {
            let runtime = RuntimeBuilder::new_current_thread()
                .enable_all()
                .build()
                .context("failed to build tokio runtime for miden tip")?;

            runtime.block_on(async move {
                let mut client = build_client(&endpoint, &store_dir).await?;

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

async fn build_client(endpoint: &Endpoint, store_dir: &Path) -> Result<InnerClient> {
    let store_path = store_dir.join("store.sqlite3");
    let keystore_path = store_dir.join("keystore");
    let keystore = FilesystemKeyStore::new(keystore_path.clone()).with_context(|| {
        format!(
            "failed to initialize miden keystore at {}",
            keystore_path.display()
        )
    })?;

    ClientBuilder::new()
        .grpc_client(endpoint, Some(10_000))
        .sqlite_store(store_path)
        .authenticator(Arc::new(keystore))
        .in_debug_mode(DebugMode::Disabled)
        .build()
        .await
        .context("failed to build miden client")
}

#[async_trait]
impl MidenHealthCheck for MidenClient {
    async fn tip_block_height(&self) -> Result<u32> {
        MidenClient::tip_block_height(self).await
    }
}
