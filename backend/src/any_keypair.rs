use anyhow::format_err;
use serde_with::DeserializeFromStr;
use solana_sdk::{
    derivation_path::DerivationPath,
    signature::{generate_seed_from_seed_phrase_and_passphrase, Keypair, SeedDerivable},
};
use std::str::FromStr;

#[derive(Debug, DeserializeFromStr)]
pub struct AnyKeypair(pub Keypair);

impl FromStr for AnyKeypair {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_base58(s)
            .or_else(|_| Self::from_json(s))
            .or_else(|_| Self::from_mnemonic(s))
    }
}

impl From<AnyKeypair> for Keypair {
    fn from(value: AnyKeypair) -> Self {
        value.0
    }
}

impl AnyKeypair {
    fn from_json(s: &str) -> anyhow::Result<Self> {
        let bytes: Vec<u8> = serde_json::from_str(s)?;
        Keypair::from_bytes(&bytes).map_err(Into::into).map(Self)
    }

    fn from_base58(s: &str) -> anyhow::Result<Self> {
        Keypair::from_bytes(&bs58::decode(s).into_vec()?)
            .map_err(Into::into)
            .map(Self)
    }

    fn from_mnemonic(seed_phrase: &str) -> anyhow::Result<Self> {
        let seed = generate_seed_from_seed_phrase_and_passphrase(seed_phrase, "");
        Keypair::from_seed_and_derivation_path(
            &seed,
            DerivationPath::from_absolute_path_str("m/44'/501'/0'/0'")
                .unwrap()
                .into(),
        )
        .map(Self)
        .map_err(|err| format_err!("Failed to derive keypair: {}", err))
    }
}
