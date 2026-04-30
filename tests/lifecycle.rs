use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use miden_testnet_bridge::{
    AppState, app,
    chains::{
        evm::{EvmClient, EvmConfig},
        miden::MidenClient,
    },
    core::{
        lifecycle::{
            DefaultLifecycle, FlexInputDecision, Lifecycle, LifecycleEvent,
            check_flex_input_bounds, is_valid_transition, resume_in_flight_quotes,
        },
        pricer::{MockPricer, PriceQuote, Pricer, PricerError},
        state::{DynStateStore, TxHashColumn},
    },
    test_support::memory_state,
    types::{
        DepositMode, DepositType, Quote, QuoteRequest, QuoteResponse, RecipientType, RefundType,
        StatusResponse, SwapType,
    },
};
use tempfile::tempdir;
use tower::ServiceExt;
use uuid::Uuid;

const DEFAULT_SOLVER_PRIVATE_KEY: &str =
    "0x59c6995e998f97a5a0044966f0945382dbb7d2745078b2336b91c60d50d6b6d7";

#[derive(Clone)]
struct StaticPricer {
    output_amount: String,
}

#[async_trait]
impl Pricer for StaticPricer {
    async fn quote(
        &self,
        _in_asset_symbol: &str,
        _out_asset_symbol: &str,
        amount: &str,
    ) -> Result<PriceQuote, PricerError> {
        Ok(PriceQuote {
            input_usd: "1.0".to_owned(),
            output_usd: "1.0".to_owned(),
            output_amount: if self.output_amount.is_empty() {
                amount.to_owned()
            } else {
                self.output_amount.clone()
            },
        })
    }
}

struct ResumeOnlyLifecycle;

#[async_trait]
impl Lifecycle for ResumeOnlyLifecycle {
    async fn apply(&self, _event: LifecycleEvent) -> Result<()> {
        Ok(())
    }

    async fn settle(&self, _correlation_id: Uuid) -> Result<()> {
        Ok(())
    }

    async fn refund(&self, _correlation_id: Uuid) -> Result<()> {
        Ok(())
    }
}

#[test]
fn flex_input_bounds_cover_spec_ranges() {
    let quote = sample_quote_response(Uuid::new_v4(), SwapType::FlexInput).quote;
    let cases = [
        ("1100000", FlexInputDecision::AcceptAboveUpper),
        ("997500", FlexInputDecision::Accept),
        ("985050", FlexInputDecision::Accept),
        ("984000", FlexInputDecision::IncompleteDeposit),
    ];

    for (deposit_amount, expected) in cases {
        let actual = check_flex_input_bounds(&quote, deposit_amount, &SwapType::FlexInput).unwrap();
        assert_eq!(actual, expected, "deposit_amount={deposit_amount}");
    }
}

#[test]
fn valid_transitions_cover_legal_and_illegal_edges() {
    let legal = [
        ("PENDING_DEPOSIT", "KNOWN_DEPOSIT_TX"),
        ("KNOWN_DEPOSIT_TX", "PENDING_DEPOSIT"),
        ("PENDING_DEPOSIT", "PROCESSING"),
        ("PENDING_DEPOSIT", "INCOMPLETE_DEPOSIT"),
        ("PENDING_DEPOSIT", "FAILED"),
        ("PROCESSING", "SUCCESS"),
        ("PROCESSING", "FAILED"),
        ("PROCESSING", "REFUNDED"),
    ];
    for (from, to) in legal {
        assert!(is_valid_transition(from, to), "{from} -> {to}");
    }

    let illegal = [
        ("PENDING_DEPOSIT", "SUCCESS"),
        ("KNOWN_DEPOSIT_TX", "SUCCESS"),
        ("SUCCESS", "PROCESSING"),
        ("REFUNDED", "FAILED"),
    ];
    for (from, to) in illegal {
        assert!(!is_valid_transition(from, to), "{from} -> {to}");
    }
}

#[tokio::test]
async fn lifecycle_apply_happy_path_reaches_success() {
    let store = memory_state();
    let lifecycle = build_lifecycle(store.clone(), Arc::new(MockPricer)).await;
    let quote = insert_quote(store.clone(), SwapType::ExactInput).await;
    let correlation_id = Uuid::parse_str(&quote.correlation_id).unwrap();

    lifecycle
        .apply(LifecycleEvent::EvmDepositDetected {
            correlation_id,
            tx_hash: "0xabc".to_owned(),
        })
        .await
        .unwrap();
    lifecycle
        .apply(LifecycleEvent::EvmDepositConfirmed {
            correlation_id,
            tx_hash: "0xabc".to_owned(),
            amount: "1000000".to_owned(),
        })
        .await
        .unwrap();
    lifecycle
        .apply(LifecycleEvent::SettlementInitiated { correlation_id })
        .await
        .unwrap();
    lifecycle
        .apply(LifecycleEvent::SettlementSucceeded {
            correlation_id,
            tx_hash: "miden-tx-1".to_owned(),
        })
        .await
        .unwrap();

    let record = store
        .get_quote_by_correlation_id(correlation_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.status, "SUCCESS");
    assert_eq!(record.evm_deposit_tx_hashes, vec!["0xabc".to_owned()]);
    assert_eq!(record.miden_mint_tx_ids, vec!["miden-tx-1".to_owned()]);
}

#[tokio::test]
async fn incomplete_deposit_path_persists_refund_hash() {
    let store = memory_state();
    let lifecycle = build_lifecycle(store.clone(), Arc::new(MockPricer)).await;
    let quote = insert_quote(store.clone(), SwapType::FlexInput).await;
    let correlation_id = Uuid::parse_str(&quote.correlation_id).unwrap();

    lifecycle
        .apply(LifecycleEvent::EvmDepositDetected {
            correlation_id,
            tx_hash: "0xdef".to_owned(),
        })
        .await
        .unwrap();
    lifecycle
        .apply(LifecycleEvent::EvmDepositConfirmed {
            correlation_id,
            tx_hash: "0xdef".to_owned(),
            amount: "984000".to_owned(),
        })
        .await
        .unwrap();

    let record = store
        .get_quote_by_correlation_id(correlation_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.status, "INCOMPLETE_DEPOSIT");
    assert_eq!(
        record.evm_refund_tx_hashes,
        vec![format!("mock-refund-{correlation_id}")]
    );
}

#[tokio::test]
async fn slippage_exceeded_transitions_to_refunded() {
    let store = memory_state();
    let lifecycle = build_lifecycle(
        store.clone(),
        Arc::new(StaticPricer {
            output_amount: "900000".to_owned(),
        }),
    )
    .await;
    let quote = insert_quote(store.clone(), SwapType::ExactInput).await;
    let correlation_id = Uuid::parse_str(&quote.correlation_id).unwrap();

    lifecycle
        .apply(LifecycleEvent::SettlementInitiated { correlation_id })
        .await
        .unwrap();
    lifecycle.settle(correlation_id).await.unwrap();

    let record = store
        .get_quote_by_correlation_id(correlation_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.status, "REFUNDED");
    assert_eq!(
        record.evm_refund_tx_hashes,
        vec![format!("mock-refund-{correlation_id}")]
    );
}

#[tokio::test]
async fn settlement_failed_transitions_to_failed() {
    let store = memory_state();
    let lifecycle = build_lifecycle(store.clone(), Arc::new(MockPricer)).await;
    let quote = insert_quote(store.clone(), SwapType::ExactInput).await;
    let correlation_id = Uuid::parse_str(&quote.correlation_id).unwrap();

    lifecycle
        .apply(LifecycleEvent::SettlementInitiated { correlation_id })
        .await
        .unwrap();
    lifecycle
        .apply(LifecycleEvent::SettlementFailed {
            correlation_id,
            reason: "release reverted".to_owned(),
        })
        .await
        .unwrap();

    let record = store
        .get_quote_by_correlation_id(correlation_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.status, "FAILED");
}

#[tokio::test]
async fn replaying_same_event_is_a_no_op() {
    let store = memory_state();
    let lifecycle = build_lifecycle(store.clone(), Arc::new(MockPricer)).await;
    let quote = insert_quote(store.clone(), SwapType::ExactInput).await;
    let correlation_id = Uuid::parse_str(&quote.correlation_id).unwrap();

    let event = LifecycleEvent::EvmDepositDetected {
        correlation_id,
        tx_hash: "0xaaa".to_owned(),
    };
    lifecycle.apply(event.clone()).await.unwrap();
    lifecycle.apply(event).await.unwrap();

    let record = store
        .get_quote_by_correlation_id(correlation_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.evm_deposit_tx_hashes, vec!["0xaaa".to_owned()]);
}

#[tokio::test]
async fn terminal_state_survives_past_deadline_status_poll() {
    let store = memory_state();
    let lifecycle = build_lifecycle(store.clone(), Arc::new(MockPricer)).await;
    let quote =
        insert_quote_with_deadline(store.clone(), SwapType::ExactInput, "2026-01-01T00:00:00Z")
            .await;
    let correlation_id = Uuid::parse_str(&quote.correlation_id).unwrap();
    let deposit_address = quote.quote.deposit_address.clone().unwrap();

    lifecycle
        .apply(LifecycleEvent::SettlementInitiated { correlation_id })
        .await
        .unwrap();
    lifecycle
        .apply(LifecycleEvent::SettlementFailed {
            correlation_id,
            reason: "failed after deadline".to_owned(),
        })
        .await
        .unwrap();
    store
        .append_tx_hash(correlation_id, TxHashColumn::EvmDepositTxHashes, "0xlate")
        .await
        .unwrap();

    let app = app(AppState::new(store));
    let response = app
        .oneshot(
            Request::get(format!("/v0/status?depositAddress={deposit_address}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let status: StatusResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        status.status,
        miden_testnet_bridge::types::SwapStatus::Failed
    );
    assert_eq!(status.swap_details.origin_chain_tx_hashes.len(), 1);
}

#[tokio::test]
async fn resume_scan_finds_non_terminal_quotes() {
    let store = memory_state();
    let quote = insert_quote(store.clone(), SwapType::ExactInput).await;
    let correlation_id = Uuid::parse_str(&quote.correlation_id).unwrap();
    store
        .record_event(
            correlation_id,
            Some("PENDING_DEPOSIT"),
            "PROCESSING",
            "SETTLEMENT_INITIATED",
            None,
            None,
        )
        .await
        .unwrap();

    let scan = resume_in_flight_quotes(store, Arc::new(ResumeOnlyLifecycle))
        .await
        .unwrap();
    assert_eq!(scan.processing_quotes, vec![correlation_id]);
}

async fn build_lifecycle(store: DynStateStore, pricer: Arc<dyn Pricer>) -> Arc<DefaultLifecycle> {
    let temp_dir = tempdir().unwrap();
    let token_file = temp_dir.path().join("token-addresses.json");
    let evm = Arc::new(
        EvmClient::new(
            store.clone(),
            EvmConfig {
                rpc_url: "http://127.0.0.1:8545".to_owned(),
                master_mnemonic: "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about".to_owned(),
                solver_private_key: DEFAULT_SOLVER_PRIVATE_KEY.to_owned(),
                token_addresses_path: token_file,
                chain_id: 271828,
            },
        )
        .unwrap(),
    );
    let miden = Arc::new(
        MidenClient::new("http://localhost:57291", temp_dir.path())
            .await
            .unwrap(),
    );
    Arc::new(DefaultLifecycle::new(store, pricer, evm, miden))
}

async fn insert_quote(store: DynStateStore, swap_type: SwapType) -> QuoteResponse {
    insert_quote_with_deadline(store, swap_type, "2027-06-12T00:00:00Z").await
}

async fn insert_quote_with_deadline(
    store: DynStateStore,
    swap_type: SwapType,
    deadline: &str,
) -> QuoteResponse {
    let correlation_id = Uuid::new_v4();
    let request = sample_quote_request(swap_type.clone(), deadline);
    let response = sample_quote_response(correlation_id, swap_type);
    store.insert_quote(&response, &request).await.unwrap();
    response
}

fn sample_quote_request(swap_type: SwapType, deadline: &str) -> QuoteRequest {
    QuoteRequest {
        dry: false,
        deposit_mode: Some(DepositMode::Simple),
        swap_type,
        slippage_tolerance: 0.5,
        origin_asset: "eth-anvil:eth".to_owned(),
        deposit_type: DepositType::OriginChain,
        destination_asset: "miden-local:eth".to_owned(),
        amount: "1000000".to_owned(),
        refund_to: "0xfeed".to_owned(),
        refund_type: RefundType::OriginChain,
        recipient: "recipient".to_owned(),
        connected_wallets: None,
        session_id: None,
        virtual_chain_recipient: None,
        virtual_chain_refund_recipient: None,
        custom_recipient_msg: None,
        recipient_type: RecipientType::DestinationChain,
        deadline: deadline.to_owned(),
        referral: None,
        quote_waiting_time_ms: None,
        app_fees: None,
    }
}

fn sample_quote_response(correlation_id: Uuid, swap_type: SwapType) -> QuoteResponse {
    QuoteResponse {
        correlation_id: correlation_id.to_string(),
        timestamp: "2026-04-30T00:00:00Z".to_owned(),
        signature: String::new(),
        quote_request: sample_quote_request(swap_type.clone(), "2027-06-12T00:00:00Z"),
        quote: Quote {
            deposit_address: Some(format!("mock-{correlation_id}")),
            deposit_memo: None,
            amount_in: "1000000".to_owned(),
            amount_in_formatted: "1.0".to_owned(),
            amount_in_usd: "1.0".to_owned(),
            min_amount_in: "995000".to_owned(),
            max_amount_in: Some("1010000".to_owned()),
            amount_out: "1000000".to_owned(),
            amount_out_formatted: "1.0".to_owned(),
            amount_out_usd: "1.0".to_owned(),
            min_amount_out: "995000".to_owned(),
            deadline: Some("2027-06-12T00:00:00Z".to_owned()),
            time_when_inactive: Some("2027-06-12T00:00:00Z".to_owned()),
            time_estimate: 120.0,
            virtual_chain_recipient: None,
            virtual_chain_refund_recipient: None,
            custom_recipient_msg: None,
            refund_fee: None,
        },
    }
}
