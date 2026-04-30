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
        let record = state_store
            .get_quote_by_correlation_id(correlation_id)
            .await
            .context("failed to reload existing quote")?
            .ok_or_else(|| anyhow!("quote {correlation_id} not found"))?;
        let existing = match transfer.tx_column {
            TxHashColumn::MidenMintTxIds => record.miden_mint_tx_ids.last(),
            TxHashColumn::MidenRefundTxIds => record.miden_refund_tx_ids.last(),
            _ => None,
        };
        return existing
            .cloned()
            .ok_or_else(|| anyhow!("quote {correlation_id} already processed without tx id"));
    }

    let tx_id = mint_to_user(
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

    Ok(tx_id_string)
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
