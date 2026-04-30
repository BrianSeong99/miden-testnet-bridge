use anyhow::Result;
use miden_client::{
    account::component::BasicWallet,
    account::{Account, AccountBuilder, AccountStorageMode, AccountType},
    auth::{AuthSchemeId, AuthSecretKey, AuthSingleSig},
};
use miden_standards::account::metadata::AccountBuilderSchemaCommitmentExt;
use rand::{SeedableRng, rngs::StdRng};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub fn derive_outbound_deposit_account(
    master_seed: &[u8; 32],
    correlation_id: Uuid,
) -> Result<(Account, AuthSecretKey, [u8; 32], [u8; 32])> {
    let init_seed = derive_seed(master_seed, correlation_id, "miden_account_seed");
    let auth_seed = derive_seed(master_seed, correlation_id, "miden_auth_key");
    let secret_key = derive_auth_secret_key(auth_seed);
    let account = build_wallet_account(init_seed, &secret_key)?;

    Ok((account, secret_key, init_seed, auth_seed))
}

pub fn re_derive_outbound_deposit_account(
    master_seed: &[u8; 32],
    correlation_id: Uuid,
) -> Result<(Account, AuthSecretKey)> {
    let (account, secret_key, _, _) = derive_outbound_deposit_account(master_seed, correlation_id)?;
    Ok((account, secret_key))
}

pub fn build_wallet_account(init_seed: [u8; 32], secret_key: &AuthSecretKey) -> Result<Account> {
    Ok(AccountBuilder::new(init_seed)
        .account_type(AccountType::RegularAccountImmutableCode)
        .storage_mode(AccountStorageMode::Private)
        .with_auth_component(AuthSingleSig::new(
            secret_key.public_key().to_commitment(),
            AuthSchemeId::Falcon512Poseidon2,
        ))
        .with_component(BasicWallet)
        .build_with_schema_commitment()?)
}

pub fn derive_seed(master_seed: &[u8; 32], correlation_id: Uuid, label: &str) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(master_seed);
    digest.update(correlation_id.as_bytes());
    digest.update(label.as_bytes());
    digest.finalize().into()
}

pub fn derive_auth_secret_key(auth_seed: [u8; 32]) -> AuthSecretKey {
    let mut rng = StdRng::from_seed(auth_seed);
    AuthSecretKey::new_falcon512_poseidon2_with_rng(&mut rng)
}
