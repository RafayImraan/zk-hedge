use serde::{Deserialize, Serialize};
use zcash_protocol::consensus::Network;

/// Zcash network used by this relayer. Keep testnet explicit until a separate
/// production deployment is configured.
pub const ZCASH_NETWORK: Network = Network::TestNetwork;

/// Zcash transaction identifier type used by the future Lightwalletd adapter.
pub type ZcashTransactionId = zcash_primitives::transaction::TxId;

/// A cross-chain payout intent.
///
/// `amount_taz` is expressed in the smallest TAZ denomination as an integer.
/// Viewing keys and signatures are sensitive values and intentionally do not
/// implement `Debug`.
#[derive(Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Intent {
    pub amount_taz: u64,
    pub target_chain_id: u64,
    pub recipient_address: String,
    pub ephemeral_viewing_key: String,
    pub user_signature: String,
}

impl Intent {
    /// Validate required fields before an intent enters the routing pipeline.
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(self.amount_taz > 0, "amount_taz must be greater than zero");
        anyhow::ensure!(
            self.target_chain_id > 0,
            "target_chain_id must be greater than zero"
        );
        anyhow::ensure!(
            !self.recipient_address.trim().is_empty(),
            "recipient_address is required"
        );
        anyhow::ensure!(
            !self.ephemeral_viewing_key.trim().is_empty(),
            "ephemeral_viewing_key is required"
        );
        anyhow::ensure!(
            !self.user_signature.trim().is_empty(),
            "user_signature is required"
        );
        Ok(())
    }
}

/// Parse a JSON intent and reject malformed or incomplete input before routing.
pub fn parse_intent_json(input: &str) -> anyhow::Result<Intent> {
    let intent: Intent = serde_json::from_str(input)?;
    intent.validate()?;
    Ok(intent)
}
