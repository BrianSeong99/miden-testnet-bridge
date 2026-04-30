use serde::{Deserialize, Serialize};

/// Blockchain identifier used in `/v0/tokens` and elsewhere. Spec defines a
/// closed enum (near, eth, base, arb, btc, sol, ton, ...), but our mock emits
/// "miden" for testnet entries — outside the spec set. Keeping this as a free
/// string lets the mock populate non-spec values now; consumers normalize at
/// cutover when the real endpoint replaces ours.
pub type Blockchain = String;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SwapStatus {
    KnownDepositTx,
    PendingDeposit,
    IncompleteDeposit,
    Processing,
    Success,
    Refunded,
    Failed,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SwapType {
    ExactInput,
    ExactOutput,
    FlexInput,
    AnyInput,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DepositMode {
    Simple,
    Memo,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DepositType {
    OriginChain,
    Intents,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RefundType {
    OriginChain,
    Intents,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RecipientType {
    DestinationChain,
    Intents,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    Asc,
    Desc,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WithdrawalStatus {
    Success,
    Failed,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TokenResponse {
    pub asset_id: String,
    pub decimals: f64,
    pub blockchain: Blockchain,
    pub symbol: String,
    pub price: f64,
    pub price_updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_address: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppFee {
    pub recipient: String,
    pub fee: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuoteRequest {
    pub dry: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deposit_mode: Option<DepositMode>,
    pub swap_type: SwapType,
    pub slippage_tolerance: f64,
    pub origin_asset: String,
    pub deposit_type: DepositType,
    pub destination_asset: String,
    pub amount: String,
    pub refund_to: String,
    pub refund_type: RefundType,
    pub recipient: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connected_wallets: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub virtual_chain_recipient: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub virtual_chain_refund_recipient: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_recipient_msg: Option<String>,
    pub recipient_type: RecipientType,
    pub deadline: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referral: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote_waiting_time_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_fees: Option<Vec<AppFee>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Quote {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deposit_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deposit_memo: Option<String>,
    pub amount_in: String,
    pub amount_in_formatted: String,
    pub amount_in_usd: String,
    pub min_amount_in: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_amount_in: Option<String>,
    pub amount_out: String,
    pub amount_out_formatted: String,
    pub amount_out_usd: String,
    pub min_amount_out: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_when_inactive: Option<String>,
    pub time_estimate: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub virtual_chain_recipient: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub virtual_chain_refund_recipient: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_recipient_msg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refund_fee: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuoteResponse {
    pub correlation_id: String,
    pub timestamp: String,
    pub signature: String,
    pub quote_request: QuoteRequest,
    pub quote: Quote,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BadRequestResponse {
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TransactionDetails {
    pub hash: String,
    pub explorer_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SwapDetails {
    pub intent_hashes: Vec<String>,
    pub near_tx_hashes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_in: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_in_formatted: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_in_usd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_out: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_out_formatted: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_out_usd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slippage: Option<f64>,
    pub origin_chain_tx_hashes: Vec<TransactionDetails>,
    pub destination_chain_tx_hashes: Vec<TransactionDetails>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refunded_amount: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refunded_amount_formatted: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refunded_amount_usd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refund_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deposited_amount: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deposited_amount_formatted: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deposited_amount_usd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referral: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatusResponse {
    pub correlation_id: String,
    pub quote_response: QuoteResponse,
    pub status: SwapStatus,
    pub updated_at: String,
    pub swap_details: SwapDetails,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AnyInputQuoteWithdrawal {
    pub status: WithdrawalStatus,
    pub amount_out_formatted: String,
    pub amount_out_usd: String,
    pub amount_out: String,
    pub withdraw_fee_formatted: String,
    pub withdraw_fee: String,
    pub withdraw_fee_usd: String,
    pub timestamp: String,
    pub hash: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawalsResponse {
    pub asset: String,
    pub recipient: String,
    pub affiliate_recipient: String,
    pub withdrawals: Vec<AnyInputQuoteWithdrawal>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubmitDepositTxRequest {
    pub tx_hash: String,
    pub deposit_address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub near_sender_account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubmitDepositTxResponse {
    pub correlation_id: String,
    pub quote_response: QuoteResponse,
    pub status: SwapStatus,
    pub updated_at: String,
    pub swap_details: SwapDetails,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::de::DeserializeOwned;
    use serde_json::json;

    fn round_trip<T>(value: &T)
    where
        T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let serialized = serde_json::to_string(value).expect("serialize");
        let deserialized = serde_json::from_str::<T>(&serialized).expect("deserialize");
        assert_eq!(&deserialized, value);
    }

    fn sample_quote_request() -> QuoteRequest {
        QuoteRequest {
            dry: false,
            deposit_mode: Some(DepositMode::Memo),
            swap_type: SwapType::ExactOutput,
            slippage_tolerance: 100.0,
            origin_asset: "nep141:wrap.near".to_owned(),
            deposit_type: DepositType::OriginChain,
            destination_asset: "nep141:sol-token.omft.near".to_owned(),
            amount: "1000".to_owned(),
            refund_to: "0x2527D02599Ba641c19FEa793cD0F167589a0f10D".to_owned(),
            refund_type: RefundType::OriginChain,
            recipient: "13QkxhNMrTPxoCkRdYdJ65tFuwXPhL5gLS2Z5Nr6gjRK".to_owned(),
            connected_wallets: Some(vec!["0x123".to_owned(), "0x456".to_owned()]),
            session_id: Some("session_abc123".to_owned()),
            virtual_chain_recipient: Some("0xb4c2fbec9d610F9A3a9b843c47b1A8095ceC887C".to_owned()),
            virtual_chain_refund_recipient: Some(
                "0xb4c2fbec9d610F9A3a9b843c47b1A8095ceC887C".to_owned(),
            ),
            custom_recipient_msg: Some("smart-contract-recipient.near".to_owned()),
            recipient_type: RecipientType::DestinationChain,
            deadline: "2026-06-12T00:00:00Z".to_owned(),
            referral: Some("referral".to_owned()),
            quote_waiting_time_ms: Some(3000.0),
            app_fees: Some(vec![AppFee {
                recipient: "recipient.near".to_owned(),
                fee: 100.0,
            }]),
        }
    }

    fn sample_quote() -> Quote {
        Quote {
            deposit_address: Some("0x76b4c56085ED136a8744D52bE956396624a730E8".to_owned()),
            deposit_memo: Some("1111111".to_owned()),
            amount_in: "1000000".to_owned(),
            amount_in_formatted: "1".to_owned(),
            amount_in_usd: "1".to_owned(),
            min_amount_in: "995000".to_owned(),
            max_amount_in: Some("1010000".to_owned()),
            amount_out: "9950000".to_owned(),
            amount_out_formatted: "9.95".to_owned(),
            amount_out_usd: "9.95".to_owned(),
            min_amount_out: "9900000".to_owned(),
            deadline: Some("2026-06-12T00:00:00Z".to_owned()),
            time_when_inactive: Some("2026-06-11T23:50:00Z".to_owned()),
            time_estimate: 120.0,
            virtual_chain_recipient: Some("0xb4c2fbec9d610F9A3a9b843c47b1A8095ceC887C".to_owned()),
            virtual_chain_refund_recipient: Some(
                "0xb4c2fbec9d610F9A3a9b843c47b1A8095ceC887C".to_owned(),
            ),
            custom_recipient_msg: Some("smart-contract-recipient.near".to_owned()),
            refund_fee: Some("10000".to_owned()),
        }
    }

    fn sample_quote_response() -> QuoteResponse {
        QuoteResponse {
            correlation_id: "550e8400-e29b-41d4-a716-446655440000".to_owned(),
            timestamp: "2026-04-30T00:00:00Z".to_owned(),
            signature: String::new(),
            quote_request: sample_quote_request(),
            quote: sample_quote(),
        }
    }

    fn sample_swap_details() -> SwapDetails {
        SwapDetails {
            intent_hashes: vec!["intent-1".to_owned(), "intent-2".to_owned()],
            near_tx_hashes: vec!["near-1".to_owned()],
            amount_in: Some("1000".to_owned()),
            amount_in_formatted: Some("0.1".to_owned()),
            amount_in_usd: Some("0.1".to_owned()),
            amount_out: Some("9950000".to_owned()),
            amount_out_formatted: Some("9.95".to_owned()),
            amount_out_usd: Some("9.95".to_owned()),
            slippage: Some(50.0),
            origin_chain_tx_hashes: vec![TransactionDetails {
                hash: "0x123abc".to_owned(),
                explorer_url: "https://origin.example/tx/0x123abc".to_owned(),
            }],
            destination_chain_tx_hashes: vec![TransactionDetails {
                hash: "0x456def".to_owned(),
                explorer_url: "https://destination.example/tx/0x456def".to_owned(),
            }],
            refunded_amount: Some("100".to_owned()),
            refunded_amount_formatted: Some("0.01".to_owned()),
            refunded_amount_usd: Some("0.01".to_owned()),
            refund_reason: Some("PARTIAL_DEPOSIT".to_owned()),
            deposited_amount: Some("1100".to_owned()),
            deposited_amount_formatted: Some("0.11".to_owned()),
            deposited_amount_usd: Some("0.11".to_owned()),
            referral: Some("referral".to_owned()),
        }
    }

    #[test]
    fn quote_request_round_trip() {
        round_trip(&sample_quote_request());
    }

    #[test]
    fn quote_response_round_trip() {
        round_trip(&sample_quote_response());
    }

    #[test]
    fn status_response_round_trip() {
        let response = StatusResponse {
            correlation_id: "550e8400-e29b-41d4-a716-446655440000".to_owned(),
            quote_response: sample_quote_response(),
            status: SwapStatus::Processing,
            updated_at: "2026-04-30T00:10:00Z".to_owned(),
            swap_details: sample_swap_details(),
        };

        round_trip(&response);
    }

    #[test]
    fn token_response_round_trip() {
        let token = TokenResponse {
            asset_id: "nep141:wrap.near".to_owned(),
            decimals: 24.0,
            blockchain: "near".to_owned(),
            symbol: "wNEAR".to_owned(),
            price: 2.79,
            price_updated_at: "2025-03-28T12:23:00.070Z".to_owned(),
            contract_address: None,
        };

        round_trip(&token);
    }

    #[test]
    fn submit_deposit_tx_request_round_trip() {
        let request = SubmitDepositTxRequest {
            tx_hash: "0x123abc456def789".to_owned(),
            deposit_address: "0x2527D02599Ba641c19FEa793cD0F167589a0f10D".to_owned(),
            near_sender_account: Some("relay.tg".to_owned()),
            memo: Some("123456".to_owned()),
        };

        round_trip(&request);
    }

    #[test]
    fn withdrawals_response_round_trip() {
        let response = WithdrawalsResponse {
            asset: "nep141:sol-token.omft.near".to_owned(),
            recipient: "0x2527d02599ba641c19fea793cd0f167589a0f10d".to_owned(),
            affiliate_recipient: "0xfeedfeedfeedfeedfeedfeedfeedfeedfeedfeed".to_owned(),
            withdrawals: vec![AnyInputQuoteWithdrawal {
                status: WithdrawalStatus::Success,
                amount_out_formatted: "9.95".to_owned(),
                amount_out_usd: "9.95".to_owned(),
                amount_out: "9950000".to_owned(),
                withdraw_fee_formatted: "0.01".to_owned(),
                withdraw_fee: "10000".to_owned(),
                withdraw_fee_usd: "0.01".to_owned(),
                timestamp: "2026-04-30T00:15:00Z".to_owned(),
                hash: "0x123abc456def789".to_owned(),
            }],
        };

        round_trip(&response);
    }

    #[test]
    fn submit_deposit_tx_response_round_trip() {
        let response = SubmitDepositTxResponse {
            correlation_id: "550e8400-e29b-41d4-a716-446655440000".to_owned(),
            quote_response: sample_quote_response(),
            status: SwapStatus::KnownDepositTx,
            updated_at: "2026-04-30T00:20:00Z".to_owned(),
            swap_details: sample_swap_details(),
        };

        round_trip(&response);
    }

    #[test]
    fn bad_request_response_round_trip() {
        round_trip(&BadRequestResponse {
            message: "error message".to_owned(),
        });
    }

    #[test]
    fn quote_response_rejects_null_required_fields() {
        let fixture = json!({
            "correlationId": null,
            "timestamp": "2026-04-30T00:00:00Z",
            "signature": null,
            "quoteRequest": {
                "dry": false,
                "swapType": "EXACT_INPUT",
                "slippageTolerance": 100.0,
                "originAsset": "nep141:wrap.near",
                "depositType": "ORIGIN_CHAIN",
                "destinationAsset": "nep141:sol-token.omft.near",
                "amount": "1000",
                "refundTo": "0x2527D02599Ba641c19FEa793cD0F167589a0f10D",
                "refundType": "ORIGIN_CHAIN",
                "recipient": "13QkxhNMrTPxoCkRdYdJ65tFuwXPhL5gLS2Z5Nr6gjRK",
                "recipientType": "DESTINATION_CHAIN",
                "deadline": "2026-06-12T00:00:00Z"
            },
            "quote": {
                "amountIn": "1000000",
                "amountInFormatted": "1",
                "amountInUsd": "1",
                "minAmountIn": "995000",
                "amountOut": "9950000",
                "amountOutFormatted": "9.95",
                "amountOutUsd": "9.95",
                "minAmountOut": "9900000",
                "timeEstimate": 120.0
            }
        });

        let result = serde_json::from_value::<QuoteResponse>(fixture);
        assert!(result.is_err());
    }

    #[test]
    fn swap_status_deserializes_all_canonical_variants() {
        let cases = [
            ("KNOWN_DEPOSIT_TX", SwapStatus::KnownDepositTx),
            ("PENDING_DEPOSIT", SwapStatus::PendingDeposit),
            ("INCOMPLETE_DEPOSIT", SwapStatus::IncompleteDeposit),
            ("PROCESSING", SwapStatus::Processing),
            ("SUCCESS", SwapStatus::Success),
            ("REFUNDED", SwapStatus::Refunded),
            ("FAILED", SwapStatus::Failed),
        ];

        for (raw, expected) in cases {
            let parsed = serde_json::from_str::<SwapStatus>(&format!("\"{raw}\"")).expect("status");
            assert_eq!(parsed, expected);
        }
    }
}
