use axum::Json;

use crate::{
    chains::evm::{load_token_address_file, token_addresses_path_from_env},
    now_iso8601,
    types::TokenResponse,
};

pub async fn tokens() -> Json<Vec<TokenResponse>> {
    let timestamp = now_iso8601();
    let addresses = load_token_address_file(&token_addresses_path_from_env()).unwrap_or_default();

    Json(vec![
        TokenResponse {
            asset_id: "eth-anvil:eth".to_owned(),
            decimals: 18.0,
            blockchain: "eth".to_owned(),
            symbol: "ETH".to_owned(),
            price: 1.0,
            price_updated_at: timestamp.clone(),
            contract_address: None,
        },
        TokenResponse {
            asset_id: "eth-anvil:usdc".to_owned(),
            decimals: 6.0,
            blockchain: "eth".to_owned(),
            symbol: "USDC".to_owned(),
            price: 1.0,
            price_updated_at: timestamp.clone(),
            contract_address: addresses.usdc.clone(),
        },
        TokenResponse {
            asset_id: "eth-anvil:usdt".to_owned(),
            decimals: 6.0,
            blockchain: "eth".to_owned(),
            symbol: "USDT".to_owned(),
            price: 1.0,
            price_updated_at: timestamp.clone(),
            contract_address: addresses.usdt.clone(),
        },
        TokenResponse {
            asset_id: "eth-anvil:btc".to_owned(),
            decimals: 8.0,
            blockchain: "eth".to_owned(),
            symbol: "BTC".to_owned(),
            price: 1.0,
            price_updated_at: timestamp.clone(),
            contract_address: addresses.btc.clone(),
        },
        TokenResponse {
            asset_id: "miden-local:eth".to_owned(),
            decimals: 18.0,
            blockchain: "miden".to_owned(),
            symbol: "ETH".to_owned(),
            price: 1.0,
            price_updated_at: timestamp.clone(),
            contract_address: None,
        },
        TokenResponse {
            asset_id: "miden-local:usdc".to_owned(),
            decimals: 6.0,
            blockchain: "miden".to_owned(),
            symbol: "USDC".to_owned(),
            price: 1.0,
            price_updated_at: timestamp.clone(),
            contract_address: None,
        },
        TokenResponse {
            asset_id: "miden-local:usdt".to_owned(),
            decimals: 6.0,
            blockchain: "miden".to_owned(),
            symbol: "USDT".to_owned(),
            price: 1.0,
            price_updated_at: timestamp.clone(),
            contract_address: None,
        },
        TokenResponse {
            asset_id: "miden-local:btc".to_owned(),
            decimals: 8.0,
            blockchain: "miden".to_owned(),
            symbol: "BTC".to_owned(),
            price: 1.0,
            price_updated_at: timestamp,
            contract_address: None,
        },
    ])
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
}
