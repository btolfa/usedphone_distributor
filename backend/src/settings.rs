use crate::any_keypair::AnyKeypair;
use anyhow::{bail, Context};
use shuttle_secrets::SecretStore;
use solana_sdk::{pubkey::Pubkey, signature::Keypair};

pub struct Settings {
    pub solana_rpc_url: String,
    pub payer: Keypair,
    pub distributor_authority: Keypair,

    pub distributor_state: Pubkey,
    pub program_id: Pubkey,
}

impl TryFrom<&SecretStore> for Settings {
    type Error = anyhow::Error;

    fn try_from(secret_store: &SecretStore) -> Result<Self, Self::Error> {
        let Some(solana_rpc_url) = secret_store.get("SOLANA_RPC_URL") else {
            bail!("SOLANA_RPC_URL not found in secret store");
        };
        let Some(AnyKeypair(payer)) = secret_store
            .get("PAYER_KEYPAIR")
            .map(|secret| secret.parse())
            .transpose()
            .context("Can't deserialize PAYER_KEYPAIR")?
        else {
            bail!("PAYER_KEYPAIR not found in secret store")
        };
        let Some(AnyKeypair(distributor_authority)) = secret_store
            .get("DISTRIBUTOR_AUTHORITY_KEYPAIR")
            .map(|secret| secret.parse())
            .transpose()
            .context("Can't deserialize DISTRIBUTOR_AUTHORITY_KEYPAIR")?
        else {
            bail!("DISTRIBUTOR_AUTHORITY_KEYPAIR not found in secret store")
        };
        let Some(distributor_state) = secret_store
            .get("DISTRIBUTOR_STATE")
            .map(|secret| secret.parse())
            .transpose()
            .context("Can't deserialize DISTRIBUTOR_STATE")?
        else {
            bail!("DISTRIBUTOR_STATE not found in secret store")
        };
        let Some(program_id) = secret_store
            .get("PROGRAM_ID")
            .map(|secret| secret.parse())
            .transpose()
            .context("Can't deserialize PROGRAM_ID")?
        else {
            bail!("PROGRAM_ID not found in secret store")
        };

        Ok(Self {
            solana_rpc_url,
            payer,
            distributor_authority,
            distributor_state,
            program_id,
        })
    }
}
