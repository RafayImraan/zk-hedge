use std::{env, str::FromStr, time::Duration};

use anyhow::{bail, Context, Result};
use ethers::{
    abi::{decode, ParamType, Token},
    providers::{Http, Middleware, Provider, Ws},
    types::{Address, BlockNumber, Filter, H256, U256, U64},
    utils::keccak256,
};
use futures_util::StreamExt;

use crate::intent::Intent;

pub const ARBITRUM_SEPOLIA_CHAIN_ID: u64 = 421_614;
const SWAP_EXECUTED_SIGNATURE: &str = "SwapExecuted(bytes32,address,uint256)";

/// Subscribe to live SwapExecuted logs over WebSocket. HTTP receipt validation
/// remains the authorization gate; this stream provides immediate detection.
pub async fn subscribe_swap_executed_logs(ws_url: String, escrow_address: Address) -> Result<()> {
    let ws = Ws::connect(ws_url)
        .await
        .context("connect Arbitrum Sepolia WebSocket RPC")?;
    let provider = Provider::new(ws);
    let event_topic = H256::from(keccak256(SWAP_EXECUTED_SIGNATURE.as_bytes()));
    let filter = Filter::new().address(escrow_address).topic0(event_topic);
    let mut stream = provider
        .subscribe_logs(&filter)
        .await
        .context("subscribe to SwapExecuted logs")?;
    while let Some(log) = stream.next().await {
        tracing::info!(transaction_hash = ?log.transaction_hash, intent_hash = ?log.topics.get(1), "observed live SwapExecuted event");
    }
    Ok(())
}

/// Evidence returned only after a successful Arbitrum Sepolia receipt and a
/// matching `SwapExecuted` event have both been verified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayoutAuthorization {
    pub source_chain_id: u64,
    pub source_transaction_hash: H256,
    pub source_block_number: U64,
    pub intent_hash: H256,
    pub recipient: Address,
    pub escrow_amount: U256,
    pub amount_taz: u64,
}

pub struct ProofVerifier {
    provider: Provider<Http>,
    escrow_address: Address,
}

impl ProofVerifier {
    /// Build an HTTP JSON-RPC verifier from environment configuration and
    /// refuse to run against a chain other than Arbitrum Sepolia.
    pub async fn from_env() -> Result<Self> {
        let rpc_url =
            env::var("ARBITRUM_SEPOLIA_RPC_URL").context("ARBITRUM_SEPOLIA_RPC_URL must be set")?;
        let escrow_address = env::var("SHIELDED_SOLVER_ESCROW_ADDRESS")
            .context("SHIELDED_SOLVER_ESCROW_ADDRESS must be set")?
            .parse::<Address>()
            .context("SHIELDED_SOLVER_ESCROW_ADDRESS must be a valid EVM address")?;
        anyhow::ensure!(
            escrow_address != Address::zero(),
            "escrow address must not be zero"
        );

        let provider =
            Provider::<Http>::try_from(rpc_url.as_str())?.interval(Duration::from_millis(1_000));
        let actual_chain_id = provider.get_chainid().await?;
        anyhow::ensure!(
            actual_chain_id == U256::from(ARBITRUM_SEPOLIA_CHAIN_ID),
            "configured RPC is chain {}, expected Arbitrum Sepolia ({})",
            actual_chain_id,
            ARBITRUM_SEPOLIA_CHAIN_ID
        );

        Ok(Self {
            provider,
            escrow_address,
        })
    }

    /// Verify a successful escrow settlement before producing a TAZ payout
    /// authorization. The caller must validate `intent.user_signature` and
    /// ensure `intent_hash` is the hash committed for that signed intent.
    pub async fn verify_swap_and_authorize_taz_payout(
        &self,
        intent: &Intent,
        intent_hash: H256,
        transaction_hash: H256,
    ) -> Result<PayoutAuthorization> {
        intent.validate()?;
        anyhow::ensure!(
            intent.target_chain_id == ARBITRUM_SEPOLIA_CHAIN_ID,
            "intent targets chain {}, expected Arbitrum Sepolia ({})",
            intent.target_chain_id,
            ARBITRUM_SEPOLIA_CHAIN_ID
        );
        anyhow::ensure!(intent_hash != H256::zero(), "intent hash must not be zero");

        let recipient = Address::from_str(&intent.recipient_address)
            .context("recipient_address must be a valid EVM address")?;
        anyhow::ensure!(
            recipient != Address::zero(),
            "recipient_address must not be zero"
        );

        let receipt = self
            .provider
            .get_transaction_receipt(transaction_hash)
            .await?
            .context("transaction is not mined yet")?;
        anyhow::ensure!(
            receipt.transaction_hash == transaction_hash,
            "RPC returned a receipt for a different transaction"
        );
        anyhow::ensure!(
            receipt.status == Some(U64::one()),
            "source transaction did not succeed"
        );
        let block_number = receipt
            .block_number
            .context("receipt has no block number")?;

        let event_topic = H256::from(keccak256(SWAP_EXECUTED_SIGNATURE.as_bytes()));
        let filter = Filter::new()
            .address(self.escrow_address)
            .topic0(event_topic)
            .topic1(intent_hash)
            .from_block(BlockNumber::Number(block_number))
            .to_block(BlockNumber::Number(block_number));
        let logs = self.provider.get_logs(&filter).await?;

        let mut verified_event = None;
        for log in logs {
            if log.transaction_hash != Some(transaction_hash)
                || log.topics.first() != Some(&event_topic)
                || log.topics.get(1) != Some(&intent_hash)
                || log.topics.len() != 3
            {
                continue;
            }

            anyhow::ensure!(
                verified_event.is_none(),
                "multiple matching SwapExecuted events found"
            );
            let decoded = decode(&[ParamType::Uint(256)], log.data.as_ref())
                .context("could not decode SwapExecuted event data")?;
            let event_recipient = Address::from_slice(&log.topics[2].as_bytes()[12..]);
            let escrow_amount = match decoded.as_slice() {
                [Token::Uint(amount)] => *amount,
                _ => bail!("SwapExecuted event has an unexpected ABI payload"),
            };
            anyhow::ensure!(
                event_recipient == recipient,
                "SwapExecuted recipient does not match the intent"
            );
            anyhow::ensure!(
                !escrow_amount.is_zero(),
                "SwapExecuted amount must be greater than zero"
            );
            verified_event = Some(escrow_amount);
        }

        let escrow_amount =
            verified_event.context("successful transaction has no matching SwapExecuted event")?;

        Ok(PayoutAuthorization {
            source_chain_id: ARBITRUM_SEPOLIA_CHAIN_ID,
            source_transaction_hash: transaction_hash,
            source_block_number: block_number,
            intent_hash,
            recipient,
            escrow_amount,
            amount_taz: intent.amount_taz,
        })
    }
}
