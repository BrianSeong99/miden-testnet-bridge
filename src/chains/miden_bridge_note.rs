use anyhow::Result;
use miden_client::{
    Felt,
    account::AccountId,
    asset::Asset,
    crypto::FeltRng,
    note::{
        Note, NoteAssets, NoteAttachment, NoteMetadata, NoteRecipient, NoteScript, NoteStorage,
        NoteTag, NoteType,
    },
};
use miden_protocol::assembly::diagnostics::NamedSource;
use miden_standards::code_builder::CodeBuilder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{chains::miden::parse_account_id, types::QuoteRequest};

pub const BRIDGE_OUT_MEMO_VERSION: &str = "bridge-out-v1";
pub const BRIDGE_OUT_NOTE_TYPE: &str = "PUBLIC";
pub const BRIDGE_OUT_STORAGE_SCHEMA: &str = "bridge_out_v1";
pub const BRIDGE_OUT_STORAGE_ENCODING: &str = "target_account_id_plus_sha256_u32_be_v1";
pub const NETWORK_ACCOUNT_ATTACHMENT_SCHEME: &str = "NetworkAccountTarget";
pub const NETWORK_ACCOUNT_EXECUTION_HINT: &str = "Always";
pub const BRIDGE_OUT_TARGET_STORAGE_ITEMS: usize = 2;
pub const BRIDGE_OUT_QUOTE_HASH_STORAGE_ITEMS: usize = 8;
pub const BRIDGE_OUT_STORAGE_ITEMS: usize =
    BRIDGE_OUT_TARGET_STORAGE_ITEMS + BRIDGE_OUT_QUOTE_HASH_STORAGE_ITEMS;

const BRIDGE_OUT_NOTE_SCRIPT_NAME: &str = "bridge::notes::bridge_out_v1";
const BRIDGE_OUT_NOTE_SCRIPT: &str = r#"
use miden::protocol::active_account
use miden::protocol::account_id
use miden::protocol::active_note
use miden::standards::wallets::basic->basic_wallet

const ERR_BRIDGE_OUT_UNEXPECTED_NUMBER_OF_STORAGE_ITEMS="BridgeOutV1 note expects exactly 10 note storage items"
const ERR_BRIDGE_OUT_TARGET_ACCT_MISMATCH="BridgeOutV1 target account and transaction account do not match"

const STORAGE_PTR = 0
const TARGET_ACCOUNT_ID_SUFFIX_PTR = STORAGE_PTR
const TARGET_ACCOUNT_ID_PREFIX_PTR = STORAGE_PTR + 1

@note_script
pub proc main
    push.STORAGE_PTR exec.active_note::get_storage
    # => [num_storage_items, storage_ptr]

    eq.10 assert.err=ERR_BRIDGE_OUT_UNEXPECTED_NUMBER_OF_STORAGE_ITEMS
    # => [storage_ptr]

    drop
    mem_load.TARGET_ACCOUNT_ID_PREFIX_PTR
    mem_load.TARGET_ACCOUNT_ID_SUFFIX_PTR
    # => [target_account_id_suffix, target_account_id_prefix]

    exec.active_account::get_id
    # => [account_id_suffix, account_id_prefix, target_account_id_suffix, target_account_id_prefix]

    exec.account_id::is_equal assert.err=ERR_BRIDGE_OUT_TARGET_ACCT_MISMATCH
    # => []

    exec.basic_wallet::add_assets_to_account
    # => []
end
"#;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeOutDepositMemo {
    pub version: String,
    pub note_type: String,
    pub storage_schema: String,
    pub storage_encoding: String,
    pub bridge_account_id: String,
    pub attachment_scheme: String,
    pub execution_hint: String,
    pub storage: BridgeOutStoragePayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeOutStoragePayload {
    pub quote_hash: String,
    pub correlation_id: String,
    pub origin_asset: String,
    pub destination_asset: String,
    pub amount_in: String,
    pub min_amount_out: String,
    pub destination_recipient: String,
    pub refund_account: String,
    pub deadline: String,
    pub storage_items: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BridgeOutHashMaterial<'a> {
    version: &'static str,
    correlation_id: Uuid,
    origin_asset: &'a str,
    destination_asset: &'a str,
    amount_in: &'a str,
    min_amount_out: &'a str,
    destination_recipient: &'a str,
    refund_account: &'a str,
    deadline: &'a str,
    bridge_account_id: &'a str,
}

impl BridgeOutDepositMemo {
    pub fn from_quote(
        request: &QuoteRequest,
        correlation_id: Uuid,
        min_amount_out: &str,
        bridge_account_id: &str,
    ) -> Result<Self> {
        let quote_hash =
            bridge_out_quote_hash(request, correlation_id, min_amount_out, bridge_account_id)?;
        let storage_items = bridge_out_storage_felts(&quote_hash, bridge_account_id)?
            .into_iter()
            .map(|value| value.as_canonical_u64().to_string())
            .collect();

        Ok(Self {
            version: BRIDGE_OUT_MEMO_VERSION.to_owned(),
            note_type: BRIDGE_OUT_NOTE_TYPE.to_owned(),
            storage_schema: BRIDGE_OUT_STORAGE_SCHEMA.to_owned(),
            storage_encoding: BRIDGE_OUT_STORAGE_ENCODING.to_owned(),
            bridge_account_id: bridge_account_id.to_owned(),
            attachment_scheme: NETWORK_ACCOUNT_ATTACHMENT_SCHEME.to_owned(),
            execution_hint: NETWORK_ACCOUNT_EXECUTION_HINT.to_owned(),
            storage: BridgeOutStoragePayload {
                quote_hash,
                correlation_id: correlation_id.to_string(),
                origin_asset: request.origin_asset.clone(),
                destination_asset: request.destination_asset.clone(),
                amount_in: request.amount.clone(),
                min_amount_out: min_amount_out.to_owned(),
                destination_recipient: request.recipient.clone(),
                refund_account: request.refund_to.clone(),
                deadline: request.deadline.clone(),
                storage_items,
            },
        })
    }

    pub fn from_deposit_memo(value: &str) -> Result<Self> {
        Ok(serde_json::from_str(value)?)
    }

    pub fn to_deposit_memo(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn storage_felts(&self) -> Result<Vec<Felt>> {
        self.storage
            .storage_items
            .iter()
            .map(|value| {
                let parsed = value.parse::<u64>()?;
                Ok(Felt::new(parsed))
            })
            .collect()
    }

    pub fn matches_note_storage(&self, storage_items: &[Felt]) -> Result<bool> {
        Ok(storage_items == self.storage_felts()?.as_slice())
    }
}

pub fn bridge_out_quote_hash(
    request: &QuoteRequest,
    correlation_id: Uuid,
    min_amount_out: &str,
    bridge_account_id: &str,
) -> Result<String> {
    let material = BridgeOutHashMaterial {
        version: BRIDGE_OUT_MEMO_VERSION,
        correlation_id,
        origin_asset: &request.origin_asset,
        destination_asset: &request.destination_asset,
        amount_in: &request.amount,
        min_amount_out,
        destination_recipient: &request.recipient,
        refund_account: &request.refund_to,
        deadline: &request.deadline,
        bridge_account_id,
    };
    let encoded = serde_json::to_vec(&material)?;
    let hash = Sha256::digest(encoded);
    Ok(format!("0x{}", alloy::hex::encode(hash)))
}

pub fn bridge_out_note_script() -> Result<NoteScript> {
    CodeBuilder::new()
        .compile_note_script(NamedSource::new(
            BRIDGE_OUT_NOTE_SCRIPT_NAME,
            BRIDGE_OUT_NOTE_SCRIPT,
        ))
        .map_err(|error| anyhow::anyhow!("{error}"))
}

pub fn build_bridge_out_note<R: FeltRng>(
    sender_account_id: AccountId,
    assets: Vec<Asset>,
    memo: &BridgeOutDepositMemo,
    rng: &mut R,
) -> Result<Note> {
    let bridge_account_id = parse_account_id(&memo.bridge_account_id)?;
    let serial_num = rng.draw_word();
    let recipient = NoteRecipient::new(
        serial_num,
        bridge_out_note_script()?,
        NoteStorage::new(memo.storage_felts()?)?,
    );
    let metadata = NoteMetadata::new(sender_account_id, NoteType::Public)
        .with_tag(NoteTag::with_account_target(bridge_account_id))
        .with_attachment(NoteAttachment::default());
    let vault = NoteAssets::new(assets)?;

    Ok(Note::new(vault, metadata, recipient))
}

fn bridge_out_storage_felts(quote_hash: &str, bridge_account_id: &str) -> Result<Vec<Felt>> {
    let bridge_account_id = parse_account_id(bridge_account_id)?;
    let mut items = vec![
        bridge_account_id.suffix(),
        bridge_account_id.prefix().as_felt(),
    ];
    items.extend(quote_hash_storage_felts(quote_hash)?);
    Ok(items)
}

fn quote_hash_storage_felts(quote_hash: &str) -> Result<Vec<Felt>> {
    let hash = quote_hash.strip_prefix("0x").unwrap_or(quote_hash);
    let bytes = alloy::hex::decode(hash)?;
    anyhow::ensure!(
        bytes.len() == 32,
        "bridge note quote hash must decode to 32 bytes"
    );
    let items = bytes
        .chunks_exact(4)
        .map(|chunk| {
            Felt::new(u64::from(u32::from_be_bytes(
                chunk.try_into().expect("chunk size is exact"),
            )))
        })
        .collect();
    Ok(items)
}

#[cfg(test)]
mod tests {
    use miden_client::account::{AccountStorageMode, AccountType};

    use crate::test_support::test_miden_account_id;
    use crate::types::{DepositType, QuoteRequest, RecipientType, RefundType, SwapType};

    use super::*;

    fn bridge_account_id() -> String {
        test_miden_account_id(
            AccountType::RegularAccountUpdatableCode,
            AccountStorageMode::Private,
            0xccdd_eeff,
        )
        .to_hex()
    }

    fn request() -> QuoteRequest {
        QuoteRequest {
            dry: false,
            deposit_mode: None,
            swap_type: SwapType::ExactInput,
            slippage_tolerance: 100.0,
            origin_asset: "miden-testnet:eth".to_owned(),
            deposit_type: DepositType::OriginChain,
            destination_asset: "eth-anvil:eth".to_owned(),
            amount: "1000".to_owned(),
            refund_to: "0xrefund".to_owned(),
            refund_type: RefundType::OriginChain,
            recipient: "0xrecipient".to_owned(),
            connected_wallets: None,
            session_id: None,
            virtual_chain_recipient: None,
            virtual_chain_refund_recipient: None,
            custom_recipient_msg: None,
            recipient_type: RecipientType::DestinationChain,
            deadline: "2026-06-12T00:00:00Z".to_owned(),
            referral: None,
            quote_waiting_time_ms: None,
            app_fees: None,
        }
    }

    #[test]
    fn memo_serializes_expected_bridge_out_fields() {
        let correlation_id = Uuid::nil();
        let bridge_account_id = bridge_account_id();
        let memo =
            BridgeOutDepositMemo::from_quote(&request(), correlation_id, "900", &bridge_account_id)
                .expect("memo");
        let encoded = memo.to_deposit_memo().expect("deposit memo");
        let decoded: BridgeOutDepositMemo = serde_json::from_str(&encoded).expect("decode memo");

        assert_eq!(decoded.version, BRIDGE_OUT_MEMO_VERSION);
        assert_eq!(decoded.note_type, BRIDGE_OUT_NOTE_TYPE);
        assert_eq!(decoded.storage_schema, BRIDGE_OUT_STORAGE_SCHEMA);
        assert_eq!(decoded.storage_encoding, BRIDGE_OUT_STORAGE_ENCODING);
        assert_eq!(decoded.bridge_account_id, bridge_account_id);
        assert_eq!(decoded.storage.correlation_id, correlation_id.to_string());
        assert_eq!(decoded.storage.amount_in, "1000");
        assert_eq!(decoded.storage.min_amount_out, "900");
        assert!(decoded.storage.quote_hash.starts_with("0x"));
        assert_eq!(
            decoded.storage.storage_items.len(),
            BRIDGE_OUT_STORAGE_ITEMS
        );
        assert!(
            decoded
                .matches_note_storage(&decoded.storage_felts().expect("felts"))
                .expect("match")
        );
    }

    #[test]
    fn bridge_out_note_script_compiles() {
        let script = bridge_out_note_script().expect("note script");

        assert_ne!(script.root(), miden_client::EMPTY_WORD);
    }
}
