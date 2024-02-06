use crate::{
    priority_fee::fetch_recent_priority_fee, token_holder::HeliusClient,
    transaction_status::EncodedConfirmedTransactionWithStatusMeta,
};
use anchor_client::{
    anchor_lang::prelude::{AccountMeta, Pubkey},
    Program,
};
use anyhow::{anyhow, bail, Context};
use distributor::DistributorState;
use jsonrpsee::http_client::HttpClient;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction,
    program_pack::Pack,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use solana_transaction_status::{
    option_serializer::OptionSerializer, EncodedTransaction, UiMessage, UiRawMessage, UiTransaction,
};
use spl_associated_token_account::get_associated_token_address;
use spl_token::state::Account as TokenAccount;
use std::{str::FromStr, sync::Arc};
use tokio::sync::{
    mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    Mutex,
};

pub struct AppState {
    pub program: Program<Arc<Keypair>>,
    pub distributor_state_pubkey: Pubkey,
    pub distributor_state: DistributorState,
    pub helius_client: Mutex<HeliusClient>,
    pub priority_fee: HttpClient,
    pub payer: Keypair,
    pub distributor_authority: Keypair,
    pub memo: String,
}

struct Actor {
    receiver: UnboundedReceiver<ActorMessage>,
    state: AppState,
}

struct ActorMessage(Option<EncodedConfirmedTransactionWithStatusMeta>);

impl Actor {
    pub fn new(receiver: UnboundedReceiver<ActorMessage>, state: AppState) -> Self {
        Self { receiver, state }
    }

    pub async fn handle_message(&self, _: Option<EncodedConfirmedTransactionWithStatusMeta>) -> anyhow::Result<()> {
        let rpc_client = self.state.program.async_rpc();

        let data = rpc_client
            .get_account_data(&self.state.distributor_state.vault)
            .await
            .context("Failed to fetch vault balance")?;

        let vault_account = TokenAccount::unpack(&data).context("Failed to unpack vault account")?;

        self.distribute_tokens(vault_account.amount)
            .await
            .context("Failed to distribute tokens")?;

        Ok(())
    }

    #[tracing::instrument(skip(self))]
    async fn distribute_tokens(&self, vault_balance: u64) -> anyhow::Result<()> {
        let threshold = self.state.distributor_state.share_size * self.state.distributor_state.number_of_shares;
        if vault_balance >= threshold {
            tracing::info!(%threshold, "Threshold reached, distributing");
        } else {
            tracing::info!(%threshold, "Threshold isn't reached");
            return Ok(());
        }

        let mut helius_client = self.state.helius_client.lock().await;
        helius_client
            .update_token_holders_number()
            .await
            .context("Failed to update token holders number")?;

        tracing::info!(holders = %helius_client.holders_number(), "Updated token holders number");

        let winners = helius_client
            .draw_winners(self.state.distributor_state.number_of_shares - 1)
            .await
            .context("Failed to draw winners")?;
        drop(helius_client);
        tracing::info!(?winners, "Winners has been selected");

        let remaining_accounts = winners
            .into_iter()
            .flat_map(|winner| {
                let ata = get_associated_token_address(&winner, &self.state.distributor_state.mint);
                [AccountMeta::new_readonly(winner, false), AccountMeta::new(ata, false)]
            })
            .collect::<Vec<_>>();

        let rpc_client = self.state.program.async_rpc();
        let latest_hash = rpc_client
            .get_latest_blockhash()
            .await
            .context("Failed to get latest blockhash")?;

        let ixns = self
            .state
            .program
            .request()
            .instruction(ComputeBudgetInstruction::set_compute_unit_limit(800_000))
            .instruction(spl_memo::build_memo(self.state.memo.as_bytes(), &[]))
            .accounts(distributor::accounts::Distribute {
                payer: self.state.payer.pubkey(),
                distributor_authority: self.state.distributor_authority.pubkey(),
                distributor_state: self.state.distributor_state_pubkey,
                mint: self.state.distributor_state.mint,
                vault: self.state.distributor_state.vault,
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
            Some(&self.state.payer.pubkey()),
            &[&self.state.payer, &self.state.distributor_authority],
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
}

async fn run_actor(mut actor: Actor) {
    while let Some(ActorMessage(tx)) = actor.receiver.recv().await {
        match actor.handle_message(tx).await {
            Ok(_) => {},
            Err(err) => {
                tracing::warn!(%err, "Failed to handle message");
            },
        }
    }
}

#[derive(Clone)]
pub struct ActorHandle {
    sender: UnboundedSender<ActorMessage>,
}

impl ActorHandle {
    pub fn new(state: AppState) -> Self {
        let (sender, receiver) = unbounded_channel();
        let actor = Actor::new(receiver, state);
        tokio::spawn(run_actor(actor));
        Self { sender }
    }

    pub fn handle_request(&self, tx: Option<EncodedConfirmedTransactionWithStatusMeta>) {
        self.sender.send(ActorMessage(tx)).expect("Actor is dead");
    }
}

fn extract_vault_balance(vault: &Pubkey, tx: &EncodedConfirmedTransactionWithStatusMeta) -> anyhow::Result<u64> {
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
    use crate::{service::extract_vault_balance, transaction_status::EncodedConfirmedTransactionWithStatusMeta};
    use solana_sdk::pubkey;

    #[test]
    fn should_find_vault_post_balance() -> anyhow::Result<()> {
        let json = include_bytes!("transfer.json");
        let txs: Vec<EncodedConfirmedTransactionWithStatusMeta> = serde_json::from_slice(json)?;
        let vault = pubkey!("4wZ2E3St33iB5xu9R2Kf6NbMa5pkoeqVNe1SkcFVvoX5");
        let balance = extract_vault_balance(&vault, &txs[0])?;
        assert_eq!(5000000000, balance);

        let json = include_bytes!("deposit.json");
        let txs: Vec<EncodedConfirmedTransactionWithStatusMeta> = serde_json::from_slice(json)?;
        let vault = pubkey!("4wZ2E3St33iB5xu9R2Kf6NbMa5pkoeqVNe1SkcFVvoX5");
        let balance = extract_vault_balance(&vault, &txs[0])?;
        assert_eq!(4000000000, balance);

        Ok(())
    }
}
