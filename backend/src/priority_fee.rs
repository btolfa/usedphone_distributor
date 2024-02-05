use jsonrpsee::{http_client::HttpClient, proc_macros::rpc};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use solana_sdk::pubkey::Pubkey;

#[serde_as]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetPriorityFeeEstimateRequest {
    #[serde_as(as = "Vec<DisplayFromStr>")]
    account_keys: Vec<Pubkey>, // estimate fee for a list of accounts
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetPriorityFeeEstimateResponse {
    priority_fee_estimate: f64,
}

#[rpc(client)]
trait PriorityFeeApi {
    #[method(name = "getPriorityFeeEstimate")]
    async fn get_priority_fee_estimate(
        &self,
        request: GetPriorityFeeEstimateRequest,
    ) -> Result<GetPriorityFeeEstimateResponse, ErrorObjectOwned>;
}

pub async fn fetch_recent_priority_fee(client: &HttpClient) -> anyhow::Result<u64> {
    let GetPriorityFeeEstimateResponse { priority_fee_estimate } = client
        .get_priority_fee_estimate(GetPriorityFeeEstimateRequest {
            account_keys: vec![distributor::ID],
        })
        .await?;
    Ok(priority_fee_estimate as u64)
}
