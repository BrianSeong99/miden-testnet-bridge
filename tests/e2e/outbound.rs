use std::time::Duration;

use anyhow::Result;
use serial_test::serial;
use uuid::Uuid;

use crate::common::{
    Direction, LOCAL_ETH_E2E_AMOUNT, LOCAL_ETH_E2E_AMOUNT_STR, assert_status_subsequence,
    default_evm_address, evm_balance, require_e2e, send_native_eth, start_test,
};
use miden_testnet_bridge::chains::miden_bridge_note::BridgeOutDepositMemo;

#[tokio::test]
#[serial]
async fn outbound_note_releases_on_anvil() -> Result<()> {
    require_e2e("outbound e2e");

    let ctx = start_test("outbound").await?;
    let bootstrap = ctx.bootstrap_state().await?;
    let sender = ctx.create_wallet("outbound-sender").await?;
    let refund_to = ctx.miden.encode_basic_wallet_address(sender.id());
    let recipient = default_evm_address();
    let balance_before = evm_balance(recipient).await?;
    let funding_quote = ctx
        .make_quote_with_parties(
            Direction::Inbound,
            "eth",
            LOCAL_ETH_E2E_AMOUNT_STR,
            &refund_to,
            &recipient.to_string(),
        )
        .await?;
    let funding_deposit_address = funding_quote
        .quote
        .deposit_address
        .clone()
        .expect("funding deposit address");

    send_native_eth(&funding_deposit_address, LOCAL_ETH_E2E_AMOUNT).await?;
    let funding_status = ctx
        .poll_status_until(
            &funding_deposit_address,
            None,
            miden_testnet_bridge::types::SwapStatus::Success,
            Duration::from_secs(180),
        )
        .await?;
    assert_eq!(funding_status.correlation_id, funding_quote.correlation_id);
    let note_count = ctx
        .wait_for_consumable_notes(&sender, Duration::from_secs(120))
        .await?;
    assert!(note_count > 0);

    let quote = ctx
        .make_quote_with_parties(
            Direction::Outbound,
            "eth",
            LOCAL_ETH_E2E_AMOUNT_STR,
            &recipient.to_string(),
            &refund_to,
        )
        .await?;
    let deposit_address = quote
        .quote
        .deposit_address
        .clone()
        .expect("deposit address");
    let deposit_memo = quote
        .quote
        .deposit_memo
        .clone()
        .expect("bridge-note deposit memo");
    let bridge_memo = BridgeOutDepositMemo::from_deposit_memo(&deposit_memo)?;
    let correlation_id = Uuid::parse_str(&quote.correlation_id)?;

    ctx.send_outbound_note(
        &bootstrap,
        &sender,
        &deposit_address,
        &deposit_memo,
        LOCAL_ETH_E2E_AMOUNT as u64,
    )
    .await?;

    let status = ctx
        .poll_status_until(
            &deposit_address,
            Some(&deposit_memo),
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
    let balance_delta = balance_after - balance_before;
    assert_eq!(
        balance_delta,
        alloy::primitives::U256::from(LOCAL_ETH_E2E_AMOUNT)
    );
    println!(
        "E2E_EVIDENCE outbound funding_correlation_id={} outbound_correlation_id={} quote_hash={} miden_consume_tx_ids={:?} evm_release_tx_hashes={:?} balance_delta={}",
        funding_quote.correlation_id,
        quote.correlation_id,
        bridge_memo.storage.quote_hash,
        artifacts.miden_consume_tx_ids,
        artifacts.evm_release_tx_hashes,
        balance_delta
    );

    Ok(())
}
