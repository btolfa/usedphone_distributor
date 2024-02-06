use anchor_client::{Client as AnchorClient, Cluster};
use anyhow::{anyhow, Context};
use axum::{
    extract::{rejection::JsonRejection, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use backend::{
    service::{ActorHandle, AppState},
    settings::Settings,
    token_holder::HeliusClient,
    transaction_status::EncodedConfirmedTransactionWithStatusMeta,
};
use distributor::DistributorState;
use jsonrpsee::http_client::HttpClientBuilder;
use shuttle_secrets::SecretStore;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    signature::{Keypair, Signer},
};

use std::sync::Arc;
use tokio::sync::Mutex;
use tower::ServiceBuilder;
use tower_http::validate_request::ValidateRequestHeaderLayer;

#[tracing::instrument(skip_all)]
async fn webhook_handle(
    State(handle): State<ActorHandle>,
    transactions: Result<Json<Vec<EncodedConfirmedTransactionWithStatusMeta>>, JsonRejection>,
) -> Result<(), StatusCode> {
    let Json(transactions) = transactions.map_err(|err| {
        tracing::warn!(%err, "Failed to parse request body");
        StatusCode::BAD_REQUEST
    })?;

    for tx in transactions {
        handle.handle_request(Some(tx));
    }

    Ok(())
}

#[tracing::instrument(skip_all)]
async fn explicit_handle(State(handle): State<ActorHandle>) -> Result<(), StatusCode> {
    handle.handle_request(None);

    Ok(())
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

    let state = AppState {
        program,
        distributor_state,
        helius_client: Mutex::new(helius_client),
        payer: payer_keypair,
        distributor_authority: distributor_authority_keypair,
        distributor_state_pubkey,
        priority_fee,
        memo,
    };

    let handle = ActorHandle::new(state);

    let router = Router::new()
        .route("/", post(webhook_handle))
        .route("/distibute", get(explicit_handle))
        .layer(ServiceBuilder::new().layer(ValidateRequestHeaderLayer::bearer(&auth_token)))
        .with_state(handle);

    tracing::info!(%payer, %distributor_authority,
        %distributor_state_pubkey,
        %vault, %program_id, "Distributor backend setup complete.");

    Ok(router.into())
}
