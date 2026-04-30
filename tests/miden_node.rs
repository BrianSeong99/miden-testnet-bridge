use std::time::Duration;

use miden_testnet_bridge::chains::miden::MidenClient;
use tempfile::tempdir;
use tokio::time::{Instant, sleep};

#[tokio::test]
async fn syncs_against_configured_miden_rpc() {
    let rpc_url = match std::env::var("MIDEN_RPC_URL") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => return,
    };

    let store_dir = tempdir().expect("tempdir");
    let client = MidenClient::new(&rpc_url, store_dir.path())
        .await
        .expect("miden client");

    client.sync_state().await.expect("sync state");

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match client.tip_block_height().await {
            Ok(height) if height > 0 => {
                assert!(height > 0, "expected positive miden block height");
                break;
            }
            Ok(_) | Err(_) if Instant::now() < deadline => sleep(Duration::from_secs(2)).await,
            Ok(height) => panic!("miden tip height did not advance above zero, got {height}"),
            Err(err) => panic!("failed to read miden tip height: {err:#}"),
        }
    }
}
