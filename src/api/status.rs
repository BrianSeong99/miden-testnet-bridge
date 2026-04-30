use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;

use crate::{
    AppState,
    api::errors::ApiError,
    core::state::QuoteRecord,
    types::{StatusResponse, SwapDetails, SwapStatus},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusQuery {
    deposit_address: String,
    deposit_memo: Option<String>,
}

pub(crate) async fn status(
    State(state): State<AppState>,
    Query(query): Query<StatusQuery>,
) -> Result<Json<StatusResponse>, ApiError> {
    let record = state
        .store
        .get_quote_by_deposit(&query.deposit_address, query.deposit_memo.as_deref())
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?
        .ok_or_else(|| ApiError::not_found("deposit address not found"))?;

    Ok(Json(status_from_record(record)))
}

pub(crate) fn empty_swap_details() -> SwapDetails {
    SwapDetails {
        intent_hashes: Vec::new(),
        near_tx_hashes: Vec::new(),
        amount_in: None,
        amount_in_formatted: None,
        amount_in_usd: None,
        amount_out: None,
        amount_out_formatted: None,
        amount_out_usd: None,
        slippage: None,
        origin_chain_tx_hashes: Vec::new(),
        destination_chain_tx_hashes: Vec::new(),
        refunded_amount: None,
        refunded_amount_formatted: None,
        refunded_amount_usd: None,
        refund_reason: None,
        deposited_amount: None,
        deposited_amount_formatted: None,
        deposited_amount_usd: None,
        referral: None,
    }
}

fn status_from_record(record: QuoteRecord) -> StatusResponse {
    StatusResponse {
        correlation_id: record.correlation_id.to_string(),
        updated_at: record
            .updated_at
            .format(&time::format_description::well_known::Rfc3339)
            .expect("RFC3339 formatting should succeed"),
        quote_response: record.quote_response,
        status: swap_status_from_db(&record.status),
        swap_details: SwapDetails {
            intent_hashes: record.intent_hashes,
            near_tx_hashes: record.near_tx_hashes,
            amount_in: None,
            amount_in_formatted: None,
            amount_in_usd: None,
            amount_out: None,
            amount_out_formatted: None,
            amount_out_usd: None,
            slippage: None,
            origin_chain_tx_hashes: record
                .evm_deposit_tx_hashes
                .into_iter()
                .map(transaction_details)
                .collect(),
            destination_chain_tx_hashes: record
                .evm_release_tx_hashes
                .into_iter()
                .map(transaction_details)
                .collect(),
            refunded_amount: None,
            refunded_amount_formatted: None,
            refunded_amount_usd: None,
            refund_reason: None,
            deposited_amount: None,
            deposited_amount_formatted: None,
            deposited_amount_usd: None,
            referral: None,
        },
    }
}

fn swap_status_from_db(status: &str) -> SwapStatus {
    match status {
        "KNOWN_DEPOSIT_TX" => SwapStatus::KnownDepositTx,
        "PENDING_DEPOSIT" => SwapStatus::PendingDeposit,
        "INCOMPLETE_DEPOSIT" => SwapStatus::IncompleteDeposit,
        "PROCESSING" => SwapStatus::Processing,
        "SUCCESS" => SwapStatus::Success,
        "REFUNDED" => SwapStatus::Refunded,
        "FAILED" => SwapStatus::Failed,
        _ => SwapStatus::Failed,
    }
}

fn transaction_details(hash: String) -> crate::types::TransactionDetails {
    crate::types::TransactionDetails {
        explorer_url: format!("https://example.com/tx/{hash}"),
        hash,
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    use crate::{AppState, app, test_support::memory_state, types::StatusResponse};

    #[tokio::test]
    async fn dry_quote_is_not_persisted() {
        let app = app(AppState::new(memory_state()));
        let quote_response = app
            .clone()
            .oneshot(
                Request::post("/v0/quote")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "dry": true,
                            "depositMode": "SIMPLE",
                            "swapType": "EXACT_INPUT",
                            "slippageTolerance": 100.0,
                            "originAsset": "eth-anvil:eth",
                            "depositType": "ORIGIN_CHAIN",
                            "destinationAsset": "miden-local:eth",
                            "amount": "1000",
                            "refundTo": "0xfeed",
                            "refundType": "ORIGIN_CHAIN",
                            "recipient": "recipient",
                            "recipientType": "DESTINATION_CHAIN",
                            "deadline": "2026-06-12T00:00:00Z"
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("quote response");

        assert_eq!(quote_response.status(), StatusCode::OK);

        let body = to_bytes(quote_response.into_body(), usize::MAX)
            .await
            .expect("body");
        let quote: crate::types::QuoteResponse =
            serde_json::from_slice(&body).expect("quote response");
        let would_be_deposit_address = format!("mock-{}", quote.correlation_id);

        let response = app
            .oneshot(
                Request::get(format!(
                    "/v0/status?depositAddress={would_be_deposit_address}"
                ))
                .body(Body::empty())
                .expect("request"),
            )
            .await
            .expect("status response");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn persisted_quote_returns_pending_deposit_status() {
        let app = app(AppState::new(memory_state()));
        let quote_response = app
            .clone()
            .oneshot(
                Request::post("/v0/quote")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "dry": false,
                            "depositMode": "SIMPLE",
                            "swapType": "EXACT_INPUT",
                            "slippageTolerance": 100.0,
                            "originAsset": "eth-anvil:eth",
                            "depositType": "ORIGIN_CHAIN",
                            "destinationAsset": "miden-local:eth",
                            "amount": "1000",
                            "refundTo": "0xfeed",
                            "refundType": "ORIGIN_CHAIN",
                            "recipient": "recipient",
                            "recipientType": "DESTINATION_CHAIN",
                            "deadline": "2026-06-12T00:00:00Z"
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("quote response");

        let body = to_bytes(quote_response.into_body(), usize::MAX)
            .await
            .expect("body");
        let quote: crate::types::QuoteResponse =
            serde_json::from_slice(&body).expect("quote response");
        let deposit_address = quote.quote.deposit_address.expect("deposit address");

        let response = app
            .oneshot(
                Request::get(format!("/v0/status?depositAddress={deposit_address}"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("status response");

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let status: StatusResponse = serde_json::from_slice(&body).expect("status response");

        assert_eq!(status.status, crate::types::SwapStatus::PendingDeposit);
    }

    #[tokio::test]
    async fn unknown_deposit_address_returns_not_found() {
        let app = app(AppState::new(memory_state()));
        let response = app
            .oneshot(
                Request::get("/v0/status?depositAddress=unknown")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
