//! In-memory witness source for WASM
//!
//! This witness source works entirely in-memory without requiring a filesystem.
//! It's designed for WASM environments where only active shards are available.

use crate::{
    MerklePath, Pool, ShardFile, WitnessSource, position_within_shard, shard_id_from_position,
};
use eyre::{Result, eyre};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// In-memory witness source that works with pre-loaded shards
///
/// This is useful for WASM where shards are fetched via HTTP and we don't have
/// access to the full snapshot directory structure.
pub struct InMemoryWitnessSource {
    /// Cache of shards: (pool, shard_id) -> ShardFile
    shards: Arc<RwLock<HashMap<(Pool, u32), Arc<ShardFile>>>>,
    /// Pool roots (for validation)
    orchard_root: [u8; 32],
    sapling_root: [u8; 32],
    /// Shard paths from metadata: (pool, shard_id) -> Vec<[u8; 32]>
    /// These are the paths from shard root to pool root (16 nodes)
    shard_paths: Arc<RwLock<HashMap<(Pool, u32), Vec<[u8; 32]>>>>,
}

impl InMemoryWitnessSource {
    /// Create a new in-memory witness source
    pub fn new(orchard_root: [u8; 32], sapling_root: [u8; 32]) -> Self {
        Self {
            shards: Arc::new(RwLock::new(HashMap::new())),
            orchard_root,
            sapling_root,
            shard_paths: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add shard paths for a shard (from metadata.shard_paths)
    pub fn add_shard_paths(&self, pool: Pool, shard_id: u32, paths: Vec<[u8; 32]>) {
        let mut shard_paths = self.shard_paths.write().unwrap();
        shard_paths.insert((pool, shard_id), paths);
    }

    /// Add a shard to the witness source
    pub fn add_shard(&self, pool: Pool, shard_id: u32, shard_bytes: Vec<u8>) -> Result<()> {
        let shard = Arc::new(ShardFile::from_bytes(shard_bytes, pool)?);
        let mut shards = self.shards.write().unwrap();
        shards.insert((pool, shard_id), shard);
        // Note: Can't use log::debug in shard-builder without adding log dependency
        // println!("Added in-memory shard ({:?}, {})", pool, shard_id);
        Ok(())
    }

    /// Get a shard from the cache
    fn get_shard(&self, pool: Pool, shard_id: u32) -> Result<Arc<ShardFile>> {
        let shards = self.shards.read().unwrap();
        shards
            .get(&(pool, shard_id))
            .ok_or_else(|| {
                eyre!(
                    "Shard ({:?}, {}) not found in in-memory cache",
                    pool,
                    shard_id
                )
            })
            .map(Arc::clone)
    }
}

impl WitnessSource for InMemoryWitnessSource {
    fn get_merkle_path(&self, pool: Pool, position: u64) -> Result<MerklePath> {
        let shard_id = shard_id_from_position(position, pool);
        let shard = self.get_shard(pool, shard_id)?;
        let pos_in_shard = position_within_shard(position, pool);

        // Get path within shard (depth 16)
        let mut shard_path = shard.extract_path(pos_in_shard)?;

        // Append root path (levels 16-31) if available
        let shard_paths = self.shard_paths.read().unwrap();
        if let Some(root_path) = shard_paths.get(&(pool, shard_id)) {
            shard_path.extend_from_slice(root_path);
        }
        // If shard_paths not available, we only return the shard path (16 levels)
        // This may cause verification to fail, but it's better than nothing

        Ok(shard_path)
    }
}
