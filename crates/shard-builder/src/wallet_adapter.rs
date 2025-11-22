//! Wallet DB adapter for fetching commitments
//! 
//! NOTE: This adapter is not yet implemented and will fail fast to prevent
//! generating empty snapshots silently.
//! 
//! The wallet database only stores notes that belong to the wallet, not all
//! commitments in the tree. To build a complete snapshot, we need ALL commitments
//! from the entire tree, which requires scanning blocks (via Zebra) or accessing
//! a full node's commitment tree state.
//! 
//! Future implementation options:
//! 1. Query Zebra's commitment tree state directly (if available via API)
//! 2. Scan blocks using Zebra adapter (when implemented)
//! 3. Access full node's incremental tree state

use eyre::{Result, eyre};
use crate::CommitmentWithPosition;

#[cfg(feature = "wallet-db")]
use zcash_client_sqlite::WalletDb;

/// Fetch all Orchard commitments from wallet DB up to a given height
/// 
/// NOTE: This function is not yet implemented. The wallet DB only stores
/// wallet-owned notes, not all commitments in the tree. To build a complete
/// snapshot, we need ALL commitments, which requires a different data source.
#[cfg(feature = "wallet-db")]
pub fn fetch_orchard_commitments_from_wallet(
    _wallet_db: &WalletDb,
    _target_height: u32,
) -> Result<Vec<CommitmentWithPosition>> {
    Err(eyre!(
        "Wallet DB adapter not yet implemented. \
        The wallet database only stores notes belonging to the wallet, not all commitments \
        in the tree. To build a complete snapshot, you need ALL commitments from the entire \
        tree, which requires:\n\
        1. Implementing the Zebra adapter (--features zebra) to scan blocks, or\n\
        2. Accessing a full node's commitment tree state directly.\n\
        \n\
        For now, use synthetic data for testing: cargo run --bin shard_builder"
    ))
}

/// Fetch all Sapling commitments from wallet DB up to a given height
/// 
/// NOTE: This function is not yet implemented. The wallet DB only stores
/// wallet-owned notes, not all commitments in the tree. To build a complete
/// snapshot, we need ALL commitments, which requires a different data source.
#[cfg(feature = "wallet-db")]
pub fn fetch_sapling_commitments_from_wallet(
    _wallet_db: &WalletDb,
    _target_height: u32,
) -> Result<Vec<CommitmentWithPosition>> {
    Err(eyre!(
        "Wallet DB adapter not yet implemented. \
        The wallet database only stores notes belonging to the wallet, not all commitments \
        in the tree. To build a complete snapshot, you need ALL commitments from the entire \
        tree, which requires:\n\
        1. Implementing the Zebra adapter (--features zebra) to scan blocks, or\n\
        2. Accessing a full node's commitment tree state directly.\n\
        \n\
        For now, use synthetic data for testing: cargo run --bin shard_builder"
    ))
}

/// Fetch all commitments from wallet DB up to a given height
#[cfg(feature = "wallet-db")]
pub fn fetch_all_commitments_from_wallet(
    wallet_db: &WalletDb,
    target_height: u32,
) -> Result<(Vec<CommitmentWithPosition>, Vec<CommitmentWithPosition>)> {
    let orchard = fetch_orchard_commitments_from_wallet(wallet_db, target_height)?;
    let sapling = fetch_sapling_commitments_from_wallet(wallet_db, target_height)?;
    Ok((orchard, sapling))
}

