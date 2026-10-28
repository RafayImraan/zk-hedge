//! In-process FROST-Ed25519 threshold-signing demonstration.
//!
//! The dealer and all signers run in this process for a hackathon demo. This
//! validates the FROST protocol flow, but is not distributed key custody.
use anyhow::{Context, Result};
use frost_ed25519 as frost;
use rand_core::OsRng;

/// Generate a 2-of-3 dealer key set, sign a message with two shares, aggregate,
/// and verify the resulting group signature before returning its hex encoding.
pub fn threshold_sign_intent_hash(message: &[u8]) -> Result<String> {
    let mut rng = OsRng;
    let (shares, public_key_package) =
        frost::keys::generate_with_dealer(3, 2, frost::keys::IdentifierList::Default, &mut rng)?;

    let mut key_packages = Vec::new();
    for (identifier, secret_share) in shares {
        key_packages.push(
            frost::keys::KeyPackage::try_from(secret_share).context("invalid FROST key share")?,
        );
        // Preserve the identifiers associated with these packages below.
        let _ = identifier;
    }
    let selected = key_packages.into_iter().take(2).collect::<Vec<_>>();
    let mut commitments = Vec::new();
    let mut nonces = Vec::new();
    for package in &selected {
        let (nonces_for_signer, commitment) =
            frost::round1::commit(package.signing_share(), &mut rng);
        nonces.push(nonces_for_signer);
        commitments.push((package.identifier().clone(), commitment));
    }
    let signing_package =
        frost::SigningPackage::new(commitments.iter().cloned().collect(), message);
    let mut signature_shares = std::collections::BTreeMap::new();
    for (package, signer_nonces) in selected.iter().zip(nonces.iter()) {
        let share = frost::round2::sign(&signing_package, signer_nonces, package)?;
        signature_shares.insert(package.identifier().clone(), share);
    }
    let signature = frost::aggregate(&signing_package, &signature_shares, &public_key_package)?;
    public_key_package
        .verifying_key()
        .verify(message, &signature)
        .context("aggregated FROST signature did not verify")?;
    let serialized = signature
        .serialize()
        .context("could not serialize aggregated FROST signature")?;
    Ok(hex::encode(serialized))
}

#[cfg(test)]
mod tests {
    #[test]
    fn threshold_signature_verifies() {
        let signature =
            super::threshold_sign_intent_hash(&[7u8; 32]).expect("FROST signing succeeds");
        assert_eq!(signature.len(), 128, "Ed25519 signature is 64 bytes");
    }
}
