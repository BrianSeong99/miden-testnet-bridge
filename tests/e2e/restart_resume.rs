use std::time::Duration;

use anyhow::{Result, ensure};
use serial_test::serial;
use tokio::time::{Instant, sleep};
use uuid::Uuid;

use crate::common::{
    Direction, LOCAL_ETH_E2E_AMOUNT, LOCAL_ETH_E2E_AMOUNT_STR, assert_status_subsequence,
    require_e2e, send_native_eth, start_test,
};

#[tokio::test]
#[serial]
async fn restart_during_processing_resumes_to_success() -> Result<()> {
    require_e2e("restart/resume e2e");

    let ctx = start_test("restart-resume").await?;
    let user_wallet = ctx.create_wallet("restart-user").await?;
    let recipient = ctx.miden.encode_basic_wallet_address(user_wallet.id());
    let quote = ctx
        .make_quote_with_parties(
            Direction::Inbound,
            "eth",
            LOCAL_ETH_E2E_AMOUNT_STR,
            &recipient,
            "0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc",
        )
        .await?;
    let deposit_address = quote
        .quote
        .deposit_address
        .clone()
        .expect("deposit address");
    let correlation_id = Uuid::parse_str(&quote.correlation_id)?;

    send_native_eth(&deposit_address, LOCAL_ETH_E2E_AMOUNT).await?;
    wait_for_processing(&ctx, correlation_id).await?;
    ctx.restart_bridge().await?;

    let status = ctx
        .poll_status_until(
            &deposit_address,
            None,
            miden_testnet_bridge::types::SwapStatus::Success,
            Duration::from_secs(120),
        )
        .await?;
    assert_eq!(status.correlation_id, quote.correlation_id);

    let lifecycle = ctx.lifecycle_statuses(correlation_id).await?;
    assert_status_subsequence(
        &lifecycle,
        &[
            "KNOWN_DEPOSIT_TX",
            "PENDING_DEPOSIT",
            "PROCESSING",
            "SUCCESS",
        ],
    );
    let artifacts = ctx.chain_artifacts(correlation_id).await?;
    println!(
        "E2E_EVIDENCE restart_resume correlation_id={} evm_deposit_tx_hashes={:?} miden_mint_tx_ids={:?}",
        quote.correlation_id, artifacts.evm_deposit_tx_hashes, artifacts.miden_mint_tx_ids
    );

    Ok(())
}

async fn wait_for_processing(ctx: &crate::common::TestContext, correlation_id: Uuid) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let lifecycle = ctx.lifecycle_statuses(correlation_id).await?;
        if lifecycle.iter().any(|status| status == "PROCESSING") {
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "timed out waiting for PROCESSING"
        );
        sleep(Duration::from_millis(250)).await;
    }
}
