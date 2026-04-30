use std::time::Duration;

use anyhow::Result;
use serial_test::serial;
use uuid::Uuid;

use crate::common::{
    Direction, assert_status_subsequence, run_e2e_enabled, send_native_eth, start_test,
};

#[tokio::test]
#[serial]
async fn deposit_below_min_amount_becomes_incomplete() -> Result<()> {
    if !run_e2e_enabled() {
        eprintln!("skipping incomplete e2e; set RUN_E2E=1");
        return Ok(());
    }

    let ctx = start_test("incomplete").await?;
    let user_wallet = ctx.create_wallet("incomplete-user").await?;
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

    send_native_eth(&deposit_address, 500_000_000_000_000_000).await?;

    let status = ctx
        .poll_status_until(
            &deposit_address,
            miden_testnet_bridge::types::SwapStatus::IncompleteDeposit,
            Duration::from_secs(120),
        )
        .await?;
    assert_eq!(status.correlation_id, quote.correlation_id);

    let lifecycle = ctx.lifecycle_statuses(correlation_id).await?;
    assert_status_subsequence(&lifecycle, &["KNOWN_DEPOSIT_TX", "INCOMPLETE_DEPOSIT"]);

    Ok(())
}
