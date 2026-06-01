use std::{env, str::FromStr};

use alloy::{
    primitives::{Address, B256, Bytes, U256},
    sol,
    sol_types::SolCall,
};
use anyhow::{Context, Result, anyhow, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::chains::miden::parse_account_id;

pub const DEFAULT_BRIDGE_ADDRESS: &str = "0x1348947e282138d8f377b467f7d9c2eb0f335d1f";
pub const DEFAULT_DEST_NETWORK: u32 = 76;
pub const DEFAULT_DEST_L1_NETWORK: u32 = 0;
pub const DEFAULT_L2_CHAIN_ID: u64 = 1_022_211_914;
pub const DEFAULT_MIDEN_BRIDGE_ID: &str = "mcst1arychvrurzxdy5qwz0mg5p5umsvsepyx";
pub const DEFAULT_MIDEN_FAUCET_ID: &str = "mcst1arnrhfau9svl7cpu2tr8lfzzd5j87wwe";
pub const DEFAULT_BRIDGE_SERVICE_API: &str =
    "https://miden-testnet-bridge.dev.eu-north-3.gateway.fm/api";
pub const DEFAULT_MIDEN_NODE_URL: &str = "https://rpc.testnet.miden.io:443";
pub const DEFAULT_SEPOLIA_RPC_URL: &str = "https://ethereum-sepolia-rpc.publicnode.com";
pub const DEFAULT_GAS_TOKEN_ADDRESS: &str = "0x0000000000000000000000000000000000000000";
pub const DEFAULT_GAS_LIMIT: u64 = 300_000;
pub const DEFAULT_AMOUNT_ETH: &str = "0.001";
pub const DEFAULT_MIDEN_WITHDRAW_AMOUNT: &str = "10000";
pub const VERSION_PIN: &str = "deployed Bali AggLayer: miden-client v0.14.4 / Poseidon2";
pub const SOURCE_NOTE: &str = "0xMiden/miden-client#2173 Bali bridge docs, rechecked 2026-05-27";
pub const L2_WITHDRAW_REFERENCE_SCRIPT: &str =
    "0xMiden/miden-client#2173 docs/.../scripts/bali-l2-withdraw.sh";
pub const L2_WITHDRAW_REQUIRED_CONFIG_KEYS: &[&str] = &[
    "MIDEN_STORE_DIR",
    "MIDEN_NODE_URL",
    "MIDEN_ACCOUNT_ID",
    "MIDEN_BRIDGE_ID",
    "MIDEN_FAUCET_ID",
    "MIDEN_WITHDRAW_AMOUNT",
    "ETH_ACCOUNT_ID",
    "DEST_L1_NETWORK",
];
pub const CLAIM_ASSET_SIGNATURE: &str = "claimAsset(bytes32[32],bytes32[32],uint256,bytes32,bytes32,uint32,address,uint32,address,uint256,bytes)";

sol! {
    function bridgeAsset(uint32 destinationNetwork, address destinationAddress, uint256 amount, address token, bool forceUpdateGlobalExitRoot, bytes permitData);
    function claimAsset(bytes32[32] smtProofLocalExitRoot, bytes32[32] smtProofRollupExitRoot, uint256 globalIndex, bytes32 mainnetExitRoot, bytes32 rollupExitRoot, uint32 originNetwork, address originTokenAddress, uint32 destinationNetwork, address destinationAddress, uint256 amount, bytes metadata);
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgglayerConfig {
    pub bridge_address: String,
    pub dest_network: u32,
    pub dest_l1_network: u32,
    pub l2_chain_id: u64,
    pub miden_bridge_id: String,
    pub miden_bridge_hex: String,
    pub miden_faucet_id: String,
    pub miden_faucet_hex: String,
    pub bridge_service_api: String,
    pub miden_node_url: String,
    pub sepolia_rpc_url: String,
    pub gas_token_address: String,
    pub gas_limit: u64,
    pub force_update_global_exit_root: bool,
    pub version_pin: &'static str,
    pub source_note: &'static str,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct AgglayerL1DepositPlanRequest {
    pub miden_account_id: String,
    pub amount_eth: Option<String>,
    pub eth_keystore: Option<String>,
    pub sepolia_rpc_url: Option<String>,
    pub gas_limit: Option<u64>,
    pub force_update_global_exit_root: Option<bool>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgglayerL1DepositPlan {
    pub dry_run: bool,
    pub direction: &'static str,
    pub bridge_address: String,
    pub destination_network: u32,
    pub destination_miden_account_id: String,
    pub destination_miden_account_hex: String,
    pub destination_bridge_address: String,
    pub amount_eth: String,
    pub amount_wei: String,
    pub gas_token_address: String,
    pub force_update_global_exit_root: bool,
    pub gas_limit: u64,
    pub calldata: String,
    pub status_url: String,
    pub command: Vec<String>,
    pub shell_command: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct AgglayerL2WithdrawPlanRequest {
    pub miden_account_id: String,
    pub eth_account_id: String,
    pub miden_store_dir: Option<String>,
    pub miden_withdraw_amount: Option<String>,
    pub bridge_out_tool: Option<String>,
    pub miden_node_url: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgglayerL2WithdrawPlan {
    pub dry_run: bool,
    pub direction: &'static str,
    pub miden_account_id: String,
    pub miden_account_id_hex: String,
    pub eth_account_id: String,
    pub bridge_id: String,
    pub bridge_id_hex: String,
    pub faucet_id: String,
    pub faucet_id_hex: String,
    pub destination_network: u32,
    pub amount: String,
    pub command: Vec<String>,
    pub shell_command: String,
    pub status_url: String,
    pub claims_url: String,
    pub merkle_proof_url_template: String,
    pub claim_command_template: String,
    pub reference_script: &'static str,
    pub required_config_keys: Vec<&'static str>,
    pub readiness_checks: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct AgglayerL2ClaimPlanRequest {
    pub eth_account_id: String,
    pub deposit_count: Option<u64>,
    pub eth_keystore: Option<String>,
    pub sepolia_rpc_url: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgglayerL2ClaimPlan {
    pub ready_for_claim: bool,
    pub direction: &'static str,
    pub bridge_address: String,
    pub eth_account_id: String,
    pub status_url: String,
    pub claims_url: String,
    pub merkle_proof_url: Option<String>,
    pub deposit: Option<AgglayerBridgeDeposit>,
    pub calldata: Option<String>,
    pub transaction: Option<AgglayerEvmTransaction>,
    pub command: Vec<String>,
    pub shell_command: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgglayerEvmTransaction {
    pub to: String,
    pub data: String,
    pub value: String,
    pub gas: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct AgglayerBridgeDeposit {
    pub leaf_type: Option<u64>,
    pub orig_net: u32,
    pub orig_addr: String,
    pub amount: String,
    pub dest_net: u32,
    pub dest_addr: String,
    pub block_num: String,
    pub deposit_cnt: u64,
    pub network_id: u32,
    pub tx_hash: String,
    #[serde(default)]
    pub claim_tx_hash: String,
    #[serde(default)]
    pub metadata: String,
    pub ready_for_claim: bool,
    pub global_index: String,
}

#[derive(Debug, Deserialize)]
struct BridgeDepositsResponse {
    deposits: Vec<AgglayerBridgeDeposit>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
struct BridgeProofData {
    main_exit_root: String,
    rollup_exit_root: String,
    merkle_proof: Vec<String>,
    rollup_merkle_proof: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BridgeProofResponse {
    proof: BridgeProofData,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgglayerInfo {
    pub mode: &'static str,
    pub constants: AgglayerConfig,
    pub l1_to_miden_flow: Vec<&'static str>,
    pub miden_to_l1_flow: Vec<&'static str>,
    pub warnings: Vec<&'static str>,
}

impl AgglayerConfig {
    pub fn from_env() -> Result<Self> {
        let bridge_address = env_or("AGGLAYER_BRIDGE_ADDRESS", DEFAULT_BRIDGE_ADDRESS);
        let gas_token_address = env_or("AGGLAYER_GAS_TOKEN_ADDRESS", DEFAULT_GAS_TOKEN_ADDRESS);
        Address::from_str(&bridge_address).context("AGGLAYER_BRIDGE_ADDRESS must be an address")?;
        Address::from_str(&gas_token_address)
            .context("AGGLAYER_GAS_TOKEN_ADDRESS must be an address")?;

        let miden_bridge_id = env_or("AGGLAYER_MIDEN_BRIDGE_ID", DEFAULT_MIDEN_BRIDGE_ID);
        let miden_bridge_hex = normalize_miden_account_hex(&miden_bridge_id)
            .context("AGGLAYER_MIDEN_BRIDGE_ID must be a Miden account id")?;
        let miden_faucet_id = env_or("AGGLAYER_MIDEN_FAUCET_ID", DEFAULT_MIDEN_FAUCET_ID);
        let miden_faucet_hex = normalize_miden_account_hex(&miden_faucet_id)
            .context("AGGLAYER_MIDEN_FAUCET_ID must be a Miden account id")?;

        Ok(Self {
            bridge_address,
            dest_network: env_parse("AGGLAYER_DEST_NETWORK", DEFAULT_DEST_NETWORK)?,
            dest_l1_network: env_parse("AGGLAYER_DEST_L1_NETWORK", DEFAULT_DEST_L1_NETWORK)?,
            l2_chain_id: env_parse("AGGLAYER_L2_CHAIN_ID", DEFAULT_L2_CHAIN_ID)?,
            miden_bridge_id,
            miden_bridge_hex,
            miden_faucet_id,
            miden_faucet_hex,
            bridge_service_api: env_or("AGGLAYER_BRIDGE_SERVICE_API", DEFAULT_BRIDGE_SERVICE_API),
            miden_node_url: env_or("AGGLAYER_MIDEN_NODE_URL", DEFAULT_MIDEN_NODE_URL),
            sepolia_rpc_url: env_or("AGGLAYER_SEPOLIA_RPC_URL", DEFAULT_SEPOLIA_RPC_URL),
            gas_token_address,
            gas_limit: env_parse("AGGLAYER_GAS_LIMIT", DEFAULT_GAS_LIMIT)?,
            force_update_global_exit_root: env_bool(
                "AGGLAYER_FORCE_UPDATE_GLOBAL_EXIT_ROOT",
                true,
            )?,
            version_pin: VERSION_PIN,
            source_note: SOURCE_NOTE,
        })
    }
}

pub fn agglayer_info() -> Result<AgglayerInfo> {
    Ok(AgglayerInfo {
        mode: "agglayer-testnet-helper",
        constants: AgglayerConfig::from_env()?,
        l1_to_miden_flow: vec![
            "Create or sync a Miden testnet account.",
            "Generate bridgeAsset calldata for Sepolia.",
            "Broadcast with cast only after reviewing the dry-run command.",
            "Poll the Gateway FM bridge service until ready_for_claim=true.",
            "Run miden-client sync and consume-notes.",
        ],
        miden_to_l1_flow: vec![
            "Build gateway-fm/miden-agglayer bridge-out-tool.",
            "Use the 0xMiden/miden-client#2173 bali-l2-withdraw.sh helper header as the withdraw config reference.",
            "Submit a B2AGG note from the Miden wallet.",
            "Poll /bridges/{sepolia-address} until the Miden-origin row has ready_for_claim=true.",
            "Fetch /merkle-proof with deposit_cnt and network_id, then call claimAsset on Sepolia.",
        ],
        warnings: vec![
            "Testnet only. Never use mainnet funds or production keys.",
            "The public tutorial is still an open PR; recheck AGGLAYER_* constants before a funded run.",
            "gateway-fm/miden-agglayer scripts/e2e-l2-to-l1.sh still has local hardcoded values; use it only for claimAsset calldata shape.",
            "This service returns dry-run plans; Miden-to-Sepolia claimAsset remains an explicit operator action.",
        ],
    })
}

pub fn build_l1_deposit_plan(
    config: AgglayerConfig,
    request: AgglayerL1DepositPlanRequest,
) -> Result<AgglayerL1DepositPlan> {
    let amount_eth = request
        .amount_eth
        .unwrap_or_else(|| DEFAULT_AMOUNT_ETH.to_owned());
    let amount_wei = parse_eth_to_wei(&amount_eth)?;
    ensure!(
        amount_wei > U256::ZERO,
        "amountEth must be greater than zero"
    );

    let destination_miden_account_hex = normalize_miden_account_hex(&request.miden_account_id)?;
    let destination_bridge_address =
        miden_account_to_bridge_destination(&request.miden_account_id)?;
    let gas_limit = request.gas_limit.unwrap_or(config.gas_limit);
    let force_update_global_exit_root = request
        .force_update_global_exit_root
        .unwrap_or(config.force_update_global_exit_root);
    let sepolia_rpc_url = request
        .sepolia_rpc_url
        .unwrap_or_else(|| config.sepolia_rpc_url.clone());
    let eth_keystore = request
        .eth_keystore
        .unwrap_or_else(|| "./miden-bali-sepolia".to_owned());
    let calldata = bridge_asset_calldata(
        &config,
        &destination_bridge_address,
        amount_wei,
        force_update_global_exit_root,
    )?;
    let status_url = format!(
        "{}/bridges/{}?limit=1&offset=0",
        config.bridge_service_api.trim_end_matches('/'),
        destination_bridge_address
    );
    let command = vec![
        "cast".to_owned(),
        "send".to_owned(),
        config.bridge_address.clone(),
        calldata.clone(),
        "--value".to_owned(),
        amount_wei.to_string(),
        "--keystore".to_owned(),
        eth_keystore,
        "--rpc-url".to_owned(),
        sepolia_rpc_url,
        "--gas-limit".to_owned(),
        gas_limit.to_string(),
    ];

    Ok(AgglayerL1DepositPlan {
        dry_run: true,
        direction: "sepolia-to-miden",
        bridge_address: config.bridge_address,
        destination_network: config.dest_network,
        destination_miden_account_id: request.miden_account_id,
        destination_miden_account_hex,
        destination_bridge_address,
        amount_eth,
        amount_wei: amount_wei.to_string(),
        gas_token_address: config.gas_token_address,
        force_update_global_exit_root,
        gas_limit,
        calldata,
        status_url,
        shell_command: shell_join(&command),
        command,
        warnings: vec![
            "Dry-run only: this service does not broadcast Sepolia transactions.".to_owned(),
            "Funded runs require a Sepolia keystore, gas, and a final constants check.".to_owned(),
        ],
    })
}

pub fn build_l2_withdraw_plan(
    config: AgglayerConfig,
    request: AgglayerL2WithdrawPlanRequest,
) -> Result<AgglayerL2WithdrawPlan> {
    Address::from_str(&request.eth_account_id).context("ethAccountId must be a Sepolia address")?;
    let miden_account_id_hex = normalize_miden_account_hex(&request.miden_account_id)?;
    let amount = normalize_positive_integer(
        &request
            .miden_withdraw_amount
            .unwrap_or_else(|| DEFAULT_MIDEN_WITHDRAW_AMOUNT.to_owned()),
        "midenWithdrawAmount",
    )?;
    let bridge_out_tool = request
        .bridge_out_tool
        .unwrap_or_else(|| "bridge-out-tool".to_owned());
    let miden_store_dir = request
        .miden_store_dir
        .unwrap_or_else(|| "~/.miden".to_owned());
    let miden_node_url = request
        .miden_node_url
        .unwrap_or_else(|| config.miden_node_url.clone());
    let command = vec![
        bridge_out_tool,
        "--store-dir".to_owned(),
        miden_store_dir,
        "--node-url".to_owned(),
        miden_node_url,
        "--wallet-id".to_owned(),
        request.miden_account_id.clone(),
        "--bridge-id".to_owned(),
        config.miden_bridge_id.clone(),
        "--faucet-id".to_owned(),
        config.miden_faucet_id.clone(),
        "--amount".to_owned(),
        amount.clone(),
        "--dest-address".to_owned(),
        request.eth_account_id.clone(),
        "--dest-network".to_owned(),
        config.dest_l1_network.to_string(),
    ];
    let status_url = bridge_status_url(&config, &request.eth_account_id);
    let claims_url = claims_url(&config, &request.eth_account_id);
    let merkle_proof_url_template = merkle_proof_url_template(&config);
    let claim_command_template = l2_claim_command_template(&config);

    Ok(AgglayerL2WithdrawPlan {
        dry_run: true,
        direction: "miden-to-sepolia",
        miden_account_id: request.miden_account_id,
        miden_account_id_hex,
        eth_account_id: request.eth_account_id,
        bridge_id: config.miden_bridge_id,
        bridge_id_hex: config.miden_bridge_hex,
        faucet_id: config.miden_faucet_id,
        faucet_id_hex: config.miden_faucet_hex,
        destination_network: config.dest_l1_network,
        amount,
        shell_command: shell_join(&command),
        command,
        status_url,
        claims_url,
        merkle_proof_url_template,
        claim_command_template,
        reference_script: L2_WITHDRAW_REFERENCE_SCRIPT,
        required_config_keys: L2_WITHDRAW_REQUIRED_CONFIG_KEYS.to_vec(),
        readiness_checks: vec![
            format!("ready_for_claim == true"),
            format!("network_id == {}", config.dest_network),
            format!("dest_net == {}", config.dest_l1_network),
            "claim_tx_hash is empty".to_owned(),
        ],
        warnings: vec![
            "Dry-run only: this service does not submit B2AGG notes or claim assets.".to_owned(),
            "Poll the bridges URL for readiness; the claims URL is post-claim history and stays empty until claimAsset lands.".to_owned(),
            "Use the 0xMiden/miden-client#2173 bali-l2-withdraw.sh header as the withdraw env/config reference.".to_owned(),
            "gateway-fm/miden-agglayer scripts/e2e-l2-to-l1.sh contains local hardcoded values; do not copy its IDs or RPC URLs."
                .to_owned(),
        ],
    })
}

pub async fn build_l2_claim_plan(
    config: AgglayerConfig,
    request: AgglayerL2ClaimPlanRequest,
) -> Result<AgglayerL2ClaimPlan> {
    Address::from_str(&request.eth_account_id).context("ethAccountId must be a Sepolia address")?;
    let status_url = bridge_status_url(&config, &request.eth_account_id);
    let claims = claims_url(&config, &request.eth_account_id);
    let deposits = fetch_bridge_deposits(&status_url).await?;

    let Some(deposit) = select_ready_l2_deposit(
        &deposits,
        config.dest_network,
        config.dest_l1_network,
        request.deposit_count,
    )
    .cloned() else {
        let missing = request
            .deposit_count
            .map(|deposit_count| {
                format!("No ready unclaimed Miden-origin bridge row was found for deposit count {deposit_count}.")
            })
            .unwrap_or_else(|| "No ready unclaimed Miden-origin bridge row was found yet.".to_owned());
        return Ok(AgglayerL2ClaimPlan {
            ready_for_claim: false,
            direction: "miden-to-sepolia",
            bridge_address: config.bridge_address,
            eth_account_id: request.eth_account_id,
            status_url,
            claims_url: claims,
            merkle_proof_url: None,
            deposit: None,
            calldata: None,
            transaction: None,
            command: Vec::new(),
            shell_command: None,
            warnings: vec![
                missing,
                "Keep polling the bridges URL; polling the claims URL before claimAsset will return empty history.".to_owned(),
            ],
        });
    };

    let merkle_proof_url = merkle_proof_url(&config, deposit.deposit_cnt, deposit.network_id);
    let proof = fetch_bridge_proof(&merkle_proof_url).await?;
    let sepolia_rpc_url = request
        .sepolia_rpc_url
        .unwrap_or_else(|| config.sepolia_rpc_url.clone());
    let eth_keystore = request
        .eth_keystore
        .unwrap_or_else(|| "<sepolia-claimer-keystore>".to_owned());
    let command = l2_claim_command(&config, &deposit, &proof, &sepolia_rpc_url, &eth_keystore);
    let calldata = claim_asset_calldata(&deposit, &proof)?;
    let transaction = l2_claim_transaction(&config, calldata.clone());

    Ok(AgglayerL2ClaimPlan {
        ready_for_claim: true,
        direction: "miden-to-sepolia",
        bridge_address: config.bridge_address,
        eth_account_id: request.eth_account_id,
        status_url,
        claims_url: claims,
        merkle_proof_url: Some(merkle_proof_url),
        deposit: Some(deposit),
        calldata: Some(calldata),
        transaction: Some(transaction),
        shell_command: Some(shell_join(&command)),
        command,
        warnings: vec![
            "Review the ready bridge row before broadcasting claimAsset.".to_owned(),
            "The claimer pays Sepolia gas; use testnet keys only.".to_owned(),
        ],
    })
}

pub fn miden_account_to_bridge_destination(value: &str) -> Result<String> {
    let account_hex = normalize_miden_account_hex(value)?;
    let raw = account_hex
        .strip_prefix("0x")
        .ok_or_else(|| anyhow!("normalized Miden account hex is missing 0x prefix"))?;
    Ok(format!("0x00000000{}00", raw.to_ascii_lowercase()))
}

pub fn normalize_miden_account_hex(value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("Miden account id must not be empty");
    }
    if let Some(raw) = trimmed.strip_prefix("0x") {
        return normalize_30_hex(raw);
    }
    if trimmed.len() == 30 && trimmed.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return normalize_30_hex(trimmed);
    }

    parse_account_id(trimmed)
        .map(|account_id| account_id.to_hex())
        .with_context(|| format!("failed to parse Miden account id {trimmed}"))
}

fn normalize_30_hex(raw: &str) -> Result<String> {
    ensure!(
        raw.len() == 30 && raw.chars().all(|ch| ch.is_ascii_hexdigit()),
        "Miden account hex must be 30 hex characters"
    );
    Ok(format!("0x{}", raw.to_ascii_lowercase()))
}

fn bridge_asset_calldata(
    config: &AgglayerConfig,
    destination_bridge_address: &str,
    amount_wei: U256,
    force_update_global_exit_root: bool,
) -> Result<String> {
    let destination = Address::from_str(destination_bridge_address)
        .context("destination bridge address must be an EVM address")?;
    let gas_token =
        Address::from_str(&config.gas_token_address).context("gas token must be an EVM address")?;
    let call = bridgeAssetCall::new((
        config.dest_network,
        destination,
        amount_wei,
        gas_token,
        force_update_global_exit_root,
        Bytes::new(),
    ));
    Ok(format!("0x{}", alloy::hex::encode(call.abi_encode())))
}

async fn fetch_bridge_deposits(status_url: &str) -> Result<Vec<AgglayerBridgeDeposit>> {
    let response = reqwest::get(status_url)
        .await
        .with_context(|| format!("failed to fetch AggLayer bridge status from {status_url}"))?;
    ensure!(
        response.status().is_success(),
        "AggLayer bridge status returned {}",
        response.status()
    );
    let body = response
        .json::<BridgeDepositsResponse>()
        .await
        .context("failed to parse AggLayer bridge status response")?;
    Ok(body.deposits)
}

async fn fetch_bridge_proof(merkle_proof_url: &str) -> Result<BridgeProofData> {
    let response = reqwest::get(merkle_proof_url).await.with_context(|| {
        format!("failed to fetch AggLayer merkle proof from {merkle_proof_url}")
    })?;
    ensure!(
        response.status().is_success(),
        "AggLayer merkle proof returned {}",
        response.status()
    );
    let body = response
        .json::<BridgeProofResponse>()
        .await
        .context("failed to parse AggLayer merkle proof response")?;
    Ok(body.proof)
}

fn select_ready_l2_deposit(
    deposits: &[AgglayerBridgeDeposit],
    miden_network_id: u32,
    l1_network_id: u32,
    deposit_count: Option<u64>,
) -> Option<&AgglayerBridgeDeposit> {
    deposits.iter().find(|deposit| {
        deposit.ready_for_claim
            && deposit.network_id == miden_network_id
            && deposit.dest_net == l1_network_id
            && deposit.claim_tx_hash.trim().is_empty()
            && deposit_count.is_none_or(|expected| deposit.deposit_cnt == expected)
    })
}

fn l2_claim_command(
    config: &AgglayerConfig,
    deposit: &AgglayerBridgeDeposit,
    proof: &BridgeProofData,
    sepolia_rpc_url: &str,
    eth_keystore: &str,
) -> Vec<String> {
    vec![
        "cast".to_owned(),
        "send".to_owned(),
        config.bridge_address.clone(),
        CLAIM_ASSET_SIGNATURE.to_owned(),
        fixed_bytes32_array(&proof.merkle_proof),
        fixed_bytes32_array(&proof.rollup_merkle_proof),
        deposit.global_index.clone(),
        proof.main_exit_root.clone(),
        proof.rollup_exit_root.clone(),
        deposit.orig_net.to_string(),
        deposit.orig_addr.clone(),
        deposit.dest_net.to_string(),
        deposit.dest_addr.clone(),
        deposit.amount.clone(),
        normalized_metadata(&deposit.metadata),
        "--keystore".to_owned(),
        eth_keystore.to_owned(),
        "--rpc-url".to_owned(),
        sepolia_rpc_url.to_owned(),
    ]
}

fn claim_asset_calldata(
    deposit: &AgglayerBridgeDeposit,
    proof: &BridgeProofData,
) -> Result<String> {
    let origin_token_address = Address::from_str(&deposit.orig_addr)
        .context("origin token address must be an EVM address")?;
    let destination_address = Address::from_str(&deposit.dest_addr)
        .context("destination address must be an EVM address")?;
    let global_index =
        U256::from_str(&deposit.global_index).context("global index must be a uint256")?;
    let amount = U256::from_str(&deposit.amount).context("amount must be a uint256")?;
    let main_exit_root =
        B256::from_str(&proof.main_exit_root).context("main exit root must be bytes32")?;
    let rollup_exit_root =
        B256::from_str(&proof.rollup_exit_root).context("rollup exit root must be bytes32")?;
    let metadata = parse_hex_bytes(&normalized_metadata(&deposit.metadata))?;

    let call = claimAssetCall::new((
        fixed_b256_array(&proof.merkle_proof)?,
        fixed_b256_array(&proof.rollup_merkle_proof)?,
        global_index,
        main_exit_root,
        rollup_exit_root,
        deposit.orig_net,
        origin_token_address,
        deposit.dest_net,
        destination_address,
        amount,
        metadata,
    ));
    Ok(format!("0x{}", alloy::hex::encode(call.abi_encode())))
}

fn l2_claim_transaction(config: &AgglayerConfig, calldata: String) -> AgglayerEvmTransaction {
    AgglayerEvmTransaction {
        to: config.bridge_address.clone(),
        data: calldata,
        value: "0x0".to_owned(),
        gas: format!("0x{:x}", config.gas_limit),
    }
}

fn l2_claim_command_template(config: &AgglayerConfig) -> String {
    let command = vec![
        "cast".to_owned(),
        "send".to_owned(),
        config.bridge_address.clone(),
        CLAIM_ASSET_SIGNATURE.to_owned(),
        "<local-merkle-proof[32]>".to_owned(),
        "<rollup-merkle-proof[32]>".to_owned(),
        "<global-index>".to_owned(),
        "<main-exit-root>".to_owned(),
        "<rollup-exit-root>".to_owned(),
        "<orig-net>".to_owned(),
        "<orig-token-address>".to_owned(),
        "<dest-net>".to_owned(),
        "<dest-address>".to_owned(),
        "<amount>".to_owned(),
        "<metadata-or-0x>".to_owned(),
        "--keystore".to_owned(),
        "<sepolia-claimer-keystore>".to_owned(),
        "--rpc-url".to_owned(),
        config.sepolia_rpc_url.clone(),
    ];
    shell_join(&command)
}

fn fixed_bytes32_array(values: &[String]) -> String {
    let zero = format!("0x{}", "00".repeat(32));
    let mut padded = values.to_vec();
    while padded.len() < 32 {
        padded.push(zero.clone());
    }
    format!(
        "[{}]",
        padded.into_iter().take(32).collect::<Vec<_>>().join(",")
    )
}

fn fixed_b256_array(values: &[String]) -> Result<[B256; 32]> {
    let mut padded = [B256::ZERO; 32];
    for (index, value) in values.iter().take(32).enumerate() {
        padded[index] = B256::from_str(value).context("proof element must be bytes32")?;
    }
    Ok(padded)
}

fn normalized_metadata(metadata: &str) -> String {
    let trimmed = metadata.trim();
    if trimmed.is_empty() || trimmed == "0x" {
        "0x".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn parse_hex_bytes(value: &str) -> Result<Bytes> {
    let raw = value
        .strip_prefix("0x")
        .ok_or_else(|| anyhow!("metadata must be 0x-prefixed hex"))?;
    ensure!(
        raw.len() % 2 == 0 && raw.chars().all(|ch| ch.is_ascii_hexdigit()),
        "metadata must be valid hex"
    );
    Ok(Bytes::from(alloy::hex::decode(raw)?))
}

fn bridge_status_url(config: &AgglayerConfig, eth_account_id: &str) -> String {
    format!(
        "{}/bridges/{}?limit=20&offset=0",
        config.bridge_service_api.trim_end_matches('/'),
        eth_account_id
    )
}

fn claims_url(config: &AgglayerConfig, eth_account_id: &str) -> String {
    format!(
        "{}/claims/{}?limit=20&offset=0",
        config.bridge_service_api.trim_end_matches('/'),
        eth_account_id
    )
}

fn merkle_proof_url_template(config: &AgglayerConfig) -> String {
    format!(
        "{}/merkle-proof?deposit_cnt=<deposit_cnt>&net_id=<network_id>",
        config.bridge_service_api.trim_end_matches('/')
    )
}

fn merkle_proof_url(config: &AgglayerConfig, deposit_cnt: u64, network_id: u32) -> String {
    format!(
        "{}/merkle-proof?deposit_cnt={deposit_cnt}&net_id={network_id}",
        config.bridge_service_api.trim_end_matches('/')
    )
}

fn parse_eth_to_wei(value: &str) -> Result<U256> {
    let value = value.trim();
    ensure!(!value.is_empty(), "amountEth must not be empty");
    ensure!(
        value.chars().all(|ch| ch.is_ascii_digit() || ch == '.'),
        "amountEth must be a decimal ETH amount"
    );
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    ensure!(
        parts.next().is_none(),
        "amountEth must contain at most one decimal point"
    );
    let whole = if whole.is_empty() { "0" } else { whole };
    ensure!(
        whole.chars().all(|ch| ch.is_ascii_digit()),
        "amountEth whole part must be numeric"
    );
    let whole_wei = U256::from_str(whole)
        .context("amountEth whole part is too large")?
        .checked_mul(U256::from(1_000_000_000_000_000_000u128))
        .ok_or_else(|| anyhow!("amountEth is too large"))?;
    let fraction_wei = match fraction {
        Some(raw) => {
            ensure!(raw.len() <= 18, "amountEth supports at most 18 decimals");
            ensure!(
                raw.chars().all(|ch| ch.is_ascii_digit()),
                "amountEth fractional part must be numeric"
            );
            let padded = format!("{raw:0<18}");
            U256::from_str(&padded).context("amountEth fractional part is too large")?
        }
        None => U256::ZERO,
    };
    whole_wei
        .checked_add(fraction_wei)
        .ok_or_else(|| anyhow!("amountEth is too large"))
}

fn normalize_positive_integer(value: &str, field_name: &str) -> Result<String> {
    let trimmed = value.trim();
    ensure!(
        !trimmed.is_empty() && trimmed.chars().all(|ch| ch.is_ascii_digit()),
        "{field_name} must be an integer amount"
    );
    let normalized = trimmed.trim_start_matches('0');
    ensure!(
        !normalized.is_empty(),
        "{field_name} must be greater than zero"
    );
    Ok(normalized.to_owned())
}

fn env_or(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_owned())
}

fn env_parse<T>(name: &str, default: T) -> Result<T>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    match env::var(name) {
        Ok(value) => value
            .parse::<T>()
            .with_context(|| format!("{name} has an invalid value")),
        Err(_) => Ok(default),
    }
}

fn env_bool(name: &str, default: bool) -> Result<bool> {
    match env::var(name) {
        Ok(value) => match value.as_str() {
            "1" | "true" | "TRUE" | "yes" | "YES" => Ok(true),
            "0" | "false" | "FALSE" | "no" | "NO" => Ok(false),
            other => bail!("{name} must be a boolean, got {other}"),
        },
        Err(_) => Ok(default),
    }
}

fn shell_join(args: &[String]) -> String {
    args.iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(arg: &str) -> String {
    if arg.is_empty() {
        return "''".to_owned();
    }

    if let Some(rest) = arg.strip_prefix("~/")
        && !rest.is_empty()
        && rest.chars().all(is_safe_shell_word)
    {
        return arg.to_owned();
    }

    if arg.chars().all(is_safe_shell_word) {
        return arg.to_owned();
    }

    format!("'{}'", arg.replace('\'', "'\"'\"'"))
}

fn is_safe_shell_word(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || "-_./:=".contains(ch)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> AgglayerConfig {
        AgglayerConfig {
            bridge_address: DEFAULT_BRIDGE_ADDRESS.to_owned(),
            dest_network: DEFAULT_DEST_NETWORK,
            dest_l1_network: DEFAULT_DEST_L1_NETWORK,
            l2_chain_id: DEFAULT_L2_CHAIN_ID,
            miden_bridge_id: DEFAULT_MIDEN_BRIDGE_ID.to_owned(),
            miden_bridge_hex: "0xc98bb07c188cd2500e13f68a069cdc".to_owned(),
            miden_faucet_id: DEFAULT_MIDEN_FAUCET_ID.to_owned(),
            miden_faucet_hex: "0xe63ba7bc2c19ff603c52c67fa4426d".to_owned(),
            bridge_service_api: DEFAULT_BRIDGE_SERVICE_API.to_owned(),
            miden_node_url: DEFAULT_MIDEN_NODE_URL.to_owned(),
            sepolia_rpc_url: DEFAULT_SEPOLIA_RPC_URL.to_owned(),
            gas_token_address: DEFAULT_GAS_TOKEN_ADDRESS.to_owned(),
            gas_limit: DEFAULT_GAS_LIMIT,
            force_update_global_exit_root: true,
            version_pin: VERSION_PIN,
            source_note: SOURCE_NOTE,
        }
    }

    #[test]
    fn maps_miden_hex_to_bridge_destination_address() {
        let destination = miden_account_to_bridge_destination("0xc98bb07c188cd2500e13f68a069cdc")
            .expect("destination");
        assert_eq!(destination, "0x00000000c98bb07c188cd2500e13f68a069cdc00");
    }

    #[test]
    fn parses_decimal_eth_to_wei() {
        assert_eq!(
            parse_eth_to_wei("0.001").expect("wei").to_string(),
            "1000000000000000"
        );
        assert_eq!(
            parse_eth_to_wei("1.000000000000000001")
                .expect("wei")
                .to_string(),
            "1000000000000000001"
        );
        assert!(parse_eth_to_wei("0.0000000000000000001").is_err());
        assert!(parse_eth_to_wei(&format!("{}0", U256::MAX)).is_err());
    }

    #[test]
    fn normalizes_withdraw_amount_and_rejects_zero() {
        assert_eq!(
            normalize_positive_integer("00010000", "midenWithdrawAmount").expect("amount"),
            "10000"
        );
        assert!(normalize_positive_integer("00", "midenWithdrawAmount").is_err());
    }

    #[test]
    fn l1_plan_is_dry_run_and_uses_live_review_constants() {
        let plan = build_l1_deposit_plan(
            test_config(),
            AgglayerL1DepositPlanRequest {
                miden_account_id: "0xc98bb07c188cd2500e13f68a069cdc".to_owned(),
                amount_eth: Some("0.001".to_owned()),
                eth_keystore: Some("./miden-bali-sepolia".to_owned()),
                sepolia_rpc_url: None,
                gas_limit: None,
                force_update_global_exit_root: None,
            },
        )
        .expect("plan");

        assert!(plan.dry_run);
        assert_eq!(plan.destination_network, 76);
        assert_eq!(plan.amount_wei, "1000000000000000");
        assert!(plan.calldata.starts_with("0x"));
        assert!(plan.shell_command.contains("cast send"));
        assert!(plan.status_url.contains("limit=1&offset=0"));
    }

    #[test]
    fn l2_plan_is_dry_run_handoff_to_bridge_out_tool() {
        let plan = build_l2_withdraw_plan(
            test_config(),
            AgglayerL2WithdrawPlanRequest {
                miden_account_id: "0xc98bb07c188cd2500e13f68a069cdc".to_owned(),
                eth_account_id: "0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc".to_owned(),
                miden_store_dir: Some("~/.miden".to_owned()),
                miden_withdraw_amount: Some("00010000".to_owned()),
                bridge_out_tool: None,
                miden_node_url: None,
            },
        )
        .expect("plan");

        assert!(plan.dry_run);
        assert_eq!(plan.destination_network, 0);
        assert!(plan.shell_command.contains("bridge-out-tool"));
        assert!(plan.shell_command.contains("--dest-network 0"));
        assert!(plan.status_url.contains("/bridges/"));
        assert!(plan.status_url.contains("limit=20&offset=0"));
        assert!(plan.claims_url.contains("/claims/"));
        assert!(
            plan.merkle_proof_url_template
                .contains("/merkle-proof?deposit_cnt=<deposit_cnt>&net_id=<network_id>")
        );
        assert!(
            plan.claim_command_template
                .contains("claimAsset(bytes32[32],bytes32[32],uint256")
        );
        assert_eq!(plan.reference_script, L2_WITHDRAW_REFERENCE_SCRIPT);
        assert!(plan.required_config_keys.contains(&"MIDEN_WITHDRAW_AMOUNT"));
        assert!(plan.required_config_keys.contains(&"DEST_L1_NETWORK"));
        assert!(
            plan.warnings
                .iter()
                .any(|warning| warning.contains("claims URL is post-claim history"))
        );
        assert_eq!(plan.amount, "10000");
    }

    #[test]
    fn selects_ready_unclaimed_miden_origin_deposit() {
        let deposits = vec![
            AgglayerBridgeDeposit {
                leaf_type: Some(1),
                orig_net: 0,
                orig_addr: "0x0000000000000000000000000000000000000000".to_owned(),
                amount: "10000".to_owned(),
                dest_net: 0,
                dest_addr: "0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc".to_owned(),
                block_num: "1".to_owned(),
                deposit_cnt: 1,
                network_id: 0,
                tx_hash: "0xwrong".to_owned(),
                claim_tx_hash: String::new(),
                metadata: "0x".to_owned(),
                ready_for_claim: true,
                global_index: "1".to_owned(),
            },
            AgglayerBridgeDeposit {
                leaf_type: Some(1),
                orig_net: 76,
                orig_addr: "0x0000000000000000000000000000000000000000".to_owned(),
                amount: "10000".to_owned(),
                dest_net: 0,
                dest_addr: "0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc".to_owned(),
                block_num: "2".to_owned(),
                deposit_cnt: 2,
                network_id: 76,
                tx_hash: "0xready".to_owned(),
                claim_tx_hash: String::new(),
                metadata: "0x1234".to_owned(),
                ready_for_claim: true,
                global_index: "18446744073709551618".to_owned(),
            },
            AgglayerBridgeDeposit {
                leaf_type: Some(1),
                orig_net: 76,
                orig_addr: "0x0000000000000000000000000000000000000000".to_owned(),
                amount: "10000".to_owned(),
                dest_net: 0,
                dest_addr: "0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc".to_owned(),
                block_num: "3".to_owned(),
                deposit_cnt: 3,
                network_id: 76,
                tx_hash: "0xclaimed".to_owned(),
                claim_tx_hash: "0xclaim".to_owned(),
                metadata: "0x".to_owned(),
                ready_for_claim: true,
                global_index: "3".to_owned(),
            },
        ];

        let selected = select_ready_l2_deposit(&deposits, 76, 0, None).expect("ready deposit");
        assert_eq!(selected.deposit_cnt, 2);
        assert_eq!(selected.tx_hash, "0xready");

        let filtered =
            select_ready_l2_deposit(&deposits, 76, 0, Some(2)).expect("filtered deposit");
        assert_eq!(filtered.tx_hash, "0xready");
        assert!(select_ready_l2_deposit(&deposits, 76, 0, Some(3)).is_none());
    }

    #[test]
    fn claim_command_uses_bridge_service_deposit_and_proof() {
        let config = test_config();
        let deposit = AgglayerBridgeDeposit {
            leaf_type: Some(1),
            orig_net: 76,
            orig_addr: "0x0000000000000000000000000000000000000000".to_owned(),
            amount: "10000".to_owned(),
            dest_net: 0,
            dest_addr: "0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc".to_owned(),
            block_num: "2".to_owned(),
            deposit_cnt: 2,
            network_id: 76,
            tx_hash: "0xready".to_owned(),
            claim_tx_hash: String::new(),
            metadata: String::new(),
            ready_for_claim: true,
            global_index: "18446744073709551618".to_owned(),
        };
        let proof = BridgeProofData {
            main_exit_root: format!("0x{}", "11".repeat(32)),
            rollup_exit_root: format!("0x{}", "22".repeat(32)),
            merkle_proof: vec![format!("0x{}", "33".repeat(32))],
            rollup_merkle_proof: vec![format!("0x{}", "44".repeat(32))],
        };

        let command = l2_claim_command(
            &config,
            &deposit,
            &proof,
            "https://ethereum-sepolia-rpc.publicnode.com",
            "./sepolia-keystore",
        );
        let shell = shell_join(&command);

        assert!(shell.contains(CLAIM_ASSET_SIGNATURE));
        assert!(shell.contains("18446744073709551618"));
        assert!(shell.contains("--keystore ./sepolia-keystore"));
        assert!(shell.contains("--rpc-url https://ethereum-sepolia-rpc.publicnode.com"));
        assert!(shell.contains("0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc"));
        assert!(shell.contains(" 0x "));
    }

    #[test]
    fn claim_transaction_uses_bridge_service_deposit_and_proof() {
        let config = test_config();
        let deposit = AgglayerBridgeDeposit {
            leaf_type: Some(1),
            orig_net: 76,
            orig_addr: "0x0000000000000000000000000000000000000000".to_owned(),
            amount: "10000".to_owned(),
            dest_net: 0,
            dest_addr: "0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc".to_owned(),
            block_num: "2".to_owned(),
            deposit_cnt: 2,
            network_id: 76,
            tx_hash: "0xready".to_owned(),
            claim_tx_hash: String::new(),
            metadata: "0x1234".to_owned(),
            ready_for_claim: true,
            global_index: "18446744073709551618".to_owned(),
        };
        let proof = BridgeProofData {
            main_exit_root: format!("0x{}", "11".repeat(32)),
            rollup_exit_root: format!("0x{}", "22".repeat(32)),
            merkle_proof: vec![format!("0x{}", "33".repeat(32))],
            rollup_merkle_proof: vec![format!("0x{}", "44".repeat(32))],
        };

        let calldata = claim_asset_calldata(&deposit, &proof).expect("claim calldata");
        let transaction = l2_claim_transaction(&config, calldata.clone());

        assert!(calldata.starts_with("0x"));
        assert_ne!(calldata, "0x");
        assert_eq!(transaction.to, config.bridge_address);
        assert_eq!(transaction.data, calldata);
        assert_eq!(transaction.value, "0x0");
        assert_eq!(transaction.gas, format!("0x{:x}", config.gas_limit));
    }

    #[test]
    fn shell_command_quotes_placeholders_but_keeps_home_expansion() {
        let command = shell_join(&[
            "bridge-out-tool".to_owned(),
            "--store-dir".to_owned(),
            "~/.miden".to_owned(),
            "--keystore".to_owned(),
            "<foundry-keystore-path>".to_owned(),
        ]);

        assert!(command.contains("--store-dir ~/.miden"));
        assert!(command.contains("--keystore '<foundry-keystore-path>'"));
    }
}
