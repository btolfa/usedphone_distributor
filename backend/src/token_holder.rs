use anyhow::bail;
use itertools::Itertools;
use jsonrpsee::{
    http_client::{HttpClient, HttpClientBuilder},
    proc_macros::rpc,
};
use rand::distributions::{Distribution, Uniform};
use serde::Deserialize;
use serde_with::{serde_as, DisplayFromStr, FromInto};
use solana_sdk::pubkey::Pubkey;

#[serde_as]
#[derive(Deserialize)]
struct GetTokenAccountsResponse {
    total: u64,
    #[serde_as(as = "Vec<FromInto<TokenAccount>>")]
    token_accounts: Vec<Pubkey>,
}

#[serde_as]
#[derive(Deserialize)]
struct TokenAccount {
    // address: Pubkey
    // mint: Pubkey,
    // amount: u64,
    // delegated_amount: u64,
    // frozen: false,
    #[serde_as(as = "DisplayFromStr")]
    owner: Pubkey,
}

impl From<TokenAccount> for Pubkey {
    fn from(account: TokenAccount) -> Self {
        account.owner
    }
}

#[rpc(client)]
trait HeliusGetTokenAccounts {
    #[method(name = "getTokenAccounts", param_kind = map)]
    async fn get_token_accounts(&self, mint: &str, page: u64, limit: u64) -> RpcResult<GetTokenAccountsResponse>;
}

pub struct HeliusClient {
    client: HttpClient,
    mint: Pubkey,
    holders_number: u64,
}

impl HeliusClient {
    pub fn new(url: impl AsRef<str>, mint: Pubkey) -> anyhow::Result<Self> {
        let client = HttpClientBuilder::default().build(url)?;
        Ok(Self {
            client,
            mint,
            holders_number: 0,
        })
    }

    pub async fn update_token_holders_number(&mut self) -> anyhow::Result<()> {
        self.holders_number = self.discover_token_holders_number().await?;
        Ok(())
    }

    pub async fn discover_token_holders_number(&self) -> anyhow::Result<u64> {
        let limit = 1000;

        for page in (self.holders_number / limit + 1)..2000 {
            let GetTokenAccountsResponse { total, .. } = self
                .client
                .get_token_accounts(&self.mint.to_string(), page, limit)
                .await?;
            if total < limit {
                return Ok(limit * (page - 1) + total);
            }
        }
        bail!("There is more than 2000 pages of token accounts");
    }

    pub async fn draw_winners(&self, n: u64) -> anyhow::Result<Vec<Pubkey>> {
        let winner_idx = {
            let mut rng = rand::thread_rng();
            let distr = Uniform::from(0..self.holders_number);

            let mut winner_idx: Vec<_> = distr.sample_iter(&mut rng).take(n as usize).collect();
            winner_idx.sort_unstable();
            winner_idx
        };

        let limit = 1000;
        let mut winners = Vec::with_capacity(n as usize);
        let distribution: Vec<_> = winner_idx
            .into_iter()
            .group_by(|idx| *idx / limit + 1)
            .into_iter()
            .map(|(page, idxs)| (page, idxs.collect::<Vec<_>>()))
            .collect();

        for (page, idxs) in distribution {
            let GetTokenAccountsResponse { token_accounts, .. } = self
                .client
                .get_token_accounts(&self.mint.to_string(), page, limit)
                .await?;
            winners.extend(idxs.into_iter().map(|idx| token_accounts[(idx % limit) as usize]));
        }
        Ok(winners)
    }

    pub fn holders_number(&self) -> u64 {
        self.holders_number
    }
}

#[cfg(test)]
mod tests {
    use crate::token_holder::HeliusClient;
    use dotenvy::dotenv;
    use solana_sdk::pubkey;

    #[tokio::test]
    async fn should_discover_token_holders_number() -> anyhow::Result<()> {
        dotenv().ok();
        let solana_rpc_url = std::env::var("SOLANA_RPC_URL")?;

        let client = HeliusClient::new(solana_rpc_url, pubkey!("7GCihgDB8fe6KNjn2MYtkzZcRjQy3t9GHdC8uHYmW2hr"))?;
        let holders_number = client.discover_token_holders_number().await?;
        println!("{}", holders_number);

        Ok(())
    }

    #[tokio::test]
    async fn should_select_random_holders() -> anyhow::Result<()> {
        dotenv().ok();
        let solana_rpc_url = std::env::var("SOLANA_RPC_URL")?;

        let mut client = HeliusClient::new(solana_rpc_url, pubkey!("7GCihgDB8fe6KNjn2MYtkzZcRjQy3t9GHdC8uHYmW2hr"))?;
        client.update_token_holders_number().await?;

        let winners = client.draw_winners(10).await?;
        println!("{:?}", winners);

        Ok(())
    }
}
