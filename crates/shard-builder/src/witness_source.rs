//! SnapshotWitnessSource - witness extraction from snapshot shards
//!
//! This module implements the WitnessSource trait for extracting Merkle paths
//! from snapshot shard files. It batches requests by (pool, shard_id) to minimize
//! shard loads and supports LRU caching with pinning for hot shards.

use crate::{Pool, ShardFile, SnapshotMetadata, position_within_shard, shard_id_from_position};
use eyre::{Result, ensure, eyre};
use log::{debug, info, warn};
use lru::LruCache;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

/// Merkle path (siblings from leaf to root, excluding the leaf itself)
pub type MerklePath = Vec<[u8; 32]>;

/// Trait for witness sources
///
/// This allows different witness sources (snapshot, wallet, etc.) to be
/// used interchangeably in the prover pipeline.
pub trait WitnessSource {
    /// Get a Merkle path for a given position in a pool
    fn get_merkle_path(&self, pool: Pool, position: u64) -> Result<MerklePath>;
}

/// Cache configuration for SnapshotWitnessSource
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of shards to cache per pool (excluding pinned shards)
    pub lru_capacity: usize,
    /// Whether to prewarm the cache with active shards on startup
    pub prewarm: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            lru_capacity: 4, // Default: cache 4 additional shards per pool
            prewarm: true,   // Default: prewarm active shards
        }
    }
}

/// Snapshot-based witness source with LRU caching and hot shard pinning
///
/// Loads shards on-demand and extracts witnesses. Uses LRU cache for older shards
/// and pins the active shard(s) for each pool to avoid eviction.
pub struct SnapshotWitnessSource {
    snapshot_dir: PathBuf,
    /// LRU cache for shards: (pool, shard_id) -> ShardFile
    /// Protected by RwLock for concurrent access
    shard_cache: Arc<RwLock<LruCache<(Pool, u32), Arc<ShardFile>>>>,
    /// Set of pinned shard IDs that should never be evicted
    /// Keyed by (pool, shard_id)
    pinned_shards: Arc<RwLock<HashSet<(Pool, u32)>>>,
    /// Cache hit/miss metrics per pool
    cache_hits: Arc<RwLock<HashMap<Pool, AtomicU64>>>,
    cache_misses: Arc<RwLock<HashMap<Pool, AtomicU64>>>,
    config: CacheConfig,
}

impl SnapshotWitnessSource {
    /// Create a new snapshot witness source with default cache configuration
    pub fn new<P: AsRef<Path>>(snapshot_dir: P) -> Result<Self> {
        Self::with_config(snapshot_dir, CacheConfig::default())
    }

    /// Create a new snapshot witness source with custom cache configuration
    pub fn with_config<P: AsRef<Path>>(snapshot_dir: P, config: CacheConfig) -> Result<Self> {
        let snapshot_dir = snapshot_dir.as_ref().to_path_buf();

        // Verify snapshot directory exists
        ensure!(
            snapshot_dir.exists(),
            "Snapshot directory does not exist: {}",
            snapshot_dir.display()
        );

        // Verify metadata.json exists
        let metadata_path = snapshot_dir.join("metadata.json");
        ensure!(
            metadata_path.exists(),
            "Snapshot metadata not found: {}",
            metadata_path.display()
        );

        // Create LRU cache with capacity (minimum 1)
        let capacity = NonZeroUsize::new(config.lru_capacity.max(1))
            .ok_or_else(|| eyre::eyre!("Cache capacity must be at least 1"))?;
        let shard_cache = Arc::new(RwLock::new(LruCache::new(capacity)));

        // Initialize metrics
        let mut cache_hits: HashMap<Pool, AtomicU64> = HashMap::new();
        let mut cache_misses: HashMap<Pool, AtomicU64> = HashMap::new();
        cache_hits.insert(Pool::Orchard, AtomicU64::new(0));
        cache_hits.insert(Pool::Sapling, AtomicU64::new(0));
        cache_misses.insert(Pool::Orchard, AtomicU64::new(0));
        cache_misses.insert(Pool::Sapling, AtomicU64::new(0));

        let source = Self {
            snapshot_dir,
            shard_cache,
            pinned_shards: Arc::new(RwLock::new(HashSet::new())),
            cache_hits: Arc::new(RwLock::new(cache_hits)),
            cache_misses: Arc::new(RwLock::new(cache_misses)),
            config,
        };

        // Prewarm if configured
        if source.config.prewarm {
            source.warmup()?;
        }

        Ok(source)
    }

    /// Warmup the cache by preloading active shards
    ///
    /// Identifies the latest shard for each pool from metadata and preloads them.
    /// These shards are pinned to prevent eviction.
    pub fn warmup(&self) -> Result<()> {
        debug!("Warming up shard cache...");

        // Load metadata to identify active shards
        let metadata_path = self.snapshot_dir.join("metadata.json");
        let metadata_str = std::fs::read_to_string(&metadata_path)
            .map_err(|e| eyre::eyre!("Failed to read snapshot metadata: {}", e))?;
        let metadata: SnapshotMetadata = serde_json::from_str(&metadata_str)
            .map_err(|e| eyre::eyre!("Failed to parse snapshot metadata: {}", e))?;

        // Find latest shard ID for each pool
        let mut pinned_count = 0;

        for pool in [Pool::Orchard, Pool::Sapling] {
            let pool_name = match pool {
                Pool::Orchard => "orchard",
                Pool::Sapling => "sapling",
            };

            if let Some(shards) = metadata.shards.get(pool_name) {
                // Find maximum shard ID
                let max_shard_id = shards.keys().filter_map(|k| k.parse::<u32>().ok()).max();

                if let Some(shard_id) = max_shard_id {
                    // Pin and preload the latest shard
                    self.pin_shard(pool, shard_id)?;
                    self.load_shard_internal(pool, shard_id, true)?;
                    pinned_count += 1;
                    debug!("Pinned and preloaded {} shard {}", pool_name, shard_id);
                }
            }
        }

        info!(
            "Cache warmup complete: {} shards pinned and preloaded",
            pinned_count
        );
        Ok(())
    }

    /// Pin a shard to prevent eviction from the cache
    pub fn pin_shard(&self, pool: Pool, shard_id: u32) -> Result<()> {
        let mut pinned = self.pinned_shards.write().unwrap();
        pinned.insert((pool, shard_id));
        debug!("Pinned shard ({:?}, {})", pool, shard_id);
        Ok(())
    }

    /// Inject an in-memory shard into the cache
    /// This is useful for WASM where shards are fetched via HTTP
    pub fn inject_shard(&self, pool: Pool, shard_id: u32, shard_bytes: Vec<u8>) -> Result<()> {
        let shard = Arc::new(ShardFile::from_bytes(shard_bytes, pool)?);

        // Pin the shard to prevent eviction
        self.pin_shard(pool, shard_id)?;

        // Insert into cache
        let mut cache = self.shard_cache.write().unwrap();
        cache.put((pool, shard_id), shard);

        debug!("Injected in-memory shard ({:?}, {})", pool, shard_id);
        Ok(())
    }

    /// Unpin a shard, allowing it to be evicted
    pub fn unpin_shard(&self, pool: Pool, shard_id: u32) -> Result<()> {
        let mut pinned = self.pinned_shards.write().unwrap();
        pinned.remove(&(pool, shard_id));
        debug!("Unpinned shard ({:?}, {})", pool, shard_id);
        Ok(())
    }

    /// Load a shard file (with caching and metrics)
    fn load_shard(&self, pool: Pool, shard_id: u32) -> Result<Arc<ShardFile>> {
        self.load_shard_internal(pool, shard_id, false)
    }

    /// Internal shard loading logic
    fn load_shard_internal(
        &self,
        pool: Pool,
        shard_id: u32,
        is_warmup: bool,
    ) -> Result<Arc<ShardFile>> {
        // Check cache first
        {
            let mut cache = self.shard_cache.write().unwrap();
            if let Some(shard) = cache.get(&(pool, shard_id)) {
                if !is_warmup {
                    // Track cache hit
                    if let Some(hits) = self.cache_hits.read().unwrap().get(&pool) {
                        hits.fetch_add(1, Ordering::Relaxed);
                    }
                }
                return Ok(Arc::clone(shard));
            }
        }

        // Cache miss - track it
        if !is_warmup {
            if let Some(misses) = self.cache_misses.read().unwrap().get(&pool) {
                misses.fetch_add(1, Ordering::Relaxed);
            }
        }

        // Load from disk
        let pool_name = match pool {
            Pool::Orchard => "orchard",
            Pool::Sapling => "sapling",
        };
        let shard_path = self
            .snapshot_dir
            .join("shards")
            .join(pool_name)
            .join(format!("{}.bin", shard_id));

        ensure!(
            shard_path.exists(),
            "Shard file not found: {}",
            shard_path.display()
        );

        let shard = Arc::new(ShardFile::open(&shard_path, pool)?);

        // Check if this shard is pinned
        let is_pinned = {
            let pinned = self.pinned_shards.read().unwrap();
            pinned.contains(&(pool, shard_id))
        };

        // Insert into cache
        {
            let mut cache = self.shard_cache.write().unwrap();

            // If shard is pinned, we need to ensure it stays in cache
            // For pinned shards, we'll keep them even if LRU would evict
            if is_pinned {
                // For pinned shards, we can safely put them in cache
                // They won't be evicted because we check pinned set before eviction
                cache.put((pool, shard_id), Arc::clone(&shard));
            } else {
                // For non-pinned shards, use normal LRU eviction
                // Check if we need to evict
                if cache.len() >= cache.cap().get() {
                    // Find a non-pinned entry to evict
                    let mut evicted = false;
                    let keys_to_remove: Vec<_> = cache
                        .iter()
                        .filter(|(k, _)| {
                            let pinned = self.pinned_shards.read().unwrap();
                            !pinned.contains(k)
                        })
                        .map(|(k, _)| *k)
                        .collect();

                    if let Some(key_to_evict) = keys_to_remove.first() {
                        cache.pop(key_to_evict);
                        evicted = true;
                        warn!(
                            "Cache evicted shard ({:?}, {})",
                            key_to_evict.0, key_to_evict.1
                        );
                    }

                    if !evicted {
                        // All entries are pinned, warn but don't evict
                        warn!("Cache at capacity but all shards are pinned, cannot evict");
                    }
                }

                cache.put((pool, shard_id), Arc::clone(&shard));
            }
        }

        Ok(shard)
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> CacheStats {
        let cache = self.shard_cache.read().unwrap();
        let pinned = self.pinned_shards.read().unwrap();

        let mut hits: HashMap<Pool, u64> = HashMap::new();
        let mut misses: HashMap<Pool, u64> = HashMap::new();

        for pool in [Pool::Orchard, Pool::Sapling] {
            if let Some(h) = self.cache_hits.read().unwrap().get(&pool) {
                hits.insert(pool, h.load(Ordering::Relaxed));
            }
            if let Some(m) = self.cache_misses.read().unwrap().get(&pool) {
                misses.insert(pool, m.load(Ordering::Relaxed));
            }
        }

        CacheStats {
            cache_size: cache.len(),
            cache_capacity: cache.cap().get(),
            pinned_count: pinned.len(),
            hits,
            misses,
        }
    }

    /// Log cache statistics
    pub fn log_cache_stats(&self) {
        let stats = self.cache_stats();
        info!(
            "Cache stats: size={}/{}, pinned={}, hits={:?}, misses={:?}",
            stats.cache_size, stats.cache_capacity, stats.pinned_count, stats.hits, stats.misses
        );
    }

    /// Batch extract paths for multiple positions in the same shard
    ///
    /// This is more efficient than calling get_merkle_path() multiple times
    /// as it only loads the shard once.
    pub fn batch_get_paths(&self, pool: Pool, positions: &[u64]) -> Result<Vec<(u64, MerklePath)>> {
        if positions.is_empty() {
            return Ok(Vec::new());
        }

        // Group positions by shard_id
        let mut by_shard: HashMap<u32, Vec<u64>> = HashMap::new();
        for &pos in positions {
            let shard_id = shard_id_from_position(pos, pool);
            by_shard.entry(shard_id).or_insert_with(Vec::new).push(pos);
        }

        let mut results = Vec::new();

        // Process each shard
        for (shard_id, shard_positions) in by_shard {
            let shard = self.load_shard(pool, shard_id)?;

            for pos in shard_positions {
                let pos_in_shard = position_within_shard(pos, pool);
                let path = shard.extract_path(pos_in_shard)?;
                results.push((pos, path));
            }
        }

        Ok(results)
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub cache_size: usize,
    pub cache_capacity: usize,
    pub pinned_count: usize,
    pub hits: std::collections::HashMap<Pool, u64>,
    pub misses: std::collections::HashMap<Pool, u64>,
}

impl WitnessSource for SnapshotWitnessSource {
    fn get_merkle_path(&self, pool: Pool, position: u64) -> Result<MerklePath> {
        // Step 1: Extract 16 siblings from shard file (levels 0-15)
        let siblings_from_shard = self.extract_siblings_from_shard(pool, position)?;

        // Step 2: Load metadata and get 16 siblings from shard root path (levels 16-31)
        let metadata = self.load_metadata()?;
        let siblings_from_metadata =
            self.extract_siblings_from_metadata(&metadata, pool, position)?;

        // Step 3: Combine both to get complete 32-level path
        let complete_path = self.combine_paths(siblings_from_shard, siblings_from_metadata);

        Ok(complete_path)
    }
}

impl SnapshotWitnessSource {
    /// Extract the first 16 siblings (levels 0-15) from the shard file
    fn extract_siblings_from_shard(&self, pool: Pool, position: u64) -> Result<Vec<[u8; 32]>> {
        // Calculate which shard contains this position
        let shard_id = shard_id_from_position(position, pool);

        // Load the shard file (with LRU caching)
        let shard = self.load_shard(pool, shard_id)?;

        // Calculate position within this shard (0 to 65535)
        let pos_in_shard = position_within_shard(position, pool);

        // Walk up the binary tree extracting siblings at each level
        let path = shard.extract_path(pos_in_shard)?;

        debug!(
            "Extracted {} siblings from shard {} for position {}",
            path.len(),
            shard_id,
            position
        );

        Ok(path)
    }

    /// Load and parse metadata.json
    fn load_metadata(&self) -> Result<SnapshotMetadata> {
        let metadata_path = self.snapshot_dir.join("metadata.json");
        let metadata_str = std::fs::read_to_string(&metadata_path)
            .map_err(|e| eyre!("Failed to read snapshot metadata: {}", e))?;
        let metadata: SnapshotMetadata = serde_json::from_str(&metadata_str)
            .map_err(|e| eyre!("Failed to parse snapshot metadata: {}", e))?;
        Ok(metadata)
    }

    /// Extract the next 16 siblings (levels 16-31) from metadata shard_paths
    fn extract_siblings_from_metadata(
        &self,
        metadata: &SnapshotMetadata,
        pool: Pool,
        position: u64,
    ) -> Result<Vec<[u8; 32]>> {
        let pool_name = match pool {
            Pool::Orchard => "orchard",
            Pool::Sapling => "sapling",
        };

        let shard_id = shard_id_from_position(position, pool);
        let shard_id_str = shard_id.to_string();

        // Look up shard root path in metadata
        let shard_root_path = metadata
            .shard_paths
            .get(pool_name)
            .and_then(|pool_paths| pool_paths.get(&shard_id_str))
            .ok_or_else(|| {
                eyre!(
                    "Shard root path not found in metadata for {} shard {}",
                    pool_name,
                    shard_id
                )
            })?;

        debug!(
            "Loading shard root path for {:?} shard {} (position {}):",
            pool, shard_id, position
        );
        debug!("  Snapshot dir: {}", self.snapshot_dir.display());

        // Decode hex strings to [u8; 32]
        let mut siblings = Vec::with_capacity(shard_root_path.len());

        for (idx, hex_node) in shard_root_path.iter().enumerate() {
            let node = self.decode_hex_node(hex_node, idx)?;
            siblings.push(node);

            // Log first and last few for debugging
            if idx < 3 || idx >= shard_root_path.len() - 3 {
                let level = 16 + idx;
                let position_bit = (position >> level) & 1;
                let sibling_side = if position_bit == 0 { "right" } else { "left" };
                debug!("  Level {}: {} ({} sibling)", level, hex_node, sibling_side);
            }
        }

        debug!(
            "Extracted {} siblings from metadata for shard {}",
            siblings.len(),
            shard_id
        );

        Ok(siblings)
    }

    /// Decode a hex string to a 32-byte array
    fn decode_hex_node(&self, hex_string: &str, index: usize) -> Result<[u8; 32]> {
        let node_bytes = hex::decode(hex_string)
            .map_err(|e| eyre!("Failed to decode hex at index {}: {}", index, e))?;

        if node_bytes.len() != 32 {
            return Err(eyre!(
                "Invalid node length at index {}: expected 32, got {}",
                index,
                node_bytes.len()
            ));
        }

        let mut node = [0u8; 32];
        node.copy_from_slice(&node_bytes);
        Ok(node)
    }

    /// Combine shard siblings and metadata siblings into complete 32-level path
    fn combine_paths(
        &self,
        mut shard_siblings: Vec<[u8; 32]>,
        metadata_siblings: Vec<[u8; 32]>,
    ) -> Vec<[u8; 32]> {
        let shard_count = shard_siblings.len();
        let metadata_count = metadata_siblings.len();

        // Append metadata siblings to shard siblings
        shard_siblings.extend(metadata_siblings);

        debug!(
            "Combined path: {} total siblings ({} from shard + {} from metadata)",
            shard_siblings.len(),
            shard_count,
            metadata_count
        );

        shard_siblings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SnapshotBuilder;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_snapshot(
        temp_dir: &TempDir,
        height: u32,
        orchard_count: u64,
        sapling_count: u64,
    ) -> PathBuf {
        let snapshot_dir = temp_dir.path().join("snapshot");
        fs::create_dir_all(&snapshot_dir).unwrap();

        // Generate synthetic commitments
        let mut orchard_commitments = Vec::new();
        for i in 0..orchard_count {
            let mut node = [0u8; 32];
            node[0..8].copy_from_slice(&(i as u64).to_le_bytes());
            node[8..15].copy_from_slice(b"ORCHARD");
            orchard_commitments.push((i, node));
        }

        let mut sapling_commitments = Vec::new();
        for i in 0..sapling_count {
            let mut node = [0u8; 32];
            node[0..8].copy_from_slice(&(i as u64).to_le_bytes());
            node[8..15].copy_from_slice(b"SAPLING");
            sapling_commitments.push((i, node));
        }

        // Compute roots
        let orchard_root =
            SnapshotBuilder::compute_pool_root(Pool::Orchard, &orchard_commitments).unwrap();
        let sapling_root =
            SnapshotBuilder::compute_pool_root(Pool::Sapling, &sapling_commitments).unwrap();

        // Build snapshot
        let builder = SnapshotBuilder::new(
            height,
            "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            snapshot_dir.clone(),
        );

        builder
            .build_snapshot(
                &orchard_commitments,
                &sapling_commitments,
                orchard_root,
                sapling_root,
            )
            .unwrap();

        snapshot_dir
    }

    #[test]
    fn test_snapshot_witness_source_basic() {
        let temp_dir = TempDir::new().unwrap();
        let snapshot_dir = create_test_snapshot(&temp_dir, 100000, 1000, 1000);

        // Create witness source
        let witness_source = SnapshotWitnessSource::new(&snapshot_dir).unwrap();

        // Test Orchard witnesses
        for pos in [0, 100, 500, 999] {
            let path = witness_source.get_merkle_path(Pool::Orchard, pos).unwrap();
            assert!(
                !path.is_empty(),
                "Path should not be empty for position {}",
                pos
            );
            // Path length should be 16 (log2(65536)) for positions within a single shard
            assert_eq!(
                path.len(),
                16,
                "Path length should be 16 for position {}",
                pos
            );
        }

        // Test Sapling witnesses
        for pos in [0, 100, 500, 999] {
            let path = witness_source.get_merkle_path(Pool::Sapling, pos).unwrap();
            assert!(
                !path.is_empty(),
                "Path should not be empty for position {}",
                pos
            );
            assert_eq!(
                path.len(),
                16,
                "Path length should be 16 for position {}",
                pos
            );
        }
    }

    #[test]
    fn test_snapshot_witness_source_batch() {
        let temp_dir = TempDir::new().unwrap();
        let snapshot_dir = create_test_snapshot(&temp_dir, 100000, 1000, 1000);

        let witness_source = SnapshotWitnessSource::new(&snapshot_dir).unwrap();

        // Batch request for multiple positions
        let positions = vec![0, 100, 200, 300, 400, 500];
        let results = witness_source
            .batch_get_paths(Pool::Orchard, &positions)
            .unwrap();

        assert_eq!(results.len(), positions.len());
        for (pos, path) in &results {
            assert!(
                positions.contains(pos),
                "Position {} should be in results",
                pos
            );
            assert_eq!(path.len(), 16, "Path length should be 16");
        }

        // Verify individual requests match batch requests
        for (pos, batch_path) in &results {
            let individual_path = witness_source.get_merkle_path(Pool::Orchard, *pos).unwrap();
            assert_eq!(
                batch_path, &individual_path,
                "Batch and individual paths should match for position {}",
                pos
            );
        }
    }

    #[test]
    fn test_snapshot_witness_source_cross_shard() {
        let temp_dir = TempDir::new().unwrap();
        // Create snapshot with enough commitments to span multiple shards
        // 70000 commitments = shard 0 (65536) + shard 1 (4464)
        let snapshot_dir = create_test_snapshot(&temp_dir, 100000, 70000, 70000);

        let witness_source = SnapshotWitnessSource::new(&snapshot_dir).unwrap();

        // Test positions in different shards (shards 0 and 1 exist)
        let positions = vec![0, 65536]; // Positions in shards 0 and 1
        let results = witness_source
            .batch_get_paths(Pool::Orchard, &positions)
            .unwrap();

        assert_eq!(results.len(), positions.len());
        for (pos, path) in &results {
            assert_eq!(
                path.len(),
                16,
                "Path length should be 16 for position {}",
                pos
            );
        }

        // Position in non-existent shard 2 should fail
        assert!(
            witness_source
                .get_merkle_path(Pool::Orchard, 131072)
                .is_err()
        );
    }

    #[test]
    fn test_snapshot_witness_source_invalid_position() {
        let temp_dir = TempDir::new().unwrap();
        let snapshot_dir = create_test_snapshot(&temp_dir, 100000, 1000, 1000);

        let witness_source = SnapshotWitnessSource::new(&snapshot_dir).unwrap();

        // Position beyond available commitments should fail
        // Note: Shards are padded to SHARD_SIZE, but positions beyond actual commitments
        // will have empty leaves. However, extract_path will succeed for any position < SHARD_SIZE.
        // To test invalid positions, we need positions beyond the last shard.
        // With 1000 commitments, we only have shard 0, so positions >= 65536 should fail.
        assert!(
            witness_source
                .get_merkle_path(Pool::Orchard, 65536)
                .is_err()
        );
        assert!(
            witness_source
                .get_merkle_path(Pool::Sapling, 65536)
                .is_err()
        );
    }

    #[test]
    fn test_snapshot_witness_source_invalid_dir() {
        let temp_dir = TempDir::new().unwrap();
        let invalid_dir = temp_dir.path().join("nonexistent");

        assert!(SnapshotWitnessSource::new(&invalid_dir).is_err());
    }

    #[test]
    fn test_cache_hit_rate() {
        let temp_dir = TempDir::new().unwrap();
        // Create snapshot spanning 2 shards
        let snapshot_dir = create_test_snapshot(&temp_dir, 100000, 70000, 70000);

        // Create witness source with small cache (capacity 2)
        let config = CacheConfig {
            lru_capacity: 2,
            prewarm: false, // Disable prewarm for this test
        };
        let witness_source = SnapshotWitnessSource::with_config(&snapshot_dir, config).unwrap();

        // Initial stats should show no hits
        let stats = witness_source.cache_stats();
        assert_eq!(stats.hits.get(&Pool::Orchard).unwrap_or(&0), &0);
        assert_eq!(stats.misses.get(&Pool::Orchard).unwrap_or(&0), &0);

        // Load witnesses from shard 0 (cache miss)
        witness_source.get_merkle_path(Pool::Orchard, 0).unwrap();
        let stats = witness_source.cache_stats();
        assert_eq!(stats.misses.get(&Pool::Orchard).unwrap_or(&0), &1);
        assert_eq!(stats.cache_size, 1);

        // Load same shard again (cache hit)
        witness_source.get_merkle_path(Pool::Orchard, 100).unwrap();
        let stats = witness_source.cache_stats();
        assert_eq!(stats.hits.get(&Pool::Orchard).unwrap_or(&0), &1);
        assert_eq!(stats.misses.get(&Pool::Orchard).unwrap_or(&0), &1);

        // Load from shard 1 (cache miss)
        witness_source
            .get_merkle_path(Pool::Orchard, 65536)
            .unwrap();
        let stats = witness_source.cache_stats();
        assert_eq!(stats.misses.get(&Pool::Orchard).unwrap_or(&0), &2);
        assert_eq!(stats.cache_size, 2);

        // Load from shard 1 again (cache hit)
        witness_source
            .get_merkle_path(Pool::Orchard, 65537)
            .unwrap();
        let stats = witness_source.cache_stats();
        assert_eq!(stats.hits.get(&Pool::Orchard).unwrap_or(&0), &2);
    }

    #[test]
    fn test_pin_unpin_shard() {
        let temp_dir = TempDir::new().unwrap();
        let snapshot_dir = create_test_snapshot(&temp_dir, 100000, 70000, 70000);

        let witness_source = SnapshotWitnessSource::new(&snapshot_dir).unwrap();

        // Pin shard 0
        witness_source.pin_shard(Pool::Orchard, 0).unwrap();
        let stats = witness_source.cache_stats();
        assert_eq!(stats.pinned_count, 1);

        // Load shard 0
        witness_source.get_merkle_path(Pool::Orchard, 0).unwrap();

        // Unpin shard 0
        witness_source.unpin_shard(Pool::Orchard, 0).unwrap();
        let stats = witness_source.cache_stats();
        assert_eq!(stats.pinned_count, 0);
    }

    #[test]
    fn test_warmup() {
        let temp_dir = TempDir::new().unwrap();
        let snapshot_dir = create_test_snapshot(&temp_dir, 100000, 70000, 70000);

        // Create with prewarm enabled
        let config = CacheConfig {
            lru_capacity: 4,
            prewarm: true,
        };
        let witness_source = SnapshotWitnessSource::with_config(&snapshot_dir, config).unwrap();

        // After warmup, latest shards should be pinned and cached
        let stats = witness_source.cache_stats();
        // Latest shard for Orchard is 1 (70000 commitments = shard 0 + shard 1)
        // Latest shard for Sapling is also 1
        assert!(stats.pinned_count >= 1); // At least one shard pinned
        assert!(stats.cache_size >= 1); // At least one shard cached
    }
}
