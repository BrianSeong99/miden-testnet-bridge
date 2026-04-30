use std::{str::FromStr, sync::Arc, time::Duration};

use alloy::primitives::{Address, U256};
use anyhow::{Context, Result, anyhow};
use miden_client::{
    account::AccountId, asset::Asset, keystore::Keystore, note::Note,
    transaction::TransactionRequestBuilder,
};
use serde_json::json;

use crate::{
    chains::{
        evm::{EvmAsset, EvmClient},
        miden::MidenClient,
        miden_bootstrap::{sync_with_retry, wait_for_tx},
        miden_deposit_account::re_derive_outbound_deposit_account,
    },
    core::state::{DynStateStore, TxHashColumn},
};

pub async fn poll_outbound_deposits(
    client: Arc<MidenClient>,
    state_store: DynStateStore,
    evm: Arc<EvmClient>,
    master_seed: [u8; 32],
) -> Result<()> {
    let mut interval = tokio::time::interval(Duration::from_secs(3));
    loop {
        interval.tick().await;
        if let Err(error) = poll_outbound_deposits_once(
            client.clone(),
            state_store.clone(),
            evm.clone(),
            master_seed,
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
) -> Result<()> {
    let quotes = state_store
        .list_miden_tracked_quotes()
        .await
        .context("failed to list Miden outbound quotes")?;

    for quote in quotes {
        if quote.status == "PROCESSING" && !quote.evm_release_tx_hashes.is_empty() {
            let success_key = format!("miden_release_success_{}", quote.correlation_id);
            if state_store
                .record_idempotency_key(quote.correlation_id, &success_key)
                .await?
            {
                state_store
                    .record_event(
                        quote.correlation_id,
                        Some("PROCESSING"),
                        "SUCCESS",
                        "MIDEN_RELEASE_CONFIRMED",
                        None,
                        Some(json!({ "releaseTxHash": quote.evm_release_tx_hashes[0] })),
                    )
                    .await?;
            }
            continue;
        }

        let deposit_account_id = AccountId::from_hex(&quote.miden_deposit_account_id)
            .with_context(|| {
                format!(
                    "invalid deposit account id {}",
                    quote.miden_deposit_account_id
                )
            })?;
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

        let detect_key = format!("miden_deposit_detected_{note_id}");
        if state_store
            .record_idempotency_key(quote.correlation_id, &detect_key)
            .await?
        {
            state_store
                .record_event(
                    quote.correlation_id,
                    Some(&quote.status),
                    "KNOWN_DEPOSIT_TX",
                    "MIDEN_DEPOSIT_DETECTED",
                    None,
                    Some(json!({ "noteId": note_id })),
                )
                .await?;
        }

        let confirm_key = format!("miden_deposit_confirmed_{note_id}");
        if state_store
            .record_idempotency_key(quote.correlation_id, &confirm_key)
            .await?
        {
            state_store
                .record_event(
                    quote.correlation_id,
                    Some("KNOWN_DEPOSIT_TX"),
                    "PENDING_DEPOSIT",
                    "MIDEN_DEPOSIT_CONFIRMED",
                    None,
                    Some(json!({ "noteId": note_id })),
                )
                .await?;
        }

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

        let release_key = format!("miden_release_{}", quote.correlation_id);
        if !state_store
            .record_idempotency_key(quote.correlation_id, &release_key)
            .await?
        {
            continue;
        }

        state_store
            .record_event(
                quote.correlation_id,
                Some("PENDING_DEPOSIT"),
                "PROCESSING",
                "MIDEN_RELEASE_INITIATED",
                None,
                Some(json!({ "noteId": note_id })),
            )
            .await?;

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

        evm.release(
            quote.correlation_id,
            Address::from_str(&quote.recipient)
                .with_context(|| format!("invalid EVM recipient {}", quote.recipient))?,
            evm_asset_for_destination(evm.as_ref(), &quote.destination_asset)?,
            U256::from(amount),
        )
        .await
        .context("failed to release outbound asset on EVM")?;

        let success_key = format!("miden_release_success_{}", quote.correlation_id);
        if state_store
            .record_idempotency_key(quote.correlation_id, &success_key)
            .await?
        {
            let record = state_store
                .get_quote_by_correlation_id(quote.correlation_id)
                .await?
                .ok_or_else(|| anyhow!("quote {} disappeared mid-release", quote.correlation_id))?;
            let release_tx_hash = record
                .evm_release_tx_hashes
                .last()
                .cloned()
                .unwrap_or_default();
            state_store
                .record_event(
                    quote.correlation_id,
                    Some("PROCESSING"),
                    "SUCCESS",
                    "MIDEN_RELEASE_CONFIRMED",
                    None,
                    Some(json!({ "releaseTxHash": release_tx_hash })),
                )
                .await?;
        }
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
    match destination_asset {
        "eth-anvil:eth" => Ok(EvmAsset::NativeEth),
        "eth-anvil:usdc" | "eth-anvil:usdt" | "eth-anvil:btc" => evm
            .token_address(destination_asset)
            .map(EvmAsset::Erc20)
            .ok_or_else(|| anyhow!("missing token address for {destination_asset}")),
        _ => Err(anyhow!(
            "unsupported outbound destination asset {destination_asset}"
        )),
    }
}
