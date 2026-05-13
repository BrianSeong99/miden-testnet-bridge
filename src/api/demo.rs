use std::{env, str::FromStr, time::Duration};

use alloy::{
    network::TransactionBuilder as _,
    primitives::{Address, U256},
    providers::{Provider, ProviderBuilder},
    rpc::types::eth::TransactionRequest,
    signers::local::PrivateKeySigner,
};
use axum::{
    Json,
    extract::{Path, State, rejection::JsonRejection},
};
use miden_client::{
    asset::FungibleAsset, auth::AuthSecretKey, keystore::Keystore,
    transaction::TransactionRequestBuilder,
};
use rand::SeedableRng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use time::format_description::well_known::Rfc3339;
use tokio::{
    runtime::Builder as RuntimeBuilder,
    task,
    time::{Instant, sleep},
};
use uuid::Uuid;

use crate::{
    AppState,
    api::{errors::ApiError, quote::create_quote},
    chains::{
        miden::{MidenClient, parse_account_id},
        miden_bootstrap::{bootstrap_state_from_record, sync_with_retry, wait_for_tx},
        miden_bridge_note::{BridgeOutDepositMemo, build_bridge_out_note},
        miden_deposit_account::build_wallet_account,
    },
    core::state::{LifecycleEventRecord, QuoteRecord},
    now_iso8601,
    types::{
        DepositMode, DepositType, QuoteRequest, QuoteResponse, RecipientType, RefundType, SwapType,
    },
};

const DEFAULT_ANVIL_FUNDED_PRIVATE_KEY: &str =
    "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
const DEFAULT_DEMO_EVM_RECIPIENT: &str = "0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc";
const DEFAULT_DEMO_AMOUNT: &str = "1000000000000";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoInfoResponse {
    pub demo_enabled: bool,
    pub ui_enabled: bool,
    pub runtime_profile: String,
    pub near_intents_mock: bool,
    pub primary_api_flow: Vec<&'static str>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoStartRequest {
    pub asset: Option<String>,
    pub amount: Option<String>,
    pub recipient: Option<String>,
    pub refund_to: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoClaimRequest {
    pub account_id: String,
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoOutboundSubmitRequest {
    pub sender_account_id: String,
    pub recipient: Option<String>,
    pub refund_to: Option<String>,
    pub asset: Option<String>,
    pub amount: Option<String>,
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoWallet {
    pub account_id: String,
    pub address: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoTxResponse {
    pub tx_hash: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoInboundStartResponse {
    pub wallet: DemoWallet,
    pub quote: QuoteResponse,
    pub evm_deposit_tx_hash: String,
    pub flow: DemoFlowResponse,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoOutboundFundResponse {
    pub wallet: DemoWallet,
    pub funding_quote: QuoteResponse,
    pub evm_deposit_tx_hash: String,
    pub flow: DemoFlowResponse,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoOutboundSubmitResponse {
    pub quote: QuoteResponse,
    pub consumed_funding_note_tx_id: Option<String>,
    pub bridge_out_note_tx_id: String,
    pub flow: DemoFlowResponse,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoClaimResponse {
    pub account_id: String,
    pub consumed_note_tx_id: Option<String>,
    pub consumed_note_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoFlowSummary {
    pub correlation_id: String,
    pub direction: String,
    pub status: String,
    pub origin_asset: String,
    pub destination_asset: String,
    pub amount: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoFlowResponse {
    pub correlation_id: String,
    pub direction: String,
    pub status: String,
    pub updated_at: String,
    pub quote_response: QuoteResponse,
    pub lifecycle: Vec<DemoLifecycleEvent>,
    pub artifacts: DemoArtifacts,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoLifecycleEvent {
    pub id: i64,
    pub from_status: Option<String>,
    pub to_status: String,
    pub event_kind: String,
    pub reason: Option<String>,
    pub metadata: Option<Value>,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoArtifacts {
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

pub async fn info(State(state): State<AppState>) -> Json<DemoInfoResponse> {
    Json(DemoInfoResponse {
        demo_enabled: state.demo_enabled,
        ui_enabled: state.ui_enabled,
        runtime_profile: state.runtime_profile,
        near_intents_mock: true,
        primary_api_flow: vec![
            "GET /v0/tokens",
            "POST /v0/quote",
            "user sends origin-chain deposit",
            "POST /v0/deposit/submit",
            "GET /v0/status",
        ],
    })
}

pub async fn flows(State(state): State<AppState>) -> Result<Json<Vec<DemoFlowSummary>>, ApiError> {
    ensure_demo_enabled(&state)?;
    let records = state
        .store
        .list_recent_quotes(25)
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?;
    Ok(Json(records.into_iter().map(flow_summary).collect()))
}

pub async fn flow(
    State(state): State<AppState>,
    Path(correlation_id): Path<Uuid>,
) -> Result<Json<DemoFlowResponse>, ApiError> {
    ensure_demo_enabled(&state)?;
    Ok(Json(flow_response(&state, correlation_id).await?))
}

pub async fn start_inbound(
    State(state): State<AppState>,
    request: Result<Json<DemoStartRequest>, JsonRejection>,
) -> Result<Json<DemoInboundStartResponse>, ApiError> {
    ensure_demo_enabled(&state)?;
    let Json(request) = request.map_err(ApiError::from_json_rejection)?;
    let amount = request
        .amount
        .unwrap_or_else(|| DEFAULT_DEMO_AMOUNT.to_owned());
    let asset = request.asset.unwrap_or_else(|| "eth".to_owned());
    let miden = miden_client(&state)?;
    let wallet = create_demo_wallet(miden.clone(), "inbound-recipient").await?;
    let recipient = request.recipient.unwrap_or_else(|| wallet.address.clone());
    let refund_to = request
        .refund_to
        .unwrap_or_else(|| DEFAULT_DEMO_EVM_RECIPIENT.to_owned());
    let quote = create_quote(
        &state,
        quote_request(
            &format!("eth-anvil:{asset}"),
            &format!("miden-testnet:{asset}"),
            &amount,
            &recipient,
            &refund_to,
        ),
    )
    .await?;
    let deposit_address = quote
        .quote
        .deposit_address
        .as_deref()
        .ok_or_else(|| ApiError::internal("quote did not include deposit address"))?;
    let tx_hash = send_native_deposit(deposit_address, &amount).await?;
    let correlation_id = parse_correlation_id(&quote)?;
    let flow = flow_response(&state, correlation_id).await?;

    Ok(Json(DemoInboundStartResponse {
        wallet,
        quote,
        evm_deposit_tx_hash: tx_hash,
        flow,
    }))
}

pub async fn claim_inbound(
    State(state): State<AppState>,
    request: Result<Json<DemoClaimRequest>, JsonRejection>,
) -> Result<Json<DemoClaimResponse>, ApiError> {
    ensure_demo_enabled(&state)?;
    let Json(request) = request.map_err(ApiError::from_json_rejection)?;
    let account_id = parse_account_id(&request.account_id)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let timeout = Duration::from_secs(request.timeout_secs.unwrap_or(120));
    let (count, tx_id) = wait_and_consume_notes(miden_client(&state)?, account_id, timeout)
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?;
    Ok(Json(DemoClaimResponse {
        account_id: account_id.to_hex(),
        consumed_note_tx_id: tx_id,
        consumed_note_count: count,
    }))
}

pub async fn fund_outbound(
    State(state): State<AppState>,
    request: Result<Json<DemoStartRequest>, JsonRejection>,
) -> Result<Json<DemoOutboundFundResponse>, ApiError> {
    ensure_demo_enabled(&state)?;
    let Json(request) = request.map_err(ApiError::from_json_rejection)?;
    let amount = request
        .amount
        .unwrap_or_else(|| DEFAULT_DEMO_AMOUNT.to_owned());
    let asset = request.asset.unwrap_or_else(|| "eth".to_owned());
    let miden = miden_client(&state)?;
    let wallet = create_demo_wallet(miden, "outbound-sender").await?;
    let recipient = wallet.address.clone();
    let refund_to = request
        .refund_to
        .unwrap_or_else(|| DEFAULT_DEMO_EVM_RECIPIENT.to_owned());
    let quote = create_quote(
        &state,
        quote_request(
            &format!("eth-anvil:{asset}"),
            &format!("miden-testnet:{asset}"),
            &amount,
            &recipient,
            &refund_to,
        ),
    )
    .await?;
    let deposit_address = quote
        .quote
        .deposit_address
        .as_deref()
        .ok_or_else(|| ApiError::internal("quote did not include deposit address"))?;
    let tx_hash = send_native_deposit(deposit_address, &amount).await?;
    let correlation_id = parse_correlation_id(&quote)?;
    let flow = flow_response(&state, correlation_id).await?;

    Ok(Json(DemoOutboundFundResponse {
        wallet,
        funding_quote: quote,
        evm_deposit_tx_hash: tx_hash,
        flow,
    }))
}

pub async fn submit_outbound(
    State(state): State<AppState>,
    request: Result<Json<DemoOutboundSubmitRequest>, JsonRejection>,
) -> Result<Json<DemoOutboundSubmitResponse>, ApiError> {
    ensure_demo_enabled(&state)?;
    let Json(request) = request.map_err(ApiError::from_json_rejection)?;
    let amount = request
        .amount
        .unwrap_or_else(|| DEFAULT_DEMO_AMOUNT.to_owned());
    let amount_u64 = amount
        .parse::<u64>()
        .map_err(|error| ApiError::bad_request(format!("invalid amount: {error}")))?;
    let asset = request.asset.unwrap_or_else(|| "eth".to_owned());
    let miden = miden_client(&state)?;
    let sender_id = parse_account_id(&request.sender_account_id)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let timeout = Duration::from_secs(request.timeout_secs.unwrap_or(180));
    let (_, consumed_tx_id) = wait_and_consume_notes(miden.clone(), sender_id, timeout)
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?;
    let refund_to = request
        .refund_to
        .unwrap_or_else(|| miden.encode_basic_wallet_address(sender_id));
    let recipient = request
        .recipient
        .unwrap_or_else(|| DEFAULT_DEMO_EVM_RECIPIENT.to_owned());
    let quote = create_quote(
        &state,
        quote_request(
            &format!("miden-testnet:{asset}"),
            &format!("eth-anvil:{asset}"),
            &amount,
            &recipient,
            &refund_to,
        ),
    )
    .await?;
    let deposit_address = quote
        .quote
        .deposit_address
        .as_deref()
        .ok_or_else(|| ApiError::internal("quote did not include deposit address"))?;
    let deposit_memo = quote
        .quote
        .deposit_memo
        .as_deref()
        .ok_or_else(|| ApiError::internal("quote did not include bridge-note deposit memo"))?;
    let bridge_tx_id =
        submit_bridge_out_note(&state, sender_id, deposit_address, deposit_memo, amount_u64)
            .await
            .map_err(|error| ApiError::internal(error.to_string()))?;
    let correlation_id = parse_correlation_id(&quote)?;
    let flow = flow_response(&state, correlation_id).await?;

    Ok(Json(DemoOutboundSubmitResponse {
        quote,
        consumed_funding_note_tx_id: consumed_tx_id,
        bridge_out_note_tx_id: bridge_tx_id,
        flow,
    }))
}

fn ensure_demo_enabled(state: &AppState) -> Result<(), ApiError> {
    if !state.demo_enabled {
        Err(ApiError::bad_request(
            "demo endpoints are disabled; set BRIDGE_DEMO_ENABLED=1",
        ))
    } else if state.runtime_profile != "anvil" {
        Err(ApiError::bad_request(
            "demo endpoints only support BRIDGE_PROFILE=anvil",
        ))
    } else {
        Ok(())
    }
}

fn miden_client(state: &AppState) -> Result<MidenClient, ApiError> {
    state
        .miden_client
        .as_ref()
        .map(|client| client.as_ref().clone())
        .ok_or_else(|| ApiError::internal("Miden client is not configured"))
}

fn quote_request(
    origin_asset: &str,
    destination_asset: &str,
    amount: &str,
    recipient: &str,
    refund_to: &str,
) -> QuoteRequest {
    QuoteRequest {
        dry: false,
        deposit_mode: Some(DepositMode::Simple),
        swap_type: SwapType::ExactInput,
        slippage_tolerance: 100.0,
        origin_asset: origin_asset.to_owned(),
        deposit_type: DepositType::OriginChain,
        destination_asset: destination_asset.to_owned(),
        amount: amount.to_owned(),
        refund_to: refund_to.to_owned(),
        refund_type: RefundType::OriginChain,
        recipient: recipient.to_owned(),
        connected_wallets: None,
        session_id: None,
        virtual_chain_recipient: None,
        virtual_chain_refund_recipient: None,
        custom_recipient_msg: None,
        recipient_type: RecipientType::DestinationChain,
        deadline: "2027-01-01T00:00:00Z".to_owned(),
        referral: Some("miden-testnet-bridge-lab".to_owned()),
        quote_waiting_time_ms: None,
        app_fees: None,
    }
}

fn parse_correlation_id(quote: &QuoteResponse) -> Result<Uuid, ApiError> {
    Uuid::parse_str(&quote.correlation_id)
        .map_err(|error| ApiError::internal(format!("invalid correlation id: {error}")))
}

async fn send_native_deposit(to: &str, amount: &str) -> Result<String, ApiError> {
    let private_key = env::var("DEMO_EVM_FUNDED_PRIVATE_KEY")
        .unwrap_or_else(|_| DEFAULT_ANVIL_FUNDED_PRIVATE_KEY.to_owned());
    let rpc_url = env::var("EVM_RPC_URL").unwrap_or_else(|_| "http://anvil:8545".to_owned());
    let amount = amount
        .parse::<u128>()
        .map_err(|error| ApiError::bad_request(format!("invalid amount: {error}")))?;
    let signer: PrivateKeySigner = private_key.parse().map_err(|error| {
        ApiError::internal(format!("invalid DEMO_EVM_FUNDED_PRIVATE_KEY: {error}"))
    })?;
    let provider = ProviderBuilder::new().wallet(signer).connect_http(
        rpc_url
            .parse()
            .map_err(|error| ApiError::internal(format!("invalid EVM_RPC_URL: {error}")))?,
    );
    let pending = provider
        .send_transaction(
            TransactionRequest::default()
                .with_to(
                    Address::from_str(to)
                        .map_err(|error| ApiError::bad_request(error.to_string()))?,
                )
                .with_value(U256::from(amount)),
        )
        .await
        .map_err(|error| ApiError::internal(format!("failed to send demo deposit: {error}")))?;
    let tx_hash = format!("{:#x}", pending.tx_hash());
    pending
        .watch()
        .await
        .map_err(|error| ApiError::internal(format!("failed waiting for demo deposit: {error}")))?;
    Ok(tx_hash)
}

async fn create_demo_wallet(client: MidenClient, label: &str) -> Result<DemoWallet, ApiError> {
    let label = label.to_owned();
    task::spawn_blocking(move || {
        let runtime = RuntimeBuilder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| ApiError::internal(error.to_string()))?;
        runtime.block_on(async move {
            let unique = Uuid::new_v4();
            let init_seed = seed32(&format!("{label}:{unique}:init"));
            let auth_seed = seed32(&format!("{label}:{unique}:auth"));
            let mut rng = rand::rngs::StdRng::from_seed(auth_seed);
            let secret_key = AuthSecretKey::new_falcon512_poseidon2_with_rng(&mut rng);
            let account = build_wallet_account(init_seed, &secret_key)
                .map_err(|error| ApiError::internal(error.to_string()))?;
            let keystore = client
                .open_keystore()
                .map_err(|error| ApiError::internal(error.to_string()))?;
            let mut inner = client
                .open()
                .await
                .map_err(|error| ApiError::internal(error.to_string()))?;
            keystore
                .add_key(&secret_key, account.id())
                .await
                .map_err(|error| ApiError::internal(error.to_string()))?;
            inner
                .add_account(&account, false)
                .await
                .map_err(|error| ApiError::internal(error.to_string()))?;
            Ok(DemoWallet {
                account_id: account.id().to_hex(),
                address: client.encode_basic_wallet_address(account.id()),
            })
        })
    })
    .await
    .map_err(|error| ApiError::internal(error.to_string()))?
}

fn seed32(label: &str) -> [u8; 32] {
    Sha256::digest(label.as_bytes()).into()
}

async fn wait_and_consume_notes(
    client: MidenClient,
    account_id: miden_client::account::AccountId,
    timeout: Duration,
) -> anyhow::Result<(usize, Option<String>)> {
    task::spawn_blocking(move || {
        let runtime = RuntimeBuilder::new_current_thread()
            .enable_all()
            .build()
            .map_err(anyhow::Error::from)?;
        runtime.block_on(async move {
            let deadline = Instant::now() + timeout;
            loop {
                let mut inner = client.open().await?;
                sync_with_retry(&mut inner).await?;
                let notes: Vec<_> = inner
                    .get_consumable_notes(Some(account_id))
                    .await?
                    .into_iter()
                    .filter_map(|(record, _)| record.try_into().ok())
                    .collect();
                if notes.is_empty() {
                    if Instant::now() >= deadline {
                        anyhow::bail!("timed out waiting for consumable Miden notes");
                    }
                    sleep(Duration::from_secs(2)).await;
                    continue;
                }
                if let Err(error) = inner.import_account_by_id(account_id).await {
                    tracing::warn!(%account_id, ?error, "demo account import before consume failed; continuing");
                }
                let count = notes.len();
                let request = TransactionRequestBuilder::new().build_consume_notes(notes)?;
                let tx_id = inner.submit_new_transaction(account_id, request).await?;
                wait_for_tx(&mut inner, tx_id).await?;
                return Ok((count, Some(tx_id.to_string())));
            }
        })
    })
    .await?
}

async fn submit_bridge_out_note(
    state: &AppState,
    sender_id: miden_client::account::AccountId,
    deposit_address: &str,
    deposit_memo: &str,
    amount: u64,
) -> anyhow::Result<String> {
    let miden = miden_client(state).map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let record = state
        .store
        .get_miden_bootstrap()
        .await?
        .ok_or_else(|| anyhow::anyhow!("miden bootstrap record is missing"))?;
    let bootstrap = bootstrap_state_from_record(&record)?;
    let memo = BridgeOutDepositMemo::from_deposit_memo(deposit_memo)?;
    anyhow::ensure!(
        parse_account_id(&memo.bridge_account_id)? == parse_account_id(deposit_address)?,
        "deposit address and bridge-note memo target differ"
    );
    task::spawn_blocking(move || {
        let runtime = RuntimeBuilder::new_current_thread()
            .enable_all()
            .build()
            .map_err(anyhow::Error::from)?;
        runtime.block_on(async move {
            let mut inner = miden.open().await?;
            sync_with_retry(&mut inner).await?;
            let note = build_bridge_out_note(
                sender_id,
                vec![FungibleAsset::new(bootstrap.eth_faucet_account_id, amount)?.into()],
                &memo,
                inner.rng(),
            )?;
            let request = TransactionRequestBuilder::new()
                .own_output_notes(vec![note])
                .build()?;
            let tx_id = inner.submit_new_transaction(sender_id, request).await?;
            wait_for_tx(&mut inner, tx_id).await?;
            Ok(tx_id.to_string())
        })
    })
    .await?
}

async fn flow_response(
    state: &AppState,
    correlation_id: Uuid,
) -> Result<DemoFlowResponse, ApiError> {
    let record = state
        .store
        .get_quote_by_correlation_id(correlation_id)
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?
        .ok_or_else(|| ApiError::not_found("flow not found"))?;
    let lifecycle = state
        .store
        .list_lifecycle_events(correlation_id)
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?;
    Ok(flow_detail(record, lifecycle))
}

fn flow_summary(record: QuoteRecord) -> DemoFlowSummary {
    DemoFlowSummary {
        correlation_id: record.correlation_id.to_string(),
        direction: direction(&record),
        status: record.status,
        origin_asset: record.quote_request.origin_asset,
        destination_asset: record.quote_request.destination_asset,
        amount: record.quote_response.quote.amount_in,
        updated_at: format_time(record.updated_at),
    }
}

fn flow_detail(record: QuoteRecord, lifecycle: Vec<LifecycleEventRecord>) -> DemoFlowResponse {
    DemoFlowResponse {
        correlation_id: record.correlation_id.to_string(),
        direction: direction(&record),
        status: record.status,
        updated_at: format_time(record.updated_at),
        quote_response: record.quote_response,
        lifecycle: lifecycle.into_iter().map(map_lifecycle).collect(),
        artifacts: DemoArtifacts {
            evm_deposit_tx_hashes: record.evm_deposit_tx_hashes,
            evm_release_tx_hashes: record.evm_release_tx_hashes,
            miden_mint_tx_ids: record.miden_mint_tx_ids,
            miden_consume_tx_ids: record.miden_consume_tx_ids,
            evm_refund_tx_hashes: record.evm_refund_tx_hashes,
            miden_refund_tx_ids: record.miden_refund_tx_ids,
            intent_hashes: record.intent_hashes,
            near_tx_hashes: record.near_tx_hashes,
            idempotency_keys: record.idempotency_keys,
        },
    }
}

fn direction(record: &QuoteRecord) -> String {
    if record
        .quote_request
        .origin_asset
        .starts_with("miden-testnet:")
    {
        "miden-to-evm".to_owned()
    } else {
        "evm-to-miden".to_owned()
    }
}

fn map_lifecycle(event: LifecycleEventRecord) -> DemoLifecycleEvent {
    DemoLifecycleEvent {
        id: event.id,
        from_status: event.from_status,
        to_status: event.to_status,
        event_kind: event.event_kind,
        reason: event.reason,
        metadata: event.metadata,
        created_at: format_time(event.created_at),
    }
}

fn format_time(value: time::OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_else(|_| now_iso8601())
}
