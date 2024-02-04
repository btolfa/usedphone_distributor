use anchor_client::{Client as AnchorClient, Cluster, Program};
use anyhow::{anyhow, bail, Context};
use axum::{
    extract::{rejection::JsonRejection, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use backend::{settings::Settings, transaction_status::EncodedConfirmedTransactionWithStatusMeta};
use distributor::DistributorState;
use shuttle_secrets::SecretStore;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use solana_transaction_status::{
    option_serializer::OptionSerializer, EncodedTransaction, UiMessage, UiRawMessage, UiTransaction,
};
use std::{str::FromStr, sync::Arc};
use tower::ServiceBuilder;
use tower_http::validate_request::ValidateRequestHeaderLayer;
use tracing::{field::display, Span};

#[tracing::instrument(skip_all, fields(vault_balance))]
async fn handle(
    State(state): State<Arc<AppState>>,
    transactions: Result<Json<Vec<EncodedConfirmedTransactionWithStatusMeta>>, JsonRejection>,
) -> Result<(), StatusCode> {
    let Json(transactions) = transactions.map_err(|err| {
        tracing::warn!(%err, "Failed to parse request body");
        StatusCode::BAD_REQUEST
    })?;

    let Some(vault_balance) = transactions
        .iter()
        .filter_map(|tx| vault_balance(&state.distributor_state.vault, tx).ok())
        .last()
    else {
        tracing::warn!("Payload doesn't contain any transactions with vault balance change");
        return Ok(());
    };

    let span = Span::current();
    span.record("vault_balance", display(vault_balance));

    let threshold = state.distributor_state.share_size * state.distributor_state.number_of_shares;
    if vault_balance >= threshold {
        tracing::info!(%threshold, "threshold reached, distributing");
    } else {
        tracing::info!(%threshold, "isn't threshold reached");
        return Ok(());
    }

    Ok(())
}

struct AppState {
    rpc_client: RpcClient,
    program: Program<Arc<Keypair>>,
    distributor_state: DistributorState,
}

#[shuttle_runtime::main]
async fn axum(#[shuttle_secrets::Secrets] secret_store: SecretStore) -> shuttle_axum::ShuttleAxum {
    let Settings {
        solana_rpc_url,
        payer: payer_keypair,
        distributor_authority: distributor_authority_keypair,
        distributor_state: distributor_state_pubkey,
        program_id,
        auth_token,
    } = Settings::try_from(&secret_store)?;

    let payer = payer_keypair.pubkey();
    let distributor_authority = distributor_authority_keypair.pubkey();

    let rpc_client = RpcClient::new_with_commitment(solana_rpc_url.clone(), CommitmentConfig::confirmed());
    let program = AnchorClient::new_with_options(
        Cluster::Custom(solana_rpc_url.clone(), solana_rpc_url.clone()),
        Arc::new(payer_keypair),
        CommitmentConfig::confirmed(),
    )
    .program(program_id)
    .context("Failed setup anchor client program")?;

    let distributor_state: DistributorState = program
        .account(distributor_state_pubkey)
        .await
        .context("Failed to fetch distributor state")?;
    if distributor_state.distributor_authority != distributor_authority {
        return Err(anyhow!(
            "Distributor authority mismatch: {} vs {}",
            distributor_state.distributor_authority,
            distributor_authority
        )
        .into());
    }

    let vault = distributor_state.vault;

    let router = Router::new()
        .route("/", post(handle))
        .layer(ServiceBuilder::new().layer(ValidateRequestHeaderLayer::bearer(&auth_token)))
        .with_state(Arc::new(AppState {
            rpc_client,
            program,
            distributor_state,
        }));

    tracing::info!(%payer, %distributor_authority,
        %distributor_state_pubkey,
        %vault, %program_id, "Distributor backend setup complete.");

    Ok(router.into())
}

fn vault_balance(vault: &Pubkey, tx: &EncodedConfirmedTransactionWithStatusMeta) -> anyhow::Result<u64> {
    let account_keys = match &tx.transaction.transaction {
        EncodedTransaction::Json(UiTransaction {
            message: UiMessage::Raw(UiRawMessage { account_keys, .. }),
            ..
        }) => account_keys
            .iter()
            .map(|key| Pubkey::from_str(&key))
            .collect::<Result<Vec<_>, _>>()?,
        _ => bail!("Failed to find account keys in transaction"),
    };
    let vault_position = account_keys
        .iter()
        .position(|key| key == vault)
        .ok_or_else(|| anyhow!("Vault not found in transaction"))?;

    let OptionSerializer::Some(post_token_balances) = tx
        .transaction
        .meta
        .as_ref()
        .ok_or_else(|| anyhow!("Transaction doesn't have meta field"))?
        .post_token_balances
        .as_ref()
    else {
        bail!("Transaction doesn't have post_token_balances field");
    };

    let vault_balance = post_token_balances
        .iter()
        .find_map(|token_balance| {
            (token_balance.account_index as usize == vault_position)
                .then_some(token_balance.ui_token_amount.amount.as_str())
        })
        .map(|amount| amount.parse::<u64>())
        .transpose()
        .context("Failed to parse vault balance")?
        .ok_or_else(|| anyhow!("Failed to find vault balance"))?;

    Ok(vault_balance)
}

#[cfg(test)]
mod tests {
    use crate::vault_balance;
    use backend::transaction_status::EncodedConfirmedTransactionWithStatusMeta;
    use solana_sdk::pubkey;

    #[test]
    fn should_deser_transfer() -> anyhow::Result<()> {
        let json = include_bytes!("transfer.json");
        let _: Vec<EncodedConfirmedTransactionWithStatusMeta> = serde_json::from_slice(json)?;
        Ok(())
    }

    #[test]
    fn should_deser_deposit() -> anyhow::Result<()> {
        let json = include_bytes!("deposit.json");
        let _: Vec<EncodedConfirmedTransactionWithStatusMeta> = serde_json::from_slice(json)?;
        Ok(())
    }

    #[test]
    fn should_find_vault_post_balance() -> anyhow::Result<()> {
        let json = include_bytes!("transfer.json");
        let txs: Vec<EncodedConfirmedTransactionWithStatusMeta> = serde_json::from_slice(json)?;
        let vault = pubkey!("4wZ2E3St33iB5xu9R2Kf6NbMa5pkoeqVNe1SkcFVvoX5");
        let vault_balance = vault_balance(&vault, &txs[0])?;
        assert_eq!(5000000000, vault_balance);

        let json = include_bytes!("deposit.json");
        let txs: Vec<EncodedConfirmedTransactionWithStatusMeta> = serde_json::from_slice(json)?;
        let vault = pubkey!("4wZ2E3St33iB5xu9R2Kf6NbMa5pkoeqVNe1SkcFVvoX5");
        let vault_balance = crate::vault_balance(&vault, &txs[0])?;
        assert_eq!(4000000000, vault_balance);

        Ok(())
    }
}
