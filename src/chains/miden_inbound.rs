use anyhow::{Context, Result, anyhow};
use miden_client::{
    account::AccountId,
    asset::FungibleAsset,
    note::NoteType,
    transaction::{PaymentNoteDescription, TransactionId, TransactionRequestBuilder},
};
use miden_protocol::utils::serde::Deserializable;
use serde_json::json;
use tokio::time::{Duration, sleep};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    chains::{
        miden::{MidenClient, parse_account_id},
        miden_bootstrap::{BootstrapState, sync_with_retry, wait_for_tx},
    },
    core::state::{DynStateStore, TxHashColumn},
};

pub struct MidenTransfer<'a> {
    pub faucet_id: AccountId,
    pub recipient_id: AccountId,
    pub amount: &'a str,
    pub tx_column: TxHashColumn,
    pub idempotency_prefix: &'a str,
}

pub async fn mint_to_user(
    client: &MidenClient,
    solver_id: AccountId,
    faucet_id: AccountId,
    recipient_id: AccountId,
    amount: u64,
) -> Result<TransactionId> {
    let tx_id = submit_mint_to_user(client, solver_id, faucet_id, recipient_id, amount).await?;
    wait_for_submitted_miden_tx(client, tx_id).await?;
    info!(
        tx_id = %tx_id,
        solver_account_id = %solver_id,
        recipient_account_id = %recipient_id,
        amount = amount,
        "confirmed Miden pay-to-id mint"
    );
    Ok(tx_id)
}

async fn submit_mint_to_user(
    client: &MidenClient,
    solver_id: AccountId,
    faucet_id: AccountId,
    recipient_id: AccountId,
    amount: u64,
) -> Result<TransactionId> {
    let mut inner = client.open().await?;
    sync_with_retry(&mut inner).await?;
    info!(
        solver_account_id = %solver_id,
        faucet_account_id = %faucet_id,
        recipient_account_id = %recipient_id,
        amount = amount,
        note_type = "public",
        "submitting Miden pay-to-id mint"
    );
    let tx_request = TransactionRequestBuilder::new().build_pay_to_id(
        PaymentNoteDescription::new(
            vec![FungibleAsset::new(faucet_id, amount)?.into()],
            solver_id,
            recipient_id,
        ),
        NoteType::Public,
        inner.rng(),
    )?;
    let tx_id = inner
        .submit_new_transaction(solver_id, tx_request)
        .await
        .context("failed to submit solver pay-to-id transaction")?;
    info!(
        tx_id = %tx_id,
        solver_account_id = %solver_id,
        recipient_account_id = %recipient_id,
        amount = amount,
        "submitted Miden pay-to-id mint"
    );
    Ok(tx_id)
}

async fn wait_for_submitted_miden_tx(client: &MidenClient, tx_id: TransactionId) -> Result<()> {
    let mut inner = client.open().await?;
    wait_for_tx(&mut inner, tx_id).await
}

pub async fn mint_quote_to_user(
    client: &MidenClient,
    state_store: DynStateStore,
    bootstrap: &BootstrapState,
    correlation_id: Uuid,
    transfer: MidenTransfer<'_>,
) -> Result<String> {
    let tx_amount = transfer.amount.parse::<u64>().with_context(|| {
        format!(
            "invalid Miden amount {} for quote {correlation_id}",
            transfer.amount
        )
    })?;
    let idempotency_key = format!("{}_{correlation_id}", transfer.idempotency_prefix);
    if !state_store
        .record_idempotency_key(correlation_id, &idempotency_key)
        .await
        .context("failed to record Miden tx idempotency key")?
    {
        let existing =
            wait_for_existing_miden_tx_id(state_store.as_ref(), correlation_id, transfer.tx_column)
                .await?;
        if let Some(existing) = existing {
            let tx_id = parse_transaction_id(&existing)?;
            wait_for_submitted_miden_tx(client, tx_id).await?;
            return Ok(existing);
        }

        warn!(
            correlation_id = %correlation_id,
            tx_column = ?transfer.tx_column,
            "Miden idempotency key exists without durable tx id; resubmitting"
        );
    }

    let tx_id = submit_mint_to_user(
        client,
        bootstrap.solver_account_id,
        transfer.faucet_id,
        transfer.recipient_id,
        tx_amount,
    )
    .await?;
    let tx_id_string = tx_id.to_string();

    state_store
        .append_tx_hash(correlation_id, transfer.tx_column, &tx_id_string)
        .await
        .context("failed to persist Miden tx id")?;
    wait_for_submitted_miden_tx(client, tx_id).await?;
    info!(
        tx_id = %tx_id,
        solver_account_id = %bootstrap.solver_account_id,
        recipient_account_id = %transfer.recipient_id,
        amount = tx_amount,
        "confirmed Miden pay-to-id mint"
    );

    Ok(tx_id_string)
}

async fn wait_for_existing_miden_tx_id(
    state_store: &dyn crate::core::state::StateStore,
    correlation_id: Uuid,
    tx_column: TxHashColumn,
) -> Result<Option<String>> {
    for attempt in 0..20 {
        let record = state_store
            .get_quote_by_correlation_id(correlation_id)
            .await
            .context("failed to reload existing quote")?
            .ok_or_else(|| anyhow!("quote {correlation_id} not found"))?;
        if let Some(tx_id) = existing_miden_tx_id(&record, tx_column) {
            return Ok(Some(tx_id.to_owned()));
        }

        if attempt == 0 {
            warn!(
                correlation_id = %correlation_id,
                tx_column = ?tx_column,
                "Miden idempotency key exists but tx id is not durable yet"
            );
        }
        sleep(Duration::from_millis(500)).await;
    }

    Ok(None)
}

fn existing_miden_tx_id(
    record: &crate::core::state::QuoteRecord,
    tx_column: TxHashColumn,
) -> Option<&str> {
    match tx_column {
        TxHashColumn::MidenMintTxIds => record.miden_mint_tx_ids.last().map(String::as_str),
        TxHashColumn::MidenRefundTxIds => record.miden_refund_tx_ids.last().map(String::as_str),
        _ => None,
    }
}

fn parse_transaction_id(tx_id: &str) -> Result<TransactionId> {
    let raw = alloy::hex::decode(tx_id.trim_start_matches("0x"))
        .with_context(|| format!("invalid Miden transaction id {tx_id}"))?;
    TransactionId::read_from_bytes(&raw)
        .with_context(|| format!("invalid Miden transaction id {tx_id}"))
}

pub async fn mint_quote_to_recipient(
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

    let faucet_id = bootstrap.faucet_id_for_asset(&record.quote_request.destination_asset)?;
    let recipient_id = parse_account_id(&record.quote_request.recipient)?;
    mint_quote_to_user(
        client,
        state_store,
        bootstrap,
        correlation_id,
        MidenTransfer {
            faucet_id,
            recipient_id,
            amount: &record.quote_response.quote.amount_out,
            tx_column: TxHashColumn::MidenMintTxIds,
            idempotency_prefix: "miden_mint",
        },
    )
    .await
}

pub async fn refund_quote_to_origin(
    client: &MidenClient,
    state_store: DynStateStore,
    bootstrap: &BootstrapState,
    correlation_id: Uuid,
    refund_to: &str,
    origin_asset: &str,
    amount: &str,
) -> Result<String> {
    let faucet_id = bootstrap.faucet_id_for_asset(origin_asset)?;
    let recipient_id = parse_account_id(refund_to)?;
    mint_quote_to_user(
        client,
        state_store,
        bootstrap,
        correlation_id,
        MidenTransfer {
            faucet_id,
            recipient_id,
            amount,
            tx_column: TxHashColumn::MidenRefundTxIds,
            idempotency_prefix: "miden_refund",
        },
    )
    .await
}

pub async fn mint_quote_to_user_legacy(
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

    let faucet_id = bootstrap.faucet_id_for_asset(&record.quote_request.destination_asset)?;
    let recipient_id = parse_account_id(&record.quote_request.recipient)?;
    let _ = json!({});
    mint_quote_to_user(
        client,
        state_store,
        bootstrap,
        correlation_id,
        MidenTransfer {
            faucet_id,
            recipient_id,
            amount: &record.quote_response.quote.amount_out,
            tx_column: TxHashColumn::MidenMintTxIds,
            idempotency_prefix: "miden_mint",
        },
    )
    .await
}
