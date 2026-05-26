use std::str::FromStr;

use anyhow::{Result, anyhow};

pub const MIDEN_ASSET_PREFIX: &str = "miden-testnet";
pub const SEPOLIA_EVM_ASSET_PREFIX: &str = "eth-sepolia";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BridgeProfile {
    Sepolia,
}

impl BridgeProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sepolia => "sepolia",
        }
    }

    pub fn evm_asset_prefix(self) -> &'static str {
        match self {
            Self::Sepolia => SEPOLIA_EVM_ASSET_PREFIX,
        }
    }

    pub fn is_evm_asset_id(self, asset_id: &str) -> bool {
        asset_id
            .strip_prefix(self.evm_asset_prefix())
            .is_some_and(|suffix| {
                suffix.starts_with(':') && is_supported_asset_suffix(&suffix[1..])
            })
    }
}

impl FromStr for BridgeProfile {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "sepolia" => Ok(Self::Sepolia),
            other => Err(anyhow!("unsupported bridge profile {other}")),
        }
    }
}

pub fn is_miden_asset_id(asset_id: &str) -> bool {
    asset_id
        .strip_prefix(MIDEN_ASSET_PREFIX)
        .is_some_and(|suffix| suffix.starts_with(':') && is_supported_asset_suffix(&suffix[1..]))
}

pub fn is_evm_asset_id(asset_id: &str) -> bool {
    asset_id
        .strip_prefix(SEPOLIA_EVM_ASSET_PREFIX)
        .is_some_and(|suffix| suffix.starts_with(':') && is_supported_asset_suffix(&suffix[1..]))
}

pub fn is_evm_native_asset(asset_id: &str) -> bool {
    matches!(
        asset_parts(asset_id),
        Some((SEPOLIA_EVM_ASSET_PREFIX, "eth"))
    )
}

pub fn asset_symbol(asset_id: &str) -> Result<&'static str> {
    match asset_parts(asset_id) {
        Some((MIDEN_ASSET_PREFIX | SEPOLIA_EVM_ASSET_PREFIX, "eth")) => Ok("ETH"),
        Some((MIDEN_ASSET_PREFIX | SEPOLIA_EVM_ASSET_PREFIX, "usdc")) => Ok("USDC"),
        Some((MIDEN_ASSET_PREFIX | SEPOLIA_EVM_ASSET_PREFIX, "usdt")) => Ok("USDT"),
        Some((MIDEN_ASSET_PREFIX | SEPOLIA_EVM_ASSET_PREFIX, "btc")) => Ok("BTC"),
        _ => Err(anyhow!("unsupported asset id {asset_id}")),
    }
}

// Miden's BasicFungibleFaucet caps decimals at 12. ETH is 18-decimal on EVM,
// so on the Miden side we represent it at 12 decimals; the bridge scales by
// 10^6 when minting/consuming to keep amounts consistent across chains.
pub fn miden_asset_decimals(asset_id: &str) -> Result<u8> {
    match asset_parts(asset_id) {
        Some((MIDEN_ASSET_PREFIX | SEPOLIA_EVM_ASSET_PREFIX, "eth")) => Ok(12),
        Some((MIDEN_ASSET_PREFIX | SEPOLIA_EVM_ASSET_PREFIX, "usdc")) => Ok(6),
        Some((MIDEN_ASSET_PREFIX | SEPOLIA_EVM_ASSET_PREFIX, "usdt")) => Ok(6),
        Some((MIDEN_ASSET_PREFIX | SEPOLIA_EVM_ASSET_PREFIX, "btc")) => Ok(8),
        _ => Err(anyhow!("unsupported asset id {asset_id}")),
    }
}

pub fn solver_liquidity_for_asset(asset_id: &str) -> Result<u64> {
    match asset_parts(asset_id) {
        Some((MIDEN_ASSET_PREFIX | SEPOLIA_EVM_ASSET_PREFIX, "eth")) => Ok(10_000_000_000_000),
        Some((MIDEN_ASSET_PREFIX | SEPOLIA_EVM_ASSET_PREFIX, "usdc")) => Ok(1_000_000_000_000),
        Some((MIDEN_ASSET_PREFIX | SEPOLIA_EVM_ASSET_PREFIX, "usdt")) => Ok(1_000_000_000_000),
        Some((MIDEN_ASSET_PREFIX | SEPOLIA_EVM_ASSET_PREFIX, "btc")) => Ok(10_000_000_000),
        _ => Err(anyhow!("unsupported asset id {asset_id}")),
    }
}

pub fn evm_quote_requires_deposit_address(origin_asset: &str, destination_asset: &str) -> bool {
    is_evm_asset_id(origin_asset) && is_miden_asset_id(destination_asset)
}

pub fn quote_origin_asset_is_supported(asset_id: &str) -> bool {
    is_evm_asset_id(asset_id)
}

pub fn asset_suffix(asset_id: &str) -> Option<&str> {
    asset_parts(asset_id).map(|(_, suffix)| suffix)
}

fn asset_parts(asset_id: &str) -> Option<(&str, &str)> {
    let (prefix, suffix) = asset_id.split_once(':')?;
    is_supported_asset_prefix(prefix)
        .then_some(suffix)
        .filter(|suffix| is_supported_asset_suffix(suffix))
        .map(|suffix| (prefix, suffix))
}

fn is_supported_asset_prefix(prefix: &str) -> bool {
    matches!(prefix, MIDEN_ASSET_PREFIX | SEPOLIA_EVM_ASSET_PREFIX)
}

fn is_supported_asset_suffix(suffix: &str) -> bool {
    matches!(suffix, "eth" | "usdc" | "usdt" | "btc")
}
