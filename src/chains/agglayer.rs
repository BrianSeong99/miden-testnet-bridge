use std::{env, str::FromStr};

use alloy::{
    primitives::{Address, Bytes, U256},
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
pub const SOURCE_NOTE: &str = "0xMiden/miden-client#2173 review notes, rechecked 2026-05-26";

sol! {
    function bridgeAsset(uint32 destinationNetwork, address destinationAddress, uint256 amount, address token, bool forceUpdateGlobalExitRoot, bytes permitData);
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
    pub warnings: Vec<String>,
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
            "Submit a B2AGG note from the Miden wallet.",
            "Poll the Gateway FM bridge service for claim readiness.",
            "Use the reference claimAsset script once a proof is ready.",
        ],
        warnings: vec![
            "Testnet only. Never use mainnet funds or production keys.",
            "The public tutorial is still an open PR; recheck constants before a funded run.",
            "This service returns dry-run plans and status URLs; live broadcast remains an operator action.",
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
    let status_url = format!(
        "{}/bridges/{}",
        config.bridge_service_api.trim_end_matches('/'),
        request.eth_account_id
    );

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
        warnings: vec![
            "Dry-run only: this service does not submit B2AGG notes or claim assets.".to_owned(),
            "Build gateway-fm/miden-agglayer bridge-out-tool before running the command."
                .to_owned(),
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
        assert_eq!(plan.amount, "10000");
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
