use std::time::Duration;

use anyhow::{Result, ensure};
use serial_test::serial;
use tokio::time::{Instant, sleep};
use uuid::Uuid;

use crate::common::{
    Direction, assert_status_subsequence, run_e2e_enabled, send_native_eth, start_test,
};

#[tokio::test]
#[serial]
async fn restart_during_processing_resumes_to_success() -> Result<()> {
    if !run_e2e_enabled() {
        eprintln!("skipping restart/resume e2e; set RUN_E2E=1");
        return Ok(());
    }

    let ctx = start_test("restart-resume").await?;
    let user_wallet = ctx.create_wallet("restart-user").await?;
    let recipient = ctx.miden.encode_basic_wallet_address(user_wallet.id());
    let quote = ctx
        .make_quote_with_parties(
            Direction::Inbound,
            "eth",
            "1000000000000000000",
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

    send_native_eth(&deposit_address, 1_000_000_000_000_000_000).await?;
    wait_for_processing(&ctx, correlation_id).await?;
    ctx.restart_bridge().await?;

    let status = ctx
        .poll_status_until(
            &deposit_address,
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
