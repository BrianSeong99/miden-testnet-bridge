use std::time::Duration;

use anyhow::Result;
use serial_test::serial;
use uuid::Uuid;

use crate::common::{
    Direction, LOCAL_ETH_E2E_AMOUNT, LOCAL_ETH_E2E_AMOUNT_STR, assert_status_subsequence,
    require_e2e, send_native_eth, start_test,
};

#[tokio::test]
#[serial]
async fn slippage_exceeded_refunds_origin_chain() -> Result<()> {
    require_e2e("refund e2e");

    let ctx = start_test("refund").await?;
    let user_wallet = ctx.create_wallet("refund-user").await?;
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
    ctx.force_min_amount_out(correlation_id, "10000000000000")
        .await?;

    send_native_eth(&deposit_address, LOCAL_ETH_E2E_AMOUNT).await?;

    let status = ctx
        .poll_status_until(
            &deposit_address,
            None,
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
    println!(
        "E2E_EVIDENCE refund correlation_id={} evm_deposit_tx_hashes={:?} evm_refund_tx_hashes={:?}",
        quote.correlation_id, artifacts.evm_deposit_tx_hashes, artifacts.evm_refund_tx_hashes
    );

    Ok(())
}
