use eyre::{Result, eyre};
use std::path::PathBuf;
use zfun_lib::{
    ExclusionProof, InclusionProof, SnapshotMetadata, load_snapshot_metadata,
    proof::{generate_exclusion_proof_from_files, generate_inclusion_proof_from_files},
    transparent::transparent_utxo_commitment,
};

use crate::AppError;

pub struct ProofService {
    data_dir: PathBuf,
    snapshot_dir: PathBuf,
    snapshot_metadata: SnapshotMetadata,
}

impl ProofService {
    pub fn new(data_dir: PathBuf, snapshot_dir: PathBuf) -> Result<Self, AppError> {
        tracing::info!("Using snapshot directory: {}", snapshot_dir.display());

        // Load snapshot metadata from snapshot directory
        let snapshot_metadata = load_snapshot_metadata(&snapshot_dir).map_err(|e| {
            AppError::internal(eyre!(
                "Failed to load snapshot metadata from {}: {}",
                snapshot_dir.display(),
                e
            ))
        })?;

        tracing::info!(
            "Loaded snapshot metadata: height={}, nullifier_root={}, utxo_root={}",
            snapshot_metadata.height,
            hex::encode(snapshot_metadata.nullifier_root),
            hex::encode(snapshot_metadata.utxo_root)
        );

        Ok(Self {
            data_dir,
            snapshot_dir,
            snapshot_metadata,
        })
    }

    /// Get snapshot directory
    fn get_snapshot_dir(&self) -> &PathBuf {
        &self.snapshot_dir
    }

    /// Generate UTXO inclusion proof
    pub fn generate_utxo_proof(
        &self,
        txid: &[u8; 32],
        vout: u32,
        value: u64,
        script_pubkey: &[u8],
    ) -> Result<InclusionProof, AppError> {
        // Compute UTXO commitment hash
        let utxo_hash = transparent_utxo_commitment(txid, vout, value, script_pubkey);
        tracing::debug!("utxo hash: {}", hex::encode(utxo_hash));

        // Get snapshot directory and use its nullifiers subdirectory
        let snapshot_dir = self.get_snapshot_dir();
        let nullifier_dir = snapshot_dir.join("nullifiers");

        let sorted_file = nullifier_dir.join("utxo_hashes.bin");
        let tree_dir = nullifier_dir.join("utxo_tree");

        match generate_inclusion_proof_from_files(
            sorted_file
                .to_str()
                .ok_or_else(|| AppError::internal(eyre!("No data")))?,
            tree_dir
                .to_str()
                .ok_or_else(|| AppError::internal(eyre!("No data")))?,
            &utxo_hash,
            self.snapshot_metadata.utxo_root,
        ) {
            Ok(proof) => Ok(proof),
            Err(e) => {
                if e.to_string().contains("Hash not found in sorted file") {
                    return Err(AppError::not_found("UTXO not found"));
                }
                Err(AppError::internal(e))
            }
        }
    }

    /// Generate nullifier exclusion proof
    pub fn generate_nullifier_exclusion(&self, nullifier: &[u8; 32]) -> Result<ExclusionProof> {
        // Get snapshot directory and use its nullifiers subdirectory
        let snapshot_dir = self.get_snapshot_dir();
        let nullifier_dir = snapshot_dir.join("nullifiers");

        let sorted_file = nullifier_dir.join("nullifiers_unsorted.bin");
        let tree_dir = nullifier_dir.join("nullifier_tree");

        generate_exclusion_proof_from_files(
            sorted_file.to_str().ok_or_else(|| eyre!("Invalid path"))?,
            tree_dir.to_str().ok_or_else(|| eyre!("Invalid path"))?,
            nullifier,
            self.snapshot_metadata.nullifier_root,
        )
    }
}
