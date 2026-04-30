use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde_json::Value;
use time::OffsetDateTime;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    core::state::{DynStateStore, QuoteRecord, StateStore, StateStoreError, TxHashColumn},
    types::{QuoteRequest, QuoteResponse},
};

#[derive(Default)]
struct MemoryStoreState {
    quotes: HashMap<(String, Option<String>), QuoteRecord>,
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
                quote_response: quote.clone(),
                status: "PENDING_DEPOSIT".to_owned(),
                updated_at: OffsetDateTime::now_utc(),
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

    async fn record_event(
        &self,
        correlation_id: Uuid,
        _from_status: Option<&str>,
        to_status: &str,
        _event_kind: &str,
        _reason: Option<&str>,
        _metadata: Option<Value>,
    ) -> Result<(), StateStoreError> {
        for quote in self.state.lock().await.quotes.values_mut() {
            if quote.correlation_id == correlation_id {
                quote.status = to_status.to_owned();
                quote.updated_at = OffsetDateTime::now_utc();
            }
        }

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

    async fn ping(&self) -> Result<(), StateStoreError> {
        if self.ping_ok {
            Ok(())
        } else {
            Err(StateStoreError::MissingCorrelationId)
        }
    }
}
