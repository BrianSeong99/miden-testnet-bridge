use std::{str::FromStr, sync::Arc, time::Duration};

use crate::{
    chains::{
        evm::{EvmAsset, EvmClient},
        miden::{MidenClient, parse_account_id},
        miden_bootstrap::{sync_with_retry, wait_for_tx},
        miden_bridge_note::BridgeOutDepositMemo,
        miden_deposit_account::re_derive_outbound_deposit_account,
    },
    core::{
        lifecycle::{DynLifecycle, LifecycleEvent},
        state::{DynStateStore, TxHashColumn},
    },
};
use alloy::primitives::{Address, U256};
use anyhow::{Context, Result, anyhow};
use miden_client::{
    account::AccountId,
    asset::Asset,
    keystore::Keystore,
    note::{Note, NoteType},
    transaction::TransactionRequestBuilder,
};

pub async fn poll_outbound_deposits(
    client: Arc<MidenClient>,
    state_store: DynStateStore,
    evm: Arc<EvmClient>,
    master_seed: [u8; 32],
    lifecycle: DynLifecycle,
) -> Result<()> {
    let mut interval = tokio::time::interval(Duration::from_secs(3));
    loop {
        interval.tick().await;
        if let Err(error) = poll_outbound_deposits_once(
            client.clone(),
            state_store.clone(),
            evm.clone(),
            master_seed,
            lifecycle.clone(),
        )
        .await
        {
            tracing::error!(?error, "failed to poll Miden outbound deposits");
        }
    }
}

pub async fn poll_outbound_deposits_once(
    client: Arc<MidenClient>,
    state_store: DynStateStore,
    evm: Arc<EvmClient>,
    master_seed: [u8; 32],
    lifecycle: DynLifecycle,
) -> Result<()> {
    let quotes = state_store
        .list_miden_tracked_quotes()
        .await
        .context("failed to list Miden outbound quotes")?;

    for quote in quotes {
        if quote.status == "PROCESSING" {
            lifecycle.settle(quote.correlation_id).await?;
            continue;
        }

        let Some(deposit_account_id) = quote.miden_deposit_account_id.as_deref() else {
            poll_public_bridge_note_quote(
                client.as_ref(),
                state_store.as_ref(),
                evm.as_ref(),
                lifecycle.as_ref(),
                &quote,
            )
            .await?;
            continue;
        };
        let deposit_account_id = AccountId::from_hex(deposit_account_id)
            .with_context(|| format!("invalid deposit account id {deposit_account_id}"))?;
        let mut inner = client.open().await?;
        sync_with_retry(&mut inner).await?;

        let consumable = inner
            .get_consumable_notes(Some(deposit_account_id))
            .await
            .with_context(|| {
                format!(
                    "failed to load consumable notes for {}",
                    quote.deposit_address
                )
            })?;
        let Some((note_record, _)) = consumable.into_iter().next() else {
            continue;
        };
        let note: Note = note_record
            .try_into()
            .context("failed to decode consumable deposit note")?;
        let note_id = note.id().to_hex();
        let (asset, amount) = extract_fungible_asset(&note)?;

        lifecycle
            .apply(LifecycleEvent::MidenDepositDetected {
                correlation_id: quote.correlation_id,
                note_id: note_id.clone(),
            })
            .await?;
        lifecycle
            .apply(LifecycleEvent::MidenDepositConfirmed {
                correlation_id: quote.correlation_id,
                note_id: note_id.clone(),
                amount: amount.to_string(),
            })
            .await?;

        let consume_key = format!("miden_consume_{note_id}");
        if state_store
            .record_idempotency_key(quote.correlation_id, &consume_key)
            .await?
        {
            let (account, secret_key) =
                re_derive_outbound_deposit_account(&master_seed, quote.correlation_id)?;
            let keystore = client.open_keystore()?;
            keystore.add_key(&secret_key, account.id()).await?;
            if inner.get_account(account.id()).await?.is_none() {
                inner.add_account(&account, false).await?;
            }

            let tx_request = TransactionRequestBuilder::new().build_consume_notes(vec![note])?;
            let tx_id = inner
                .submit_new_transaction(account.id(), tx_request)
                .await
                .context("failed to submit outbound consume transaction")?;
            let tx_id_string = tx_id.to_string();
            state_store
                .append_tx_hash(
                    quote.correlation_id,
                    TxHashColumn::MidenConsumeTxIds,
                    &tx_id_string,
                )
                .await?;
            wait_for_tx(&mut inner, tx_id).await?;
        }

        let expected_faucet = evm
            .miden_faucet_account_id(&quote.origin_asset)
            .await
            .context("failed to resolve expected faucet for outbound asset")?;
        if asset.faucet_id() != expected_faucet {
            return Err(anyhow!(
                "unexpected faucet id {} for quote {}",
                asset.faucet_id(),
                quote.correlation_id
            ));
        }

        let _ = (
            Address::from_str(&quote.recipient)
                .with_context(|| format!("invalid EVM recipient {}", quote.recipient))?,
            evm_asset_for_destination(evm.as_ref(), &quote.destination_asset)?,
            U256::from(amount),
        );
        lifecycle.settle(quote.correlation_id).await?;
    }

    Ok(())
}

async fn poll_public_bridge_note_quote(
    client: &MidenClient,
    state_store: &dyn crate::core::state::StateStore,
    evm: &EvmClient,
    lifecycle: &dyn crate::core::lifecycle::Lifecycle,
    quote: &crate::core::state::MidenTrackedQuote,
) -> Result<()> {
    let deposit_memo = quote
        .deposit_memo
        .as_deref()
        .ok_or_else(|| anyhow!("Miden bridge-note quote is missing deposit memo"))?;
    let memo = BridgeOutDepositMemo::from_deposit_memo(deposit_memo)
        .context("failed to decode Miden bridge-note deposit memo")?;
    let bridge_account_id = parse_account_id(&memo.bridge_account_id)?;
    tracing::info!(
        correlation_id = %quote.correlation_id,
        bridge_account_id = %memo.bridge_account_id,
        quote_hash = %memo.storage.quote_hash,
        storage_items = memo.storage.storage_items.len(),
        "polling Miden BridgeOutV1 public notes"
    );
    let expected_faucet = evm
        .miden_faucet_account_id(&quote.origin_asset)
        .await
        .context("failed to resolve expected faucet for bridge note")?;

    let mut inner = client.open().await?;
    sync_with_retry(&mut inner).await?;

    let consumable = inner
        .get_consumable_notes(Some(bridge_account_id))
        .await
        .with_context(|| {
            format!(
                "failed to load public bridge notes for {}",
                quote.deposit_address
            )
        })?;

    for (note_record, _) in consumable {
        let note: Note = note_record
            .try_into()
            .context("failed to decode public bridge note")?;
        let note_id = note.id().to_hex();
        if note.metadata().note_type() != NoteType::Public {
            tracing::debug!(
                correlation_id = %quote.correlation_id,
                note_id = %note_id,
                note_type = ?note.metadata().note_type(),
                "rejected Miden bridge note candidate: note is not public"
            );
            continue;
        }
        if !memo.matches_note_storage(note.storage().items())? {
            tracing::debug!(
                correlation_id = %quote.correlation_id,
                note_id = %note_id,
                expected_storage_items = ?memo.storage.storage_items,
                actual_storage_len = note.storage().items().len(),
                "rejected Miden bridge note candidate: storage mismatch"
            );
            continue;
        }

        let (asset, amount) = extract_fungible_asset(&note)?;
        if asset.faucet_id() != expected_faucet {
            tracing::debug!(
                correlation_id = %quote.correlation_id,
                note_id = %note_id,
                actual_faucet = %asset.faucet_id(),
                expected_faucet = %expected_faucet,
                "rejected Miden bridge note candidate: faucet mismatch"
            );
            continue;
        }
        if amount.to_string() != quote.amount_in {
            tracing::debug!(
                correlation_id = %quote.correlation_id,
                note_id = %note_id,
                actual_amount = amount,
                expected_amount = %quote.amount_in,
                "rejected Miden bridge note candidate: amount mismatch"
            );
            continue;
        }

        tracing::info!(
            correlation_id = %quote.correlation_id,
            note_id = %note_id,
            quote_hash = %memo.storage.quote_hash,
            amount = amount,
            faucet = %asset.faucet_id(),
            "matched Miden BridgeOutV1 public note"
        );
        lifecycle
            .apply(LifecycleEvent::MidenDepositDetected {
                correlation_id: quote.correlation_id,
                note_id: note_id.clone(),
            })
            .await?;
        lifecycle
            .apply(LifecycleEvent::MidenDepositConfirmed {
                correlation_id: quote.correlation_id,
                note_id: note_id.clone(),
                amount: amount.to_string(),
            })
            .await?;

        let consume_key = format!("miden_consume_{note_id}");
        if state_store
            .record_idempotency_key(quote.correlation_id, &consume_key)
            .await?
        {
            let tx_request = TransactionRequestBuilder::new().build_consume_notes(vec![note])?;
            let tx_id = inner
                .submit_new_transaction(bridge_account_id, tx_request)
                .await
                .context("failed to submit public bridge-note consume transaction")?;
            let tx_id_string = tx_id.to_string();
            tracing::info!(
                correlation_id = %quote.correlation_id,
                note_id = %note_id,
                tx_id = %tx_id_string,
                "submitted Miden BridgeOutV1 consume transaction"
            );
            state_store
                .append_tx_hash(
                    quote.correlation_id,
                    TxHashColumn::MidenConsumeTxIds,
                    &tx_id_string,
                )
                .await?;
            wait_for_tx(&mut inner, tx_id).await?;
            tracing::info!(
                correlation_id = %quote.correlation_id,
                note_id = %note_id,
                tx_id = %tx_id_string,
                "confirmed Miden BridgeOutV1 consume transaction"
            );
        }

        lifecycle.settle(quote.correlation_id).await?;
        break;
    }

    Ok(())
}

pub fn parse_persisted_miden_seed_hex(seed_hex: &str) -> Result<([u8; 32], [u8; 32])> {
    let mut parts = seed_hex.split(':');
    let init = parts
        .next()
        .ok_or_else(|| anyhow!("missing account seed"))?;
    let auth = parts.next().ok_or_else(|| anyhow!("missing auth seed"))?;
    if parts.next().is_some() {
        return Err(anyhow!(
            "unexpected extra separator in persisted seed payload"
        ));
    }

    Ok((decode_seed(init)?, decode_seed(auth)?))
}

fn decode_seed(seed_hex: &str) -> Result<[u8; 32]> {
    let bytes =
        alloy::hex::decode(seed_hex).with_context(|| format!("invalid seed hex {seed_hex}"))?;
    bytes
        .try_into()
        .map_err(|_| anyhow!("seed must decode into 32 bytes"))
}

fn extract_fungible_asset(note: &Note) -> Result<(miden_client::asset::FungibleAsset, u64)> {
    let asset = note
        .assets()
        .iter()
        .next()
        .ok_or_else(|| anyhow!("deposit note does not contain any assets"))?;
    match asset {
        Asset::Fungible(asset) => Ok((*asset, asset.amount())),
        Asset::NonFungible(_) => Err(anyhow!("non-fungible deposits are not supported")),
    }
}

fn evm_asset_for_destination(evm: &EvmClient, destination_asset: &str) -> Result<EvmAsset> {
    evm.asset_for_asset_id(destination_asset)
        .with_context(|| format!("unsupported outbound destination asset {destination_asset}"))
}
