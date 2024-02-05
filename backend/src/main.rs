use anchor_client::{Client as AnchorClient, Cluster, Program};
use anyhow::{anyhow, bail, Context};
use axum::{
    extract::{rejection::JsonRejection, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use backend::{
    priority_fee::fetch_recent_priority_fee, settings::Settings, token_holder::HeliusClient,
    transaction_status::EncodedConfirmedTransactionWithStatusMeta,
};
use distributor::DistributorState;
use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};
use shuttle_secrets::SecretStore;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    compute_budget::ComputeBudgetInstruction,
    instruction::AccountMeta,
    program_pack::Pack,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use solana_transaction_status::{
    option_serializer::OptionSerializer, EncodedTransaction, UiMessage, UiRawMessage, UiTransaction,
};
use spl_associated_token_account::get_associated_token_address;
use spl_token::state::Account as TokenAccount;
use std::{str::FromStr, sync::Arc};
use tokio::sync::Mutex;
use tower::ServiceBuilder;
use tower_http::validate_request::ValidateRequestHeaderLayer;
use tracing::{field::display, Span};

#[tracing::instrument(skip_all)]
async fn webhook_handle(
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

    distribute_tokens(state.as_ref(), vault_balance).await.map_err(|err| {
        tracing::warn!(%err, "Failed to distribute tokens");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(())
}

#[tracing::instrument(skip_all)]
async fn explicit_handle(State(state): State<Arc<AppState>>) -> Result<(), StatusCode> {
    let rpc_client = state.program.async_rpc();
    let data = rpc_client
        .get_account_data(&state.distributor_state.vault)
        .await
        .map_err(|err| {
            tracing::warn!(%err, "Failed to fetch vault balance");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let vault_account = TokenAccount::unpack(&data).map_err(|err| {
        tracing::warn!(%err, "Failed to unpack vault account");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    distribute_tokens(state.as_ref(), vault_account.amount)
        .await
        .map_err(|err| {
            tracing::warn!(%err, "Failed to distribute tokens");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(())
}

#[tracing::instrument(skip_all, fields(vault_balance))]
async fn distribute_tokens(state: &AppState, vault_balance: u64) -> anyhow::Result<()> {
    let span = Span::current();
    span.record("vault_balance", display(vault_balance));

    let threshold = state.distributor_state.share_size * state.distributor_state.number_of_shares;
    if vault_balance >= threshold {
        tracing::info!(%threshold, "Threshold reached, distributing");
    } else {
        tracing::info!(%threshold, "Threshold isn't reached");
        return Ok(());
    }

    let mut helius_client = state.helius_client.lock().await;
    helius_client
        .update_token_holders_number()
        .await
        .context("Failed to update token holders number")?;

    tracing::info!(holders = %helius_client.holders_number(), "Updated token holders number");

    let winners = helius_client
        .draw_winners(state.distributor_state.number_of_shares - 1)
        .await
        .context("Failed to draw winners")?;
    drop(helius_client);
    tracing::info!(?winners, "Winners has been selected");

    let remaining_accounts = winners
        .into_iter()
        .flat_map(|winner| {
            let ata = get_associated_token_address(&winner, &state.distributor_state.mint);
            [AccountMeta::new_readonly(winner, false), AccountMeta::new(ata, false)]
        })
        .collect::<Vec<_>>();

    let rpc_client = state.program.async_rpc();
    let latest_hash = rpc_client
        .get_latest_blockhash()
        .await
        .context("Failed to get latest blockhash")?;
    let priority_fee = fetch_recent_priority_fee(&state.priority_fee)
        .await
        .context("Failed to fetch recent priority fee")?;

    let ixns = state
        .program
        .request()
        .instruction(ComputeBudgetInstruction::set_compute_unit_limit(800_000))
        .instruction(ComputeBudgetInstruction::set_compute_unit_price(priority_fee))
        .instruction(spl_memo::build_memo(state.memo.as_bytes(), &[]))
        .accounts(distributor::accounts::Distribute {
            payer: state.payer.pubkey(),
            distributor_authority: state.distributor_authority.pubkey(),
            distributor_state: state.distributor_state_pubkey,
            mint: state.distributor_state.mint,
            vault: state.distributor_state.vault,
            system_program: solana_sdk::system_program::ID,
            token_program: spl_token::ID,
            associated_token_program: spl_associated_token_account::ID,
        })
        .accounts(remaining_accounts)
        .args(distributor::instruction::Distribute)
        .instructions()
        .context("Failed to create distribute instructions")?;

    let tx = Transaction::new_signed_with_payer(
        &ixns,
        Some(&state.payer.pubkey()),
        &[&state.payer, &state.distributor_authority],
        latest_hash,
    );

    let tx_size = bincode::serialize(&tx).unwrap_or_default().len();
    tracing::info!(%tx_size, "Distribute transaction size. Maximum possible is 1232 bytes.");

    let signature = rpc_client
        .send_transaction(&tx)
        .await
        .context("Failed to send transaction")?;

    tracing::info!(%signature, "Distribute transaction sent");

    Ok(())
}

struct AppState {
    program: Program<Arc<Keypair>>,
    distributor_state_pubkey: Pubkey,
    distributor_state: DistributorState,
    helius_client: Mutex<HeliusClient>,
    priority_fee: HttpClient,
    payer: Keypair,
    distributor_authority: Keypair,
    memo: String,
}

#[shuttle_runtime::main]
async fn axum(#[shuttle_secrets::Secrets] secret_store: SecretStore) -> shuttle_axum::ShuttleAxum {
    let Settings {
        solana_rpc_url,
        priority_fee_url,
        payer: payer_keypair,
        distributor_authority: distributor_authority_keypair,
        distributor_state: distributor_state_pubkey,
        program_id,
        auth_token,
        memo,
        marker_mint,
    } = Settings::try_from(&secret_store)?;

    let payer = payer_keypair.pubkey();
    let distributor_authority = distributor_authority_keypair.pubkey();

    let program = AnchorClient::new_with_options(
        Cluster::Custom(solana_rpc_url.clone(), solana_rpc_url.clone()),
        Arc::new(Keypair::new()),
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

    let mut helius_client = HeliusClient::new(solana_rpc_url, marker_mint).context("Failed to create Helius client")?;
    helius_client
        .update_token_holders_number()
        .await
        .context("Failed to discover marker token holders number")?;

    let priority_fee = HttpClientBuilder::default()
        .build(priority_fee_url)
        .context("Failed to build priority fee client")?;

    let vault = distributor_state.vault;

    let router = Router::new()
        .route("/", post(webhook_handle))
        .route("/distibute", get(explicit_handle))
        .layer(ServiceBuilder::new().layer(ValidateRequestHeaderLayer::bearer(&auth_token)))
        .with_state(Arc::new(AppState {
            program,
            distributor_state,
            helius_client: Mutex::new(helius_client),
            payer: payer_keypair,
            distributor_authority: distributor_authority_keypair,
            distributor_state_pubkey,
            priority_fee,
            memo,
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
            .map(|key| Pubkey::from_str(key))
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
