use sha2::{Digest, Sha256};

/// Computes the commitment hash used to index transparent UTXOs.
///
/// The hash binds the outpoint alongside its amount and script, allowing
/// callers to prove that an input being spent matches the Merkle leaf used
/// during shielded verification.
pub fn transparent_utxo_commitment(
    txid: &[u8; 32],
    index: u32,
    value_zat: u64,
    script_pubkey: &[u8],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(txid);
    hasher.update(index.to_le_bytes());
    hasher.update(value_zat.to_le_bytes());
    hasher.update(script_pubkey);
    hasher.finalize().into()
}
