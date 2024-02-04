use anchor_client::{Client as AnchorClient, Cluster, Program};
use anyhow::Context;
use axum::{routing::get, Router};
use backend::settings::Settings;
use shuttle_secrets::SecretStore;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    signature::{Keypair, Signer},
};
use std::sync::Arc;

async fn hello_world() -> &'static str {
    "Hello, world!"
}

struct AppState {
    rpc_client: RpcClient,
    program: Program<Arc<Keypair>>,
}

#[shuttle_runtime::main]
async fn axum(#[shuttle_secrets::Secrets] secret_store: SecretStore) -> shuttle_axum::ShuttleAxum {
    let Settings {
        solana_rpc_url,
        payer: payer_keypair,
        distributor_authority: distributor_authority_keypair,
        distributor_state,
        vault,
        program_id,
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

    let router = Router::new()
        .route("/", get(hello_world))
        .with_state(Arc::new(AppState { rpc_client, program }));

    tracing::info!(%payer, %distributor_authority, %distributor_state, %vault, %program_id, "Distributor backend setup complete.");

    Ok(router.into())
}
