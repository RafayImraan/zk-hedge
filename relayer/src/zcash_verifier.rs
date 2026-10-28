use std::{collections::HashMap, env, io::Cursor};

use anyhow::{bail, Context, Result};
use zcash_client_backend::{decrypt_transaction, proto::service::{compact_tx_streamer_client::CompactTxStreamerClient, ChainSpec, Empty, TxFilter}};
use zcash_keys::keys::UnifiedFullViewingKey;
use zcash_primitives::transaction::Transaction;
use zcash_protocol::consensus::{self, BlockHeight, BranchId};

/// Fetch a transaction from Testnet Lightwalletd, require it to be mined, and
/// trial-decrypt its Sapling/Orchard outputs with the supplied view-only UFVK.
/// The relayer never receives or handles spending keys.
pub async fn verify_shielded_payment(txid_hex: &str, ufvk_text: &str, required_zatoshi: u64) -> Result<u64> {
    anyhow::ensure!(required_zatoshi > 0, "required ZEC amount must be positive");
    let ufvk = UnifiedFullViewingKey::decode(&consensus::Network::TestNetwork, ufvk_text)
        .context("viewing key is not a valid Zcash Testnet unified full viewing key")?;
    let endpoint = env::var("ZCASH_LIGHTWALLETD_URL")
        .context("ZCASH_LIGHTWALLETD_URL must point to a Testnet Lightwalletd gRPC endpoint")?;
    let mut client = CompactTxStreamerClient::connect(endpoint).await.context("connect to Zcash Lightwalletd")?;
    let info = client.get_lightd_info(Empty {}).await?.into_inner();
    anyhow::ensure!(info.chain_name == "test", "Lightwalletd is on {:?}, expected Zcash Testnet", info.chain_name);
    let latest = client.get_latest_block(ChainSpec {}).await?.into_inner();
    let txid = decode_txid(txid_hex)?;
    let raw = client.get_transaction(TxFilter { block: None, index: 0, hash: txid }).await?.into_inner();
    anyhow::ensure!(raw.height > 0 && raw.height != u64::MAX, "Zcash transaction is not mined in the active chain");
    let mined_height = BlockHeight::from_u32(u32::try_from(raw.height).context("invalid Zcash block height")?);
    let confirmations = latest.height.saturating_sub(raw.height).saturating_add(1);
    let required_confirmations = env::var("ZCASH_REQUIRED_CONFIRMATIONS").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(1);
    anyhow::ensure!(confirmations >= required_confirmations, "Zcash transaction has {confirmations} confirmations; {required_confirmations} required");
    let branch = BranchId::for_height(&consensus::Network::TestNetwork, mined_height);
    let tx = Transaction::read(Cursor::new(raw.data), branch).context("parse Zcash transaction")?;
    let mut keys = HashMap::new();
    keys.insert(0u32, ufvk);
    let decrypted = decrypt_transaction(&consensus::Network::TestNetwork, Some(mined_height), Some(BlockHeight::from_u32(u32::try_from(latest.height).unwrap_or(u32::MAX))), &tx, &keys);
    let mut received = 0u64;
    for output in decrypted.sapling_outputs() { received = received.saturating_add(output.note_value().into_u64()); }
    for output in decrypted.orchard_outputs() { received = received.saturating_add(output.note().0.value().inner()); }
    if received < required_zatoshi { bail!("transaction paid {received} zatoshi to the supplied viewing key; {required_zatoshi} required"); }
    Ok(received)
}

fn decode_txid(value: &str) -> Result<Vec<u8>> {
    let text = value.strip_prefix("0x").unwrap_or(value);
    let bytes = hex::decode(text).context("Zcash txid must be 64 hex characters")?;
    anyhow::ensure!(bytes.len() == 32, "Zcash txid must be 32 bytes");
    Ok(bytes.into_iter().rev().collect())
}
