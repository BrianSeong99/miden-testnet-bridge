use std::str::FromStr;

use axum::{Json, extract::State};

use crate::{
    AppState,
    api::errors::ApiError,
    chains::evm::{load_token_address_file, token_addresses_path_from_env},
    chains::profile::BridgeProfile,
    now_iso8601,
    types::TokenResponse,
};

pub async fn tokens(State(state): State<AppState>) -> Result<Json<Vec<TokenResponse>>, ApiError> {
    let timestamp = now_iso8601();
    let addresses = load_token_address_file(&token_addresses_path_from_env()).unwrap_or_default();
    let profile = BridgeProfile::from_str(&state.runtime_profile)
        .map_err(|error| ApiError::internal(error.to_string()))?;
    let mut tokens = evm_tokens(profile, &addresses, &timestamp);
    tokens.extend(miden_tokens(&timestamp));

    for token in &mut tokens {
        let unit_amount = format!("1{}", "0".repeat(token.decimals as usize));
        let quote = state
            .pricer
            .quote(&token.symbol, &token.symbol, &unit_amount)
            .await
            .map_err(|error| ApiError::internal(error.to_string()))?;
        token.price = quote
            .input_usd
            .parse::<f64>()
            .map_err(|error| ApiError::internal(error.to_string()))?;
    }

    Ok(Json(tokens))
}

fn evm_tokens(
    profile: BridgeProfile,
    addresses: &crate::chains::evm::TokenAddressFile,
    timestamp: &str,
) -> Vec<TokenResponse> {
    let prefix = profile.evm_asset_prefix();
    let mut tokens = vec![token(
        &format!("{prefix}:eth"),
        18.0,
        "eth",
        "ETH",
        None,
        timestamp,
    )];

    if profile == BridgeProfile::Anvil || addresses.usdc.is_some() {
        tokens.push(token(
            &format!("{prefix}:usdc"),
            6.0,
            "eth",
            "USDC",
            addresses.usdc.clone(),
            timestamp,
        ));
    }
    if profile == BridgeProfile::Anvil || addresses.usdt.is_some() {
        tokens.push(token(
            &format!("{prefix}:usdt"),
            6.0,
            "eth",
            "USDT",
            addresses.usdt.clone(),
            timestamp,
        ));
    }
    if profile == BridgeProfile::Anvil || addresses.btc.is_some() {
        tokens.push(token(
            &format!("{prefix}:btc"),
            8.0,
            "eth",
            "BTC",
            addresses.btc.clone(),
            timestamp,
        ));
    }

    tokens
}

fn miden_tokens(timestamp: &str) -> Vec<TokenResponse> {
    vec![
        token("miden-testnet:eth", 18.0, "miden", "ETH", None, timestamp),
        token("miden-testnet:usdc", 6.0, "miden", "USDC", None, timestamp),
        token("miden-testnet:usdt", 6.0, "miden", "USDT", None, timestamp),
        token("miden-testnet:btc", 8.0, "miden", "BTC", None, timestamp),
    ]
}

fn token(
    asset_id: &str,
    decimals: f64,
    blockchain: &str,
    symbol: &str,
    contract_address: Option<String>,
    timestamp: &str,
) -> TokenResponse {
    TokenResponse {
        asset_id: asset_id.to_owned(),
        decimals,
        blockchain: blockchain.to_owned(),
        symbol: symbol.to_owned(),
        price: 0.0,
        price_updated_at: timestamp.to_owned(),
        contract_address,
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    use crate::{AppState, app, test_support::memory_state, types::TokenResponse};

    #[tokio::test]
    async fn returns_supported_tokens() {
        let app = app(AppState::new(memory_state()));
        let response = app
            .oneshot(
                Request::get("/v0/tokens")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let tokens: Vec<TokenResponse> = serde_json::from_slice(&body).expect("tokens");

        assert!(tokens.len() >= 4);
    }

    #[tokio::test]
    async fn sepolia_profile_returns_sepolia_eth_without_anvil_assets() {
        let app = app(AppState::new(memory_state()).with_runtime_options(
            false,
            false,
            "sepolia".to_owned(),
        ));
        let response = app
            .oneshot(
                Request::get("/v0/tokens")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let tokens: Vec<TokenResponse> = serde_json::from_slice(&body).expect("tokens");

        assert!(
            tokens
                .iter()
                .any(|token| token.asset_id == "eth-sepolia:eth")
        );
        assert!(
            tokens
                .iter()
                .any(|token| token.asset_id == "miden-testnet:eth")
        );
        assert!(!tokens.iter().any(|token| token.asset_id == "eth-anvil:eth"));
    }
}
