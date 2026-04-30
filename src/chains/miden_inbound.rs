use anyhow::{Context, Result, anyhow};
use miden_client::{
    account::AccountId,
    asset::FungibleAsset,
    note::NoteType,
    transaction::{PaymentNoteDescription, TransactionId, TransactionRequestBuilder},
};
use serde_json::json;
use uuid::Uuid;

use crate::{
    chains::{
        miden::{MidenClient, parse_account_id},
        miden_bootstrap::{BootstrapState, sync_with_retry, wait_for_tx},
    },
    core::state::{DynStateStore, TxHashColumn},
};

pub async fn mint_to_user(
    client: &MidenClient,
    solver_id: AccountId,
    faucet_id: AccountId,
    recipient_id: AccountId,
    amount: u64,
) -> Result<TransactionId> {
    let mut inner = client.open().await?;
    sync_with_retry(&mut inner).await?;
    let tx_request = TransactionRequestBuilder::new().build_pay_to_id(
        PaymentNoteDescription::new(
            vec![FungibleAsset::new(faucet_id, amount)?.into()],
            solver_id,
            recipient_id,
        ),
        NoteType::Private,
        inner.rng(),
    )?;
    let tx_id = inner
        .submit_new_transaction(solver_id, tx_request)
        .await
        .context("failed to submit solver pay-to-id transaction")?;
    wait_for_tx(&mut inner, tx_id).await?;
    Ok(tx_id)
}

pub async fn mint_quote_to_user(
    client: &MidenClient,
    state_store: DynStateStore,
    bootstrap: &BootstrapState,
    correlation_id: Uuid,
) -> Result<String> {
    let record = state_store
        .get_quote_by_correlation_id(correlation_id)
        .await
        .context("failed to load quote for inbound Miden mint")?
        .ok_or_else(|| anyhow!("quote {correlation_id} not found"))?;

    let idempotency_key = format!("miden_mint_{correlation_id}");
    if !state_store
        .record_idempotency_key(correlation_id, &idempotency_key)
        .await
        .context("failed to record Miden mint idempotency key")?
    {
        return record.miden_mint_tx_ids.last().cloned().ok_or_else(|| {
            anyhow!("quote {correlation_id} was already minted but no tx id exists")
        });
    }

    let faucet_id = bootstrap.faucet_id_for_asset(&record.quote_request.destination_asset)?;
    let recipient_id = parse_account_id(&record.quote_request.recipient)?;
    let amount = record
        .quote_response
        .quote
        .amount_out
        .parse::<u64>()
        .with_context(|| {
            format!(
                "invalid Miden mint amount {} for quote {correlation_id}",
                record.quote_response.quote.amount_out
            )
        })?;
    let tx_id = mint_to_user(
        client,
        bootstrap.solver_account_id,
        faucet_id,
        recipient_id,
        amount,
    )
    .await?;
    let tx_id_string = tx_id.to_string();

    state_store
        .append_tx_hash(correlation_id, TxHashColumn::MidenMintTxIds, &tx_id_string)
        .await
        .context("failed to persist Miden mint tx id")?;
    state_store
        .record_event(
            correlation_id,
            Some("PROCESSING"),
            "SUCCESS",
            "MIDEN_MINT_CONFIRMED",
            None,
            Some(json!({ "midenMintTxId": tx_id_string })),
        )
        .await
        .context("failed to record Miden mint success event")?;

    Ok(tx_id_string)
}
