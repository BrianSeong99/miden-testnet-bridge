use std::time::Duration;

use anyhow::Result;
use serial_test::serial;
use uuid::Uuid;

use crate::common::{
    Direction, assert_status_subsequence, default_evm_address, evm_balance, run_e2e_enabled,
    start_test,
};

#[tokio::test]
#[serial]
async fn outbound_note_releases_on_anvil() -> Result<()> {
    if !run_e2e_enabled() {
        eprintln!("skipping outbound e2e; set RUN_E2E=1");
        return Ok(());
    }

    let ctx = start_test("outbound").await?;
    let bootstrap = ctx.bootstrap_state().await?;
    let sender = ctx.create_wallet("outbound-sender").await?;
    let refund_to = ctx.miden.encode_basic_wallet_address(sender.id());
    let recipient = default_evm_address();
    let balance_before = evm_balance(recipient).await?;
    let quote = ctx
        .make_quote_with_parties(
            Direction::Outbound,
            "eth",
            "1000000000000000000",
            &recipient.to_string(),
            &refund_to,
        )
        .await?;
    let deposit_address = quote
        .quote
        .deposit_address
        .clone()
        .expect("deposit address");
    let correlation_id = Uuid::parse_str(&quote.correlation_id)?;

    ctx.mint_to_wallet(
        &bootstrap,
        &sender,
        "miden-local:eth",
        1_000_000_000_000_000_000,
    )
    .await?;
    ctx.send_outbound_note(
        &bootstrap,
        &sender,
        &deposit_address,
        1_000_000_000_000_000_000,
    )
    .await?;

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

    let artifacts = ctx.chain_artifacts(correlation_id).await?;
    assert!(!artifacts.miden_consume_tx_ids.is_empty());
    assert!(!artifacts.evm_release_tx_hashes.is_empty());

    let balance_after = evm_balance(recipient).await?;
    assert_eq!(
        balance_after - balance_before,
        alloy::primitives::U256::from(1_000_000_000_000_000_000u128)
    );

    Ok(())
}
