use std::{str::FromStr, sync::Arc};

use alloy::primitives::{Address, U256};
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    chains::{
        evm::{EvmAsset, EvmClient},
        miden::{MidenClient, asset_symbol, is_evm_asset_id, parse_account_id},
        miden_bootstrap::bootstrap_state_from_record,
        miden_inbound::{MidenTransfer, mint_quote_to_user, refund_quote_to_origin},
    },
    core::{
        pricer::Pricer,
        state::{DynStateStore, QuoteRecord, TxHashColumn},
    },
    types::{Quote, SwapType},
};

pub type DynLifecycle = Arc<dyn Lifecycle>;

#[derive(Debug, Clone, PartialEq)]
pub enum LifecycleEvent {
    EvmDepositDetected {
        correlation_id: Uuid,
        tx_hash: String,
    },
    EvmDepositConfirmed {
        correlation_id: Uuid,
        tx_hash: String,
        amount: String,
    },
    MidenDepositDetected {
        correlation_id: Uuid,
        note_id: String,
    },
    MidenDepositConfirmed {
        correlation_id: Uuid,
        note_id: String,
        amount: String,
    },
    SettlementInitiated {
        correlation_id: Uuid,
    },
    SettlementSucceeded {
        correlation_id: Uuid,
        tx_hash: String,
    },
    SettlementFailed {
        correlation_id: Uuid,
        reason: String,
    },
    SlippageExceeded {
        correlation_id: Uuid,
        expected_min: String,
        actual: String,
    },
    IncompleteDeposit {
        correlation_id: Uuid,
        deposit_amount: String,
        min_required: String,
    },
    DeadlineExpired {
        correlation_id: Uuid,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexInputDecision {
    Accept,
    AcceptAboveUpper,
    IncompleteDeposit,
}

#[async_trait]
pub trait Lifecycle: Send + Sync {
    async fn apply(&self, event: LifecycleEvent) -> Result<()>;
    async fn settle(&self, correlation_id: Uuid) -> Result<()>;
    async fn refund(&self, correlation_id: Uuid) -> Result<()>;
}

#[derive(Clone)]
pub struct DefaultLifecycle {
    pub state: DynStateStore,
    pub pricer: Arc<dyn Pricer>,
    pub evm: Arc<EvmClient>,
    pub miden: Arc<MidenClient>,
}

impl DefaultLifecycle {
    pub fn new(
        state: DynStateStore,
        pricer: Arc<dyn Pricer>,
        evm: Arc<EvmClient>,
        miden: Arc<MidenClient>,
    ) -> Self {
        Self {
            state,
            pricer,
            evm,
            miden,
        }
    }

    async fn load_quote(&self, correlation_id: Uuid) -> Result<QuoteRecord> {
        self.state
            .get_quote_by_correlation_id(correlation_id)
            .await
            .context("failed to load quote from state store")?
            .ok_or_else(|| anyhow!("quote {correlation_id} not found"))
    }

    async fn persist_transition(
        &self,
        record: &QuoteRecord,
        event: &LifecycleEvent,
        to_status: &str,
        reason_override: Option<&str>,
    ) -> Result<()> {
        if let Some((column, hash)) = tx_hash_to_append(record, event) {
            let already_present = match column {
                TxHashColumn::EvmDepositTxHashes => record
                    .evm_deposit_tx_hashes
                    .iter()
                    .any(|value| value == hash),
                TxHashColumn::EvmReleaseTxHashes => record
                    .evm_release_tx_hashes
                    .iter()
                    .any(|value| value == hash),
                TxHashColumn::MidenMintTxIds => {
                    record.miden_mint_tx_ids.iter().any(|value| value == hash)
                }
                TxHashColumn::MidenConsumeTxIds => record
                    .miden_consume_tx_ids
                    .iter()
                    .any(|value| value == hash),
                TxHashColumn::EvmRefundTxHashes => record
                    .evm_refund_tx_hashes
                    .iter()
                    .any(|value| value == hash),
                TxHashColumn::MidenRefundTxIds => {
                    record.miden_refund_tx_ids.iter().any(|value| value == hash)
                }
                TxHashColumn::IntentHashes => {
                    record.intent_hashes.iter().any(|value| value == hash)
                }
                TxHashColumn::NearTxHashes => {
                    record.near_tx_hashes.iter().any(|value| value == hash)
                }
            };
            if !already_present {
                self.state
                    .append_tx_hash(record.correlation_id, column, hash)
                    .await
                    .context("failed to append lifecycle tx hash")?;
            }
        }

        self.state
            .record_event(
                record.correlation_id,
                Some(&record.status),
                to_status,
                event_kind(event),
                reason_override.or_else(|| event_reason(event)),
                Some(event_metadata(event)),
            )
            .await
            .context("failed to record lifecycle event")?;
        Ok(())
    }

    async fn run_miden_transfer<F>(&self, f: F) -> Result<String>
    where
        F: FnOnce(Arc<MidenClient>, DynStateStore) -> Result<String> + Send + 'static,
    {
        let miden = self.miden.clone();
        let store = self.state.clone();
        tokio::task::spawn_blocking(move || f(miden, store))
            .await
            .context("failed to join blocking Miden transfer task")?
    }

    async fn refund_record(&self, record: &QuoteRecord) -> Result<()> {
        let amount = refund_amount(self.state.as_ref(), record).await?;
        if is_evm_asset_id(&record.quote_request.origin_asset) {
            if record
                .quote_response
                .quote
                .deposit_address
                .as_deref()
                .is_some_and(|value| value.starts_with("mock-"))
            {
                return self
                    .state
                    .append_tx_hash(
                        record.correlation_id,
                        TxHashColumn::EvmRefundTxHashes,
                        &format!("mock-refund-{}", record.correlation_id),
                    )
                    .await
                    .context("failed to persist mock EVM refund tx hash");
            }
            let tx_hash = self
                .evm
                .refund(
                    record.correlation_id,
                    Address::from_str(&record.quote_request.refund_to).with_context(|| {
                        format!(
                            "invalid EVM refund address {}",
                            record.quote_request.refund_to
                        )
                    })?,
                    evm_asset_for_origin(self.evm.as_ref(), &record.quote_request.origin_asset)?,
                    U256::from_str(&amount)
                        .with_context(|| format!("invalid EVM refund amount {amount}"))?,
                )
                .await
                .context("failed to submit EVM refund")?;
            return self
                .state
                .append_tx_hash(
                    record.correlation_id,
                    TxHashColumn::EvmRefundTxHashes,
                    &format!("{tx_hash:#x}"),
                )
                .await
                .context("failed to persist EVM refund tx hash");
        }

        let correlation_id = record.correlation_id;
        let refund_to = record.quote_request.refund_to.clone();
        let origin_asset = record.quote_request.origin_asset.clone();
        let amount = amount.clone();
        self.run_miden_transfer(move |miden, store| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .context("failed to build tokio runtime for Miden refund")?;
            runtime.block_on(async move {
                let bootstrap = store
                    .get_miden_bootstrap()
                    .await
                    .context("failed to load Miden bootstrap state")?
                    .ok_or_else(|| anyhow!("Miden bootstrap state is missing"))?;
                let bootstrap = bootstrap_state_from_record(&bootstrap)?;
                refund_quote_to_origin(
                    miden.as_ref(),
                    store,
                    &bootstrap,
                    correlation_id,
                    &refund_to,
                    &origin_asset,
                    &amount,
                )
                .await
            })
        })
        .await
        .map(|_| ())
    }

    async fn deadline_expired_reason(
        &self,
        record: &QuoteRecord,
        to_status: &str,
    ) -> Result<Option<&'static str>> {
        if to_status == "REFUNDED"
            && !has_confirmed_deposit_amount(self.state.as_ref(), record.correlation_id).await?
        {
            return Ok(Some("deadline_expired_no_deposit"));
        }
        Ok(None)
    }

    async fn target_status_for_deadline_expired(
        &self,
        record: &QuoteRecord,
    ) -> Result<Option<&'static str>> {
        match record.status.as_str() {
            "PROCESSING" => Ok(None),
            "INCOMPLETE_DEPOSIT" => Ok(Some("INCOMPLETE_DEPOSIT")),
            "PENDING_DEPOSIT" | "KNOWN_DEPOSIT_TX" => Ok(Some("REFUNDED")),
            status if is_terminal_status(status) => Ok(None),
            _ => Ok(Some("REFUNDED")),
        }
    }

    async fn should_submit_refund_for_deadline_expiry(
        &self,
        record: &QuoteRecord,
        to_status: &str,
    ) -> Result<bool> {
        if to_status != "REFUNDED" {
            return Ok(false);
        }
        has_confirmed_deposit_amount(self.state.as_ref(), record.correlation_id).await
    }
}

#[async_trait]
impl Lifecycle for DefaultLifecycle {
    async fn apply(&self, event: LifecycleEvent) -> Result<()> {
        let correlation_id = correlation_id(&event);
        let idempotency_key = event_idempotency_key(&event);
        if !self
            .state
            .record_idempotency_key(correlation_id, &idempotency_key)
            .await
            .context("failed to record lifecycle idempotency key")?
        {
            return Ok(());
        }

        let record = self.load_quote(correlation_id).await?;
        let target_status = match &event {
            LifecycleEvent::DeadlineExpired { .. } => {
                self.target_status_for_deadline_expired(&record).await?
            }
            _ => target_status(&record, &event)?,
        };
        let Some(to_status) = target_status else {
            return Ok(());
        };
        if record.status != to_status && !is_valid_transition(&record.status, to_status) {
            return Err(anyhow!(
                "illegal lifecycle transition for quote {}: {} -> {}",
                correlation_id,
                record.status,
                to_status
            ));
        }

        let reason_override = match &event {
            LifecycleEvent::DeadlineExpired { .. } => {
                self.deadline_expired_reason(&record, to_status).await?
            }
            _ => None,
        };

        self.persist_transition(&record, &event, to_status, reason_override)
            .await?;

        match event {
            LifecycleEvent::SlippageExceeded { .. } | LifecycleEvent::IncompleteDeposit { .. } => {
                let refreshed = self.load_quote(correlation_id).await?;
                self.refund_record(&refreshed).await?;
            }
            LifecycleEvent::EvmDepositConfirmed { .. }
            | LifecycleEvent::MidenDepositConfirmed { .. }
                if to_status == "INCOMPLETE_DEPOSIT" =>
            {
                let refreshed = self.load_quote(correlation_id).await?;
                self.refund_record(&refreshed).await?;
            }
            LifecycleEvent::DeadlineExpired { .. }
                if self
                    .should_submit_refund_for_deadline_expiry(&record, to_status)
                    .await? =>
            {
                let refreshed = self.load_quote(correlation_id).await?;
                self.refund_record(&refreshed).await?;
            }
            _ => {}
        }

        Ok(())
    }

    async fn settle(&self, correlation_id: Uuid) -> Result<()> {
        let mut record = self.load_quote(correlation_id).await?;
        if is_terminal_status(&record.status) {
            return Ok(());
        }

        if record.status != "PROCESSING" {
            self.apply(LifecycleEvent::SettlementInitiated { correlation_id })
                .await?;
            record = self.load_quote(correlation_id).await?;
        }

        let requote = self
            .pricer
            .quote(
                asset_symbol(&record.quote_request.origin_asset)?,
                asset_symbol(&record.quote_request.destination_asset)?,
                &record.quote_response.quote.amount_in,
            )
            .await
            .context("failed to re-quote during settlement")?;
        if compare_amounts(
            &requote.output_amount,
            &record.quote_response.quote.min_amount_out,
        )?
        .is_lt()
        {
            return self
                .apply(LifecycleEvent::SlippageExceeded {
                    correlation_id,
                    expected_min: record.quote_response.quote.min_amount_out.clone(),
                    actual: requote.output_amount,
                })
                .await;
        }

        if is_evm_asset_id(&record.quote_request.origin_asset) {
            if let Some(existing) = record.miden_mint_tx_ids.last() {
                return self
                    .apply(LifecycleEvent::SettlementSucceeded {
                        correlation_id,
                        tx_hash: existing.clone(),
                    })
                    .await;
            }

            let destination_asset = record.quote_request.destination_asset.clone();
            let recipient = record.quote_request.recipient.clone();
            let amount_out = record.quote_response.quote.amount_out.clone();
            let tx_id = self
                .run_miden_transfer(move |miden, store| {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .context("failed to build tokio runtime for Miden mint")?;
                    runtime.block_on(async move {
                        let bootstrap = store
                            .get_miden_bootstrap()
                            .await
                            .context("failed to load Miden bootstrap state")?
                            .ok_or_else(|| anyhow!("Miden bootstrap state is missing"))?;
                        let bootstrap = bootstrap_state_from_record(&bootstrap)?;
                        let faucet_id = bootstrap.faucet_id_for_asset(&destination_asset)?;
                        let recipient = parse_account_id(&recipient)?;
                        mint_quote_to_user(
                            miden.as_ref(),
                            store,
                            &bootstrap,
                            correlation_id,
                            MidenTransfer {
                                faucet_id,
                                recipient_id: recipient,
                                amount: &amount_out,
                                tx_column: TxHashColumn::MidenMintTxIds,
                                idempotency_prefix: "miden_mint",
                            },
                        )
                        .await
                    })
                })
                .await?;

            return self
                .apply(LifecycleEvent::SettlementSucceeded {
                    correlation_id,
                    tx_hash: tx_id,
                })
                .await;
        }

        if let Some(existing) = record.evm_release_tx_hashes.last() {
            return self
                .apply(LifecycleEvent::SettlementSucceeded {
                    correlation_id,
                    tx_hash: existing.clone(),
                })
                .await;
        }

        let tx_hash = self
            .evm
            .release(
                correlation_id,
                Address::from_str(&record.quote_request.recipient).with_context(|| {
                    format!("invalid EVM recipient {}", record.quote_request.recipient)
                })?,
                evm_asset_for_origin(self.evm.as_ref(), &record.quote_request.destination_asset)?,
                U256::from_str(&record.quote_response.quote.amount_out).with_context(|| {
                    format!(
                        "invalid EVM release amount {}",
                        record.quote_response.quote.amount_out
                    )
                })?,
            )
            .await?;

        self.apply(LifecycleEvent::SettlementSucceeded {
            correlation_id,
            tx_hash: format!("{tx_hash:#x}"),
        })
        .await
    }

    async fn refund(&self, correlation_id: Uuid) -> Result<()> {
        let record = self.load_quote(correlation_id).await?;
        if record.status == "REFUNDED" {
            return Ok(());
        }
        if !is_valid_transition(&record.status, "REFUNDED") {
            return Err(anyhow!(
                "quote {} cannot transition from {} to REFUNDED",
                correlation_id,
                record.status
            ));
        }
        self.state
            .record_event(
                correlation_id,
                Some(&record.status),
                "REFUNDED",
                "REFUND_INITIATED",
                None,
                None,
            )
            .await
            .context("failed to record refund initiation")?;
        let refreshed = self.load_quote(correlation_id).await?;
        self.refund_record(&refreshed).await
    }
}

pub fn is_valid_transition(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("PENDING_DEPOSIT", "KNOWN_DEPOSIT_TX")
            | ("KNOWN_DEPOSIT_TX", "PENDING_DEPOSIT")
            | ("KNOWN_DEPOSIT_TX", "INCOMPLETE_DEPOSIT")
            | ("KNOWN_DEPOSIT_TX", "REFUNDED")
            | ("PENDING_DEPOSIT", "PROCESSING")
            | ("PENDING_DEPOSIT", "INCOMPLETE_DEPOSIT")
            | ("PENDING_DEPOSIT", "FAILED")
            | ("PENDING_DEPOSIT", "REFUNDED")
            | ("PROCESSING", "SUCCESS")
            | ("PROCESSING", "FAILED")
            | ("PROCESSING", "REFUNDED")
            | ("KNOWN_DEPOSIT_TX", "FAILED")
    )
}

pub fn check_flex_input_bounds(
    quote: &Quote,
    deposit_amount: &str,
    swap_type: &SwapType,
) -> Result<FlexInputDecision> {
    match swap_type {
        SwapType::FlexInput => {
            let upper = parse_amount_f64(&quote.amount_in)?;
            let minimum = parse_amount_f64(&quote.min_amount_in)?;
            let actual = parse_amount_f64(deposit_amount)?;
            let lower = minimum * 0.99;
            if actual < lower {
                Ok(FlexInputDecision::IncompleteDeposit)
            } else if actual > upper {
                Ok(FlexInputDecision::AcceptAboveUpper)
            } else {
                Ok(FlexInputDecision::Accept)
            }
        }
        SwapType::ExactInput => {
            let expected = parse_amount_f64(&quote.amount_in)?;
            let actual = parse_amount_f64(deposit_amount)?;
            if actual < expected {
                Ok(FlexInputDecision::IncompleteDeposit)
            } else if actual > expected {
                Ok(FlexInputDecision::AcceptAboveUpper)
            } else {
                Ok(FlexInputDecision::Accept)
            }
        }
        SwapType::ExactOutput => {
            let minimum = parse_amount_f64(&quote.min_amount_in)?;
            let actual = parse_amount_f64(deposit_amount)?;
            let maximum = match quote.max_amount_in.as_deref() {
                Some(value) => parse_amount_f64(value)?,
                None => minimum,
            };
            if actual < minimum {
                Ok(FlexInputDecision::IncompleteDeposit)
            } else if actual > maximum {
                Ok(FlexInputDecision::AcceptAboveUpper)
            } else {
                Ok(FlexInputDecision::Accept)
            }
        }
        SwapType::AnyInput => Ok(FlexInputDecision::Accept),
    }
}

pub async fn resume_in_flight_quotes(
    store: DynStateStore,
    lifecycle: DynLifecycle,
) -> Result<ResumeScan> {
    let mut scan = ResumeScan::default();

    for quote in store
        .list_evm_tracked_quotes()
        .await
        .context("failed to list in-flight EVM quotes")?
    {
        match quote.status.as_str() {
            "PENDING_DEPOSIT" | "KNOWN_DEPOSIT_TX" => {
                scan.evm_monitor_quotes.push(quote.correlation_id)
            }
            "PROCESSING" => {
                scan.processing_quotes.push(quote.correlation_id);
                lifecycle.settle(quote.correlation_id).await?;
            }
            _ => {}
        }
    }

    for quote in store
        .list_miden_tracked_quotes()
        .await
        .context("failed to list in-flight Miden quotes")?
    {
        match quote.status.as_str() {
            "PENDING_DEPOSIT" | "KNOWN_DEPOSIT_TX" => {
                scan.miden_monitor_quotes.push(quote.correlation_id);
            }
            "PROCESSING" if !scan.processing_quotes.contains(&quote.correlation_id) => {
                scan.processing_quotes.push(quote.correlation_id);
                lifecycle.settle(quote.correlation_id).await?;
            }
            _ => {}
        }
    }

    Ok(scan)
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ResumeScan {
    pub evm_monitor_quotes: Vec<Uuid>,
    pub miden_monitor_quotes: Vec<Uuid>,
    pub processing_quotes: Vec<Uuid>,
}

fn correlation_id(event: &LifecycleEvent) -> Uuid {
    match event {
        LifecycleEvent::EvmDepositDetected { correlation_id, .. }
        | LifecycleEvent::EvmDepositConfirmed { correlation_id, .. }
        | LifecycleEvent::MidenDepositDetected { correlation_id, .. }
        | LifecycleEvent::MidenDepositConfirmed { correlation_id, .. }
        | LifecycleEvent::SettlementInitiated { correlation_id }
        | LifecycleEvent::SettlementSucceeded { correlation_id, .. }
        | LifecycleEvent::SettlementFailed { correlation_id, .. }
        | LifecycleEvent::SlippageExceeded { correlation_id, .. }
        | LifecycleEvent::IncompleteDeposit { correlation_id, .. }
        | LifecycleEvent::DeadlineExpired { correlation_id } => *correlation_id,
    }
}

fn target_status(record: &QuoteRecord, event: &LifecycleEvent) -> Result<Option<&'static str>> {
    match event {
        LifecycleEvent::EvmDepositDetected { .. } | LifecycleEvent::MidenDepositDetected { .. } => {
            Ok(Some("KNOWN_DEPOSIT_TX"))
        }
        LifecycleEvent::EvmDepositConfirmed { amount, .. }
        | LifecycleEvent::MidenDepositConfirmed { amount, .. } => match check_flex_input_bounds(
            &record.quote_response.quote,
            amount,
            &record.quote_request.swap_type,
        )? {
            FlexInputDecision::IncompleteDeposit => Ok(Some("INCOMPLETE_DEPOSIT")),
            _ => Ok(Some("PENDING_DEPOSIT")),
        },
        LifecycleEvent::SettlementInitiated { .. } => Ok(Some("PROCESSING")),
        LifecycleEvent::SettlementSucceeded { .. } => Ok(Some("SUCCESS")),
        LifecycleEvent::SettlementFailed { .. } => Ok(Some("FAILED")),
        LifecycleEvent::DeadlineExpired { .. } => Ok(Some("REFUNDED")),
        LifecycleEvent::SlippageExceeded { .. } => Ok(Some("REFUNDED")),
        LifecycleEvent::IncompleteDeposit { .. } => Ok(Some("INCOMPLETE_DEPOSIT")),
    }
}

fn event_idempotency_key(event: &LifecycleEvent) -> String {
    match event {
        LifecycleEvent::EvmDepositDetected {
            correlation_id,
            tx_hash,
        } => format!("lifecycle:evm_deposit_detected:{correlation_id}:{tx_hash}"),
        LifecycleEvent::EvmDepositConfirmed {
            correlation_id,
            tx_hash,
            amount,
        } => format!("lifecycle:evm_deposit_confirmed:{correlation_id}:{tx_hash}:{amount}"),
        LifecycleEvent::MidenDepositDetected {
            correlation_id,
            note_id,
        } => format!("lifecycle:miden_deposit_detected:{correlation_id}:{note_id}"),
        LifecycleEvent::MidenDepositConfirmed {
            correlation_id,
            note_id,
            amount,
        } => format!("lifecycle:miden_deposit_confirmed:{correlation_id}:{note_id}:{amount}"),
        LifecycleEvent::SettlementInitiated { correlation_id } => {
            format!("lifecycle:settlement_initiated:{correlation_id}")
        }
        LifecycleEvent::SettlementSucceeded {
            correlation_id,
            tx_hash,
        } => format!("lifecycle:settlement_succeeded:{correlation_id}:{tx_hash}"),
        LifecycleEvent::SettlementFailed {
            correlation_id,
            reason,
        } => format!("lifecycle:settlement_failed:{correlation_id}:{reason}"),
        LifecycleEvent::SlippageExceeded {
            correlation_id,
            expected_min,
            actual,
        } => format!("lifecycle:slippage_exceeded:{correlation_id}:{expected_min}:{actual}"),
        LifecycleEvent::IncompleteDeposit {
            correlation_id,
            deposit_amount,
            min_required,
        } => {
            format!("lifecycle:incomplete_deposit:{correlation_id}:{deposit_amount}:{min_required}")
        }
        LifecycleEvent::DeadlineExpired { correlation_id } => {
            format!("lifecycle:deadline_expired:{correlation_id}")
        }
    }
}

fn event_kind(event: &LifecycleEvent) -> &'static str {
    match event {
        LifecycleEvent::EvmDepositDetected { .. } => "EVM_DEPOSIT_DETECTED",
        LifecycleEvent::EvmDepositConfirmed { .. } => "EVM_DEPOSIT_CONFIRMED",
        LifecycleEvent::MidenDepositDetected { .. } => "MIDEN_DEPOSIT_DETECTED",
        LifecycleEvent::MidenDepositConfirmed { .. } => "MIDEN_DEPOSIT_CONFIRMED",
        LifecycleEvent::SettlementInitiated { .. } => "SETTLEMENT_INITIATED",
        LifecycleEvent::SettlementSucceeded { .. } => "SETTLEMENT_SUCCEEDED",
        LifecycleEvent::SettlementFailed { .. } => "SETTLEMENT_FAILED",
        LifecycleEvent::SlippageExceeded { .. } => "SLIPPAGE_EXCEEDED",
        LifecycleEvent::IncompleteDeposit { .. } => "INCOMPLETE_DEPOSIT",
        LifecycleEvent::DeadlineExpired { .. } => "DEADLINE_EXPIRED",
    }
}

fn event_reason(event: &LifecycleEvent) -> Option<&str> {
    match event {
        LifecycleEvent::SettlementFailed { reason, .. } => Some(reason),
        LifecycleEvent::SlippageExceeded { .. } => Some("slippage exceeded"),
        LifecycleEvent::IncompleteDeposit { .. } => Some("deposit below minimum"),
        LifecycleEvent::DeadlineExpired { .. } => Some("quote deadline expired"),
        _ => None,
    }
}

fn event_metadata(event: &LifecycleEvent) -> Value {
    match event {
        LifecycleEvent::EvmDepositDetected { tx_hash, .. } => json!({ "txHash": tx_hash }),
        LifecycleEvent::EvmDepositConfirmed {
            tx_hash, amount, ..
        } => {
            json!({ "txHash": tx_hash, "amount": amount })
        }
        LifecycleEvent::MidenDepositDetected { note_id, .. } => json!({ "noteId": note_id }),
        LifecycleEvent::MidenDepositConfirmed {
            note_id, amount, ..
        } => json!({ "noteId": note_id, "amount": amount }),
        LifecycleEvent::SettlementInitiated { .. } => json!({}),
        LifecycleEvent::SettlementSucceeded { tx_hash, .. } => json!({ "txHash": tx_hash }),
        LifecycleEvent::SettlementFailed { reason, .. } => json!({ "reason": reason }),
        LifecycleEvent::SlippageExceeded {
            expected_min,
            actual,
            ..
        } => json!({ "expectedMin": expected_min, "actual": actual }),
        LifecycleEvent::IncompleteDeposit {
            deposit_amount,
            min_required,
            ..
        } => json!({ "depositAmount": deposit_amount, "minRequired": min_required }),
        LifecycleEvent::DeadlineExpired { .. } => json!({}),
    }
}

fn tx_hash_to_append<'a>(
    record: &'a QuoteRecord,
    event: &'a LifecycleEvent,
) -> Option<(TxHashColumn, &'a str)> {
    match event {
        LifecycleEvent::EvmDepositDetected { tx_hash, .. } => {
            Some((TxHashColumn::EvmDepositTxHashes, tx_hash))
        }
        LifecycleEvent::SettlementSucceeded { tx_hash, .. }
            if is_evm_asset_id(&record.quote_request.origin_asset) =>
        {
            Some((TxHashColumn::MidenMintTxIds, tx_hash))
        }
        LifecycleEvent::SettlementSucceeded { tx_hash, .. } => {
            Some((TxHashColumn::EvmReleaseTxHashes, tx_hash))
        }
        _ => None,
    }
}

fn is_terminal_status(status: &str) -> bool {
    matches!(
        status,
        "INCOMPLETE_DEPOSIT" | "SUCCESS" | "REFUNDED" | "FAILED"
    )
}

async fn refund_amount(
    store: &dyn crate::core::state::StateStore,
    record: &QuoteRecord,
) -> Result<String> {
    if let Some(amount) = last_confirmed_amount(store, record.correlation_id).await? {
        return Ok(amount);
    }
    Ok(record.quote_response.quote.amount_in.clone())
}

async fn has_confirmed_deposit_amount(
    store: &dyn crate::core::state::StateStore,
    correlation_id: Uuid,
) -> Result<bool> {
    Ok(last_confirmed_amount(store, correlation_id)
        .await?
        .is_some())
}

async fn last_confirmed_amount(
    store: &dyn crate::core::state::StateStore,
    correlation_id: Uuid,
) -> Result<Option<String>> {
    store
        .get_last_confirmed_deposit_amount(correlation_id)
        .await
        .context("failed to load last confirmed deposit amount")
}

fn evm_asset_for_origin(evm: &EvmClient, asset_id: &str) -> Result<EvmAsset> {
    match asset_id {
        "eth-anvil:eth" => Ok(EvmAsset::NativeEth),
        "eth-anvil:usdc" | "eth-anvil:usdt" | "eth-anvil:btc" => evm
            .token_address(asset_id)
            .map(EvmAsset::Erc20)
            .ok_or_else(|| anyhow!("missing token address for {asset_id}")),
        _ => Err(anyhow!("unsupported EVM asset {asset_id}")),
    }
}

fn parse_amount_f64(raw: &str) -> Result<f64> {
    let decimal =
        Decimal::from_str_exact(raw).with_context(|| format!("invalid decimal amount {raw}"))?;
    decimal
        .to_f64()
        .ok_or_else(|| anyhow!("amount {raw} cannot be represented as f64"))
}

fn compare_amounts(left: &str, right: &str) -> Result<std::cmp::Ordering> {
    let left = Decimal::from_str_exact(left).with_context(|| format!("invalid amount {left}"))?;
    let right =
        Decimal::from_str_exact(right).with_context(|| format!("invalid amount {right}"))?;
    Ok(left.cmp(&right))
}
