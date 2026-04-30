use std::time::Duration;

use anyhow::Result;
use serial_test::serial;
use uuid::Uuid;

use crate::common::{
    Direction, assert_status_subsequence, run_e2e_enabled, send_native_eth, start_test,
    wait_for_intermediate_status,
};

#[tokio::test]
#[serial]
async fn slippage_exceeded_refunds_origin_chain() -> Result<()> {
    if !run_e2e_enabled() {
        eprintln!("skipping refund e2e; set RUN_E2E=1");
        return Ok(());
    }

    let ctx = start_test("refund").await?;
    let user_wallet = ctx.create_wallet("refund-user").await?;
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
    wait_for_intermediate_status(
        &deposit_address,
        miden_testnet_bridge::types::SwapStatus::KnownDepositTx,
        Duration::from_secs(30),
    )
    .await?;
    ctx.force_min_amount_out(correlation_id, "9999999999999999999")
        .await?;

    let status = ctx
        .poll_status_until(
            &deposit_address,
            miden_testnet_bridge::types::SwapStatus::Refunded,
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
            "REFUNDED",
        ],
    );

    let artifacts = ctx.chain_artifacts(correlation_id).await?;
    assert!(!artifacts.evm_refund_tx_hashes.is_empty());

    Ok(())
}
