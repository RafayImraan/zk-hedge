use std::{collections::HashMap, env, io::Cursor};

use anyhow::{bail, Context, Result};
use zcash_client_backend::{
    decrypt_transaction,
    proto::service::{
        compact_tx_streamer_client::CompactTxStreamerClient, ChainSpec, Empty, TxFilter,
    },
};
use zcash_keys::address::UnifiedAddress;
use zcash_keys::keys::UnifiedFullViewingKey;
use zcash_primitives::transaction::Transaction;
use zcash_protocol::consensus::{self, BlockHeight, BranchId};

/// Fetch a transaction from Testnet Lightwalletd, require it to be mined, and
/// trial-decrypt its Sapling/Orchard/Ironwood outputs with the supplied view-only UFVK.
/// The relayer never receives or handles spending keys.
pub async fn verify_shielded_payment(
    txid_hex: &str,
    ufvk_text: &str,
    required_zatoshi: u64,
    expected_intent_hash: &str,
    expected_zcash_recipient: &str,
) -> Result<u64> {
    anyhow::ensure!(required_zatoshi > 0, "required ZEC amount must be positive");
    let expected_memo = intent_memo(expected_intent_hash)?;
    let ufvk = UnifiedFullViewingKey::decode(&consensus::Network::TestNetwork, ufvk_text)
        .context("viewing key is not a valid Zcash Testnet unified full viewing key")?;
    let recipient =
        UnifiedAddress::decode(&consensus::Network::TestNetwork, expected_zcash_recipient)
            .context("recipient must be a valid Zcash Testnet Unified Address")?;
    let endpoint = env::var("ZCASH_LIGHTWALLETD_URL")
        .context("ZCASH_LIGHTWALLETD_URL must point to a Testnet Lightwalletd gRPC endpoint")?;
    let mut client = CompactTxStreamerClient::connect(endpoint)
        .await
        .context("connect to Zcash Lightwalletd")?;
    let info = client.get_lightd_info(Empty {}).await?.into_inner();
    anyhow::ensure!(
        info.chain_name == "test",
        "Lightwalletd is on {:?}, expected Zcash Testnet",
        info.chain_name
    );
    let latest = client.get_latest_block(ChainSpec {}).await?.into_inner();
    let txid = decode_txid(txid_hex)?;
    let raw = client
        .get_transaction(TxFilter {
            block: None,
            index: 0,
            hash: txid,
        })
        .await?
        .into_inner();
    anyhow::ensure!(
        raw.height > 0 && raw.height != u64::MAX,
        "Zcash transaction is not mined in the active chain"
    );
    let mined_height =
        BlockHeight::from_u32(u32::try_from(raw.height).context("invalid Zcash block height")?);
    let confirmations = latest.height.saturating_sub(raw.height).saturating_add(1);
    let required_confirmations = env::var("ZCASH_REQUIRED_CONFIRMATIONS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(1);
    anyhow::ensure!(
        confirmations >= required_confirmations,
        "Zcash transaction has {confirmations} confirmations; {required_confirmations} required"
    );
    let branch = BranchId::for_height(&consensus::Network::TestNetwork, mined_height);
    let tx = Transaction::read(Cursor::new(raw.data), branch).context("parse Zcash transaction")?;
    let mut keys = HashMap::new();
    keys.insert(0u32, ufvk);
    let decrypted = decrypt_transaction(
        &consensus::Network::TestNetwork,
        Some(mined_height),
        Some(BlockHeight::from_u32(
            u32::try_from(latest.height).unwrap_or(u32::MAX),
        )),
        &tx,
        &keys,
    );
    let mut received = 0u64;
    for output in decrypted.sapling_outputs() {
        if recipient.sapling() == Some(&output.note().recipient()) {
            validate_intent_memo(output.memo().as_slice(), &expected_memo)?;
            received = received.saturating_add(output.note_value().into_u64());
        }
    }
    for output in decrypted.orchard_outputs() {
        if recipient.orchard() == Some(&output.note().0.recipient()) {
            validate_intent_memo(output.memo().as_slice(), &expected_memo)?;
            received = received.saturating_add(output.note().0.value().inner());
        }
    }
    // NU6.3 moved new shielded activity to Ironwood. The Lightwalletd decoder
    // returns Ironwood notes through the Orchard protocol's v2 note type, so
    // check the same unified receiver and intent-bound memo there as well.
    for output in decrypted.ironwood_outputs() {
        if recipient.orchard() == Some(&output.note().0.recipient()) {
            validate_intent_memo(output.memo().as_slice(), &expected_memo)?;
            received = received.saturating_add(output.note().0.value().inner());
        }
    }
    if received < required_zatoshi {
        bail!("transaction paid {received} zatoshi to the supplied viewing key; {required_zatoshi} required");
    }
    Ok(received)
}

/// The sender must include this exact UTF-8 memo on each qualifying shielded
/// output. Zcash note encryption authenticates the memo as part of the output.
fn intent_memo(intent_hash: &str) -> Result<String> {
    let hash = intent_hash.strip_prefix("0x").unwrap_or(intent_hash);
    let decoded = hex::decode(hash).context("intent hash must be 32-byte hexadecimal")?;
    anyhow::ensure!(decoded.len() == 32, "intent hash must be 32 bytes");
    Ok(format!("ZK-HEDGE:v1:0x{}", hex::encode(decoded)))
}

fn validate_intent_memo(memo_bytes: &[u8], expected_memo: &str) -> Result<()> {
    let memo = std::str::from_utf8(memo_bytes).context("shielded output memo is not UTF-8")?;
    anyhow::ensure!(
        memo == expected_memo,
        "shielded output memo does not match the signed ZK-Hedge intent hash"
    );
    Ok(())
}

fn decode_txid(value: &str) -> Result<Vec<u8>> {
    let text = value.strip_prefix("0x").unwrap_or(value);
    let bytes = hex::decode(text).context("Zcash txid must be 64 hex characters")?;
    anyhow::ensure!(bytes.len() == 32, "Zcash txid must be 32 bytes");
    Ok(bytes.into_iter().rev().collect())
}

#[cfg(test)]
mod tests {
    use super::{intent_memo, validate_intent_memo};
    use zcash_protocol::memo::MemoBytes;

    const INTENT_HASH: &str = "0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";

    #[test]
    fn accepts_exact_intent_memo() {
        let expected = intent_memo(INTENT_HASH).unwrap();
        let memo = MemoBytes::from_bytes(expected.as_bytes()).unwrap();
        assert!(validate_intent_memo(memo.as_slice(), &expected).is_ok());
    }

    #[test]
    fn rejects_memo_for_another_intent() {
        let expected = intent_memo(INTENT_HASH).unwrap();
        let other = MemoBytes::from_bytes(
            b"ZK-HEDGE:v1:0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        )
        .unwrap();
        assert!(validate_intent_memo(other.as_slice(), &expected).is_err());
    }

    #[test]
    fn rejects_missing_or_non_utf8_memo() {
        let expected = intent_memo(INTENT_HASH).unwrap();
        let empty = MemoBytes::empty();
        assert!(validate_intent_memo(empty.as_slice(), &expected).is_err());
        assert!(validate_intent_memo(&[0xff, 0xfe], &expected).is_err());
    }

    #[test]
    fn rejects_malformed_intent_hash() {
        assert!(intent_memo("0x1234").is_err());
    }
}
