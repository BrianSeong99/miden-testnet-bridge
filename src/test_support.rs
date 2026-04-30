use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde_json::Value;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    core::state::{
        DeadlineScanQuote, DynStateStore, EvmTrackedQuote, MidenBootstrapRecord, MidenTrackedQuote,
        QuoteRecord, StateStore, StateStoreError, StuckProcessingQuote, TxHashColumn,
    },
    types::{QuoteRequest, QuoteResponse},
};

#[derive(Clone)]
struct MemoryLifecycleEvent {
    correlation_id: Uuid,
    event_kind: String,
    metadata: Option<Value>,
    created_at: OffsetDateTime,
}

#[derive(Default)]
struct MemoryStoreState {
    quotes: HashMap<(String, Option<String>), QuoteRecord>,
    miden_bootstrap: Option<MidenBootstrapRecord>,
    lifecycle_events: Vec<MemoryLifecycleEvent>,
}

#[derive(Clone)]
struct MemoryStateStore {
    state: Arc<Mutex<MemoryStoreState>>,
    ping_ok: bool,
}

pub fn memory_state() -> DynStateStore {
    Arc::new(MemoryStateStore {
        state: Arc::new(Mutex::new(MemoryStoreState::default())),
        ping_ok: true,
    })
}

pub fn failing_memory_state() -> DynStateStore {
    Arc::new(MemoryStateStore {
        state: Arc::new(Mutex::new(MemoryStoreState::default())),
        ping_ok: false,
    })
}

#[async_trait]
impl StateStore for MemoryStateStore {
    async fn insert_quote(
        &self,
        quote: &QuoteResponse,
        _request: &QuoteRequest,
    ) -> Result<(), StateStoreError> {
        let correlation_id = Uuid::parse_str(&quote.correlation_id)
            .map_err(StateStoreError::InvalidCorrelationId)?;
        let key = (
            quote
                .quote
                .deposit_address
                .clone()
                .ok_or(StateStoreError::MissingDepositAddress)?,
            quote.quote.deposit_memo.clone(),
        );

        self.state.lock().await.quotes.insert(
            key,
            QuoteRecord {
                correlation_id,
                quote_request: _request.clone(),
                quote_response: quote.clone(),
                status: "PENDING_DEPOSIT".to_owned(),
                updated_at: OffsetDateTime::now_utc(),
                evm_deposit_derivation_path: None,
                miden_deposit_account_id: None,
                miden_deposit_seed_hex: None,
                evm_deposit_tx_hashes: Vec::new(),
                evm_release_tx_hashes: Vec::new(),
                miden_mint_tx_ids: Vec::new(),
                miden_consume_tx_ids: Vec::new(),
                evm_refund_tx_hashes: Vec::new(),
                miden_refund_tx_ids: Vec::new(),
                intent_hashes: Vec::new(),
                near_tx_hashes: Vec::new(),
                idempotency_keys: Vec::new(),
            },
        );

        Ok(())
    }

    async fn get_quote_by_deposit(
        &self,
        address: &str,
        memo: Option<&str>,
    ) -> Result<Option<QuoteRecord>, StateStoreError> {
        Ok(self
            .state
            .lock()
            .await
            .quotes
            .get(&(address.to_owned(), memo.map(ToOwned::to_owned)))
            .cloned())
    }

    async fn get_quote_by_correlation_id(
        &self,
        correlation_id: Uuid,
    ) -> Result<Option<QuoteRecord>, StateStoreError> {
        Ok(self
            .state
            .lock()
            .await
            .quotes
            .values()
            .find(|quote| quote.correlation_id == correlation_id)
            .cloned())
    }

    async fn record_event(
        &self,
        correlation_id: Uuid,
        _from_status: Option<&str>,
        to_status: &str,
        event_kind: &str,
        _reason: Option<&str>,
        metadata: Option<Value>,
    ) -> Result<(), StateStoreError> {
        let mut state = self.state.lock().await;
        for quote in state.quotes.values_mut() {
            if quote.correlation_id == correlation_id {
                quote.status = to_status.to_owned();
                quote.updated_at = OffsetDateTime::now_utc();
            }
        }
        state.lifecycle_events.push(MemoryLifecycleEvent {
            correlation_id,
            event_kind: event_kind.to_owned(),
            metadata,
            created_at: OffsetDateTime::now_utc(),
        });

        Ok(())
    }

    async fn record_idempotency_key(
        &self,
        correlation_id: Uuid,
        key: &str,
    ) -> Result<bool, StateStoreError> {
        for quote in self.state.lock().await.quotes.values_mut() {
            if quote.correlation_id == correlation_id {
                if quote
                    .idempotency_keys
                    .iter()
                    .any(|existing| existing == key)
                {
                    return Ok(false);
                }
                quote.idempotency_keys.push(key.to_owned());
                return Ok(true);
            }
        }

        Ok(false)
    }

    async fn append_tx_hash(
        &self,
        correlation_id: Uuid,
        column: TxHashColumn,
        hash: &str,
    ) -> Result<(), StateStoreError> {
        for quote in self.state.lock().await.quotes.values_mut() {
            if quote.correlation_id == correlation_id {
                let target = match column {
                    TxHashColumn::EvmDepositTxHashes => &mut quote.evm_deposit_tx_hashes,
                    TxHashColumn::EvmReleaseTxHashes => &mut quote.evm_release_tx_hashes,
                    TxHashColumn::MidenMintTxIds => &mut quote.miden_mint_tx_ids,
                    TxHashColumn::MidenConsumeTxIds => &mut quote.miden_consume_tx_ids,
                    TxHashColumn::EvmRefundTxHashes => &mut quote.evm_refund_tx_hashes,
                    TxHashColumn::MidenRefundTxIds => &mut quote.miden_refund_tx_ids,
                    TxHashColumn::IntentHashes => &mut quote.intent_hashes,
                    TxHashColumn::NearTxHashes => &mut quote.near_tx_hashes,
                };
                target.push(hash.to_owned());
            }
        }

        Ok(())
    }

    async fn set_evm_deposit_derivation_path(
        &self,
        correlation_id: Uuid,
        derivation_path: &str,
    ) -> Result<(), StateStoreError> {
        for quote in self.state.lock().await.quotes.values_mut() {
            if quote.correlation_id == correlation_id {
                quote.updated_at = OffsetDateTime::now_utc();
                quote.evm_deposit_derivation_path = Some(derivation_path.to_owned());
                return Ok(());
            }
        }

        Ok(())
    }

    async fn set_miden_deposit_account(
        &self,
        correlation_id: Uuid,
        account_id: &str,
        seed_hex: &str,
    ) -> Result<(), StateStoreError> {
        for quote in self.state.lock().await.quotes.values_mut() {
            if quote.correlation_id == correlation_id {
                quote.updated_at = OffsetDateTime::now_utc();
                quote.miden_deposit_account_id = Some(account_id.to_owned());
                quote.miden_deposit_seed_hex = Some(seed_hex.to_owned());
                return Ok(());
            }
        }

        Ok(())
    }

    async fn list_evm_tracked_quotes(&self) -> Result<Vec<EvmTrackedQuote>, StateStoreError> {
        Ok(self
            .state
            .lock()
            .await
            .quotes
            .values()
            .filter(|quote| {
                matches!(
                    quote.status.as_str(),
                    "PENDING_DEPOSIT" | "KNOWN_DEPOSIT_TX" | "PROCESSING"
                )
            })
            .map(|quote| EvmTrackedQuote {
                correlation_id: quote.correlation_id,
                deposit_address: quote
                    .quote_response
                    .quote
                    .deposit_address
                    .clone()
                    .expect("memory quote should have deposit address"),
                origin_asset: quote.quote_request.origin_asset.clone(),
                amount_in: quote.quote_response.quote.amount_in.clone(),
                status: quote.status.clone(),
                evm_deposit_derivation_path: quote.evm_deposit_derivation_path.clone(),
            })
            .collect())
    }

    async fn list_miden_tracked_quotes(&self) -> Result<Vec<MidenTrackedQuote>, StateStoreError> {
        Ok(self
            .state
            .lock()
            .await
            .quotes
            .values()
            .filter_map(|quote| {
                let account_id = quote.miden_deposit_account_id.clone()?;
                let seed_hex = quote.miden_deposit_seed_hex.clone()?;
                if !matches!(
                    quote.status.as_str(),
                    "PENDING_DEPOSIT" | "KNOWN_DEPOSIT_TX" | "PROCESSING"
                ) {
                    return None;
                }
                Some(MidenTrackedQuote {
                    correlation_id: quote.correlation_id,
                    deposit_address: quote
                        .quote_response
                        .quote
                        .deposit_address
                        .clone()
                        .expect("memory quote should have deposit address"),
                    origin_asset: quote.quote_request.origin_asset.clone(),
                    destination_asset: quote.quote_request.destination_asset.clone(),
                    recipient: quote.quote_request.recipient.clone(),
                    amount_in: quote.quote_response.quote.amount_in.clone(),
                    status: quote.status.clone(),
                    miden_deposit_account_id: account_id,
                    miden_deposit_seed_hex: seed_hex,
                    evm_release_tx_hashes: quote.evm_release_tx_hashes.clone(),
                    miden_consume_tx_ids: quote.miden_consume_tx_ids.clone(),
                })
            })
            .collect())
    }

    async fn get_miden_bootstrap(&self) -> Result<Option<MidenBootstrapRecord>, StateStoreError> {
        Ok(self.state.lock().await.miden_bootstrap.clone())
    }

    async fn upsert_miden_bootstrap(
        &self,
        record: &MidenBootstrapRecord,
    ) -> Result<(), StateStoreError> {
        self.state.lock().await.miden_bootstrap = Some(record.clone());
        Ok(())
    }

    async fn get_last_confirmed_deposit_amount(
        &self,
        correlation_id: Uuid,
    ) -> Result<Option<String>, StateStoreError> {
        Ok(self
            .state
            .lock()
            .await
            .lifecycle_events
            .iter()
            .rev()
            .find(|event| {
                event.correlation_id == correlation_id
                    && matches!(
                        event.event_kind.as_str(),
                        "EVM_DEPOSIT_CONFIRMED" | "MIDEN_DEPOSIT_CONFIRMED"
                    )
            })
            .and_then(|event| event.metadata.as_ref())
            .and_then(|metadata| metadata.get("amount"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned))
    }

    async fn list_deadline_expired_quotes(
        &self,
        now: OffsetDateTime,
    ) -> Result<Vec<DeadlineScanQuote>, StateStoreError> {
        let state = self.state.lock().await;
        let mut quotes: Vec<_> = state
            .quotes
            .values()
            .filter_map(|quote| {
                let deadline = quote.quote_response.quote.deadline.as_deref()?;
                let deadline = OffsetDateTime::parse(deadline, &Rfc3339).ok()?;
                if deadline > now {
                    return None;
                }
                if !matches!(
                    quote.status.as_str(),
                    "PENDING_DEPOSIT" | "KNOWN_DEPOSIT_TX" | "PROCESSING"
                ) {
                    return None;
                }
                Some(DeadlineScanQuote {
                    correlation_id: quote.correlation_id,
                    status: quote.status.clone(),
                })
            })
            .collect();
        quotes.sort_by_key(|quote| quote.correlation_id);
        Ok(quotes)
    }

    async fn list_stuck_processing_quotes(
        &self,
        updated_before: OffsetDateTime,
    ) -> Result<Vec<StuckProcessingQuote>, StateStoreError> {
        let state = self.state.lock().await;
        let mut quotes: Vec<_> = state
            .quotes
            .values()
            .filter(|quote| quote.status == "PROCESSING" && quote.updated_at < updated_before)
            .map(|quote| StuckProcessingQuote {
                correlation_id: quote.correlation_id,
                status: quote.status.clone(),
                updated_at: quote.updated_at,
                last_event_at: state
                    .lifecycle_events
                    .iter()
                    .rev()
                    .find(|event| event.correlation_id == quote.correlation_id)
                    .map(|event| event.created_at),
            })
            .collect();
        quotes.sort_by_key(|quote| quote.updated_at);
        Ok(quotes)
    }

    async fn ping(&self) -> Result<(), StateStoreError> {
        if self.ping_ok {
            Ok(())
        } else {
            Err(StateStoreError::MissingCorrelationId)
        }
    }
}
