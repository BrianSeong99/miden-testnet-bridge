use std::sync::Arc;

use async_trait::async_trait;
use axum::http::StatusCode;
use serde_json::Value;
use sqlx::{query::query, row::Row};
use sqlx_postgres::{PgPool, PgPoolOptions, PgRow};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::types::{QuoteRequest, QuoteResponse};

#[derive(Clone)]
pub struct PostgresStateStore {
    pool: PgPool,
}

impl PostgresStateStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuoteRecord {
    pub correlation_id: Uuid,
    pub quote_response: QuoteResponse,
    pub status: String,
    pub updated_at: OffsetDateTime,
    pub evm_deposit_tx_hashes: Vec<String>,
    pub evm_release_tx_hashes: Vec<String>,
    pub miden_mint_tx_ids: Vec<String>,
    pub miden_consume_tx_ids: Vec<String>,
    pub evm_refund_tx_hashes: Vec<String>,
    pub miden_refund_tx_ids: Vec<String>,
    pub intent_hashes: Vec<String>,
    pub near_tx_hashes: Vec<String>,
    pub idempotency_keys: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxHashColumn {
    EvmDepositTxHashes,
    EvmReleaseTxHashes,
    MidenMintTxIds,
    MidenConsumeTxIds,
    EvmRefundTxHashes,
    MidenRefundTxIds,
    IntentHashes,
    NearTxHashes,
}

impl TxHashColumn {
    fn as_str(self) -> &'static str {
        match self {
            Self::EvmDepositTxHashes => "evm_deposit_tx_hashes",
            Self::EvmReleaseTxHashes => "evm_release_tx_hashes",
            Self::MidenMintTxIds => "miden_mint_tx_ids",
            Self::MidenConsumeTxIds => "miden_consume_tx_ids",
            Self::EvmRefundTxHashes => "evm_refund_tx_hashes",
            Self::MidenRefundTxIds => "miden_refund_tx_ids",
            Self::IntentHashes => "intent_hashes",
            Self::NearTxHashes => "near_tx_hashes",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StateStoreError {
    #[error("quote is missing correlation id")]
    MissingCorrelationId,
    #[error("quote is missing deposit address")]
    MissingDepositAddress,
    #[error("failed to parse correlation id: {0}")]
    InvalidCorrelationId(#[source] uuid::Error),
    #[error("failed to parse timestamp: {0}")]
    InvalidTimestamp(#[source] time::error::Parse),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

impl StateStoreError {
    pub fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

#[async_trait]
pub trait StateStore: Send + Sync {
    async fn insert_quote(
        &self,
        quote: &QuoteResponse,
        request: &QuoteRequest,
    ) -> Result<(), StateStoreError>;

    async fn get_quote_by_deposit(
        &self,
        address: &str,
        memo: Option<&str>,
    ) -> Result<Option<QuoteRecord>, StateStoreError>;

    async fn record_event(
        &self,
        correlation_id: Uuid,
        from_status: Option<&str>,
        to_status: &str,
        event_kind: &str,
        reason: Option<&str>,
        metadata: Option<Value>,
    ) -> Result<(), StateStoreError>;

    async fn record_idempotency_key(
        &self,
        correlation_id: Uuid,
        key: &str,
    ) -> Result<bool, StateStoreError>;

    async fn append_tx_hash(
        &self,
        correlation_id: Uuid,
        column: TxHashColumn,
        hash: &str,
    ) -> Result<(), StateStoreError>;

    async fn ping(&self) -> Result<(), StateStoreError>;
}

#[async_trait]
impl StateStore for PostgresStateStore {
    async fn insert_quote(
        &self,
        quote: &QuoteResponse,
        request: &QuoteRequest,
    ) -> Result<(), StateStoreError> {
        let correlation_id = Uuid::parse_str(&quote.correlation_id)
            .map_err(StateStoreError::InvalidCorrelationId)?;
        let deposit_address = quote
            .quote
            .deposit_address
            .as_deref()
            .ok_or(StateStoreError::MissingDepositAddress)?;
        let deadline = parse_optional_timestamp(quote.quote.deadline.as_deref())?;
        let request_json = serde_json::to_value(request)?;
        let response_json = serde_json::to_value(quote)?;

        let mut tx = self.pool.begin().await?;
        query::<sqlx_postgres::Postgres>(
            r#"
            INSERT INTO quotes (
                correlation_id,
                deposit_address,
                deposit_memo,
                status,
                quote_request_json,
                quote_response_json,
                deadline
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(correlation_id)
        .bind(deposit_address)
        .bind(quote.quote.deposit_memo.as_deref())
        .bind("PENDING_DEPOSIT")
        .bind(request_json)
        .bind(response_json)
        .bind(deadline)
        .execute(tx.as_mut())
        .await?;

        query::<sqlx_postgres::Postgres>(
            "INSERT INTO chain_artifacts (correlation_id) VALUES ($1)",
        )
        .bind(correlation_id)
        .execute(tx.as_mut())
        .await?;

        query::<sqlx_postgres::Postgres>(
            r#"
            INSERT INTO lifecycle_events (
                correlation_id,
                from_status,
                to_status,
                event_kind,
                reason,
                metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(correlation_id)
        .bind(Option::<&str>::None)
        .bind("PENDING_DEPOSIT")
        .bind("QUOTE_CREATED")
        .bind(Option::<&str>::None)
        .bind(Option::<Value>::None)
        .execute(tx.as_mut())
        .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn get_quote_by_deposit(
        &self,
        address: &str,
        memo: Option<&str>,
    ) -> Result<Option<QuoteRecord>, StateStoreError> {
        let row = query::<sqlx_postgres::Postgres>(
            r#"
            SELECT
                q.correlation_id,
                q.quote_response_json,
                q.status,
                q.updated_at,
                c.evm_deposit_tx_hashes,
                c.evm_release_tx_hashes,
                c.miden_mint_tx_ids,
                c.miden_consume_tx_ids,
                c.evm_refund_tx_hashes,
                c.miden_refund_tx_ids,
                c.intent_hashes,
                c.near_tx_hashes,
                c.idempotency_keys
            FROM quotes q
            INNER JOIN chain_artifacts c ON c.correlation_id = q.correlation_id
            WHERE q.deposit_address = $1
              AND q.deposit_memo IS NOT DISTINCT FROM $2
            ORDER BY q.created_at DESC
            LIMIT 1
            "#,
        )
        .bind(address)
        .bind(memo)
        .fetch_optional(&self.pool)
        .await?;

        row.map(map_quote_record).transpose()
    }

    async fn record_event(
        &self,
        correlation_id: Uuid,
        from_status: Option<&str>,
        to_status: &str,
        event_kind: &str,
        reason: Option<&str>,
        metadata: Option<Value>,
    ) -> Result<(), StateStoreError> {
        let mut tx = self.pool.begin().await?;
        query::<sqlx_postgres::Postgres>(
            r#"
            INSERT INTO lifecycle_events (
                correlation_id,
                from_status,
                to_status,
                event_kind,
                reason,
                metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(correlation_id)
        .bind(from_status)
        .bind(to_status)
        .bind(event_kind)
        .bind(reason)
        .bind(metadata)
        .execute(tx.as_mut())
        .await?;

        query::<sqlx_postgres::Postgres>(
            r#"
            UPDATE quotes
            SET status = $2,
                updated_at = NOW()
            WHERE correlation_id = $1
            "#,
        )
        .bind(correlation_id)
        .bind(to_status)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn record_idempotency_key(
        &self,
        correlation_id: Uuid,
        key: &str,
    ) -> Result<bool, StateStoreError> {
        let mut tx = self.pool.begin().await?;
        let current = query::<sqlx_postgres::Postgres>(
            r#"
            SELECT idempotency_keys
            FROM chain_artifacts
            WHERE correlation_id = $1
            FOR UPDATE
            "#,
        )
        .bind(correlation_id)
        .fetch_one(tx.as_mut())
        .await?;

        let mut keys = jsonb_array_to_vec(&current, "idempotency_keys")?;
        if keys.iter().any(|existing| existing == key) {
            tx.commit().await?;
            return Ok(false);
        }

        keys.push(key.to_owned());
        query::<sqlx_postgres::Postgres>(
            r#"
            UPDATE chain_artifacts
            SET idempotency_keys = $2,
                updated_at = NOW()
            WHERE correlation_id = $1
            "#,
        )
        .bind(correlation_id)
        .bind(serde_json::to_value(keys)?)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    async fn append_tx_hash(
        &self,
        correlation_id: Uuid,
        column: TxHashColumn,
        hash: &str,
    ) -> Result<(), StateStoreError> {
        let sql = format!(
            r#"
            UPDATE chain_artifacts
            SET {column} = {column} || to_jsonb($2::text),
                updated_at = NOW()
            WHERE correlation_id = $1
            "#,
            column = column.as_str()
        );

        query::<sqlx_postgres::Postgres>(&sql)
            .bind(correlation_id)
            .bind(hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn ping(&self) -> Result<(), StateStoreError> {
        query::<sqlx_postgres::Postgres>("SELECT 1")
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

pub type DynStateStore = Arc<dyn StateStore>;

fn map_quote_record(row: PgRow) -> Result<QuoteRecord, StateStoreError> {
    Ok(QuoteRecord {
        correlation_id: row.try_get("correlation_id")?,
        quote_response: serde_json::from_value(row.try_get("quote_response_json")?)?,
        status: row.try_get("status")?,
        updated_at: row.try_get("updated_at")?,
        evm_deposit_tx_hashes: jsonb_array_to_vec(&row, "evm_deposit_tx_hashes")?,
        evm_release_tx_hashes: jsonb_array_to_vec(&row, "evm_release_tx_hashes")?,
        miden_mint_tx_ids: jsonb_array_to_vec(&row, "miden_mint_tx_ids")?,
        miden_consume_tx_ids: jsonb_array_to_vec(&row, "miden_consume_tx_ids")?,
        evm_refund_tx_hashes: jsonb_array_to_vec(&row, "evm_refund_tx_hashes")?,
        miden_refund_tx_ids: jsonb_array_to_vec(&row, "miden_refund_tx_ids")?,
        intent_hashes: jsonb_array_to_vec(&row, "intent_hashes")?,
        near_tx_hashes: jsonb_array_to_vec(&row, "near_tx_hashes")?,
        idempotency_keys: jsonb_array_to_vec(&row, "idempotency_keys")?,
    })
}

fn jsonb_array_to_vec(row: &PgRow, column: &str) -> Result<Vec<String>, StateStoreError> {
    let value: Value = row.try_get(column)?;
    serde_json::from_value(value).map_err(StateStoreError::from)
}

fn parse_optional_timestamp(
    value: Option<&str>,
) -> Result<Option<OffsetDateTime>, StateStoreError> {
    value
        .map(|timestamp| OffsetDateTime::parse(timestamp, &Rfc3339))
        .transpose()
        .map_err(StateStoreError::InvalidTimestamp)
}

pub async fn connect_pool(database_url: &str, max_connections: u32) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await
}
