use crate::optimized_ops::parallel_fill_nodes;
use crate::{
    Pool, SHARD_SIZE, ShardFile, build_tree_levels_pool, commitment_to_node, shard_end_position,
    shard_id_from_position, shard_start_position,
};
use ahash::AHashMap;
use blake2b_simd;
use eyre::{Result, eyre};
use incrementalmerkletree::Hashable;
use memmap2::Mmap;
use once_cell::sync::Lazy;
use orchard::tree::MerkleHashOrchard;
use rayon::prelude::*;
use sapling::Node as SaplingNode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

// Pre-compute empty leaves once (lazy static)
static EMPTY_ORCHARD_LEAF: Lazy<[u8; 32]> =
    Lazy::new(|| <MerkleHashOrchard as Hashable>::empty_leaf().to_bytes());

static EMPTY_SAPLING_LEAF: Lazy<[u8; 32]> =
    Lazy::new(|| <SaplingNode as Hashable>::empty_leaf().to_bytes());

/// Snapshot metadata structure matching mds/architecture/prover_architecture.md spec
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub snapshot_height: u32,
    pub block_hash: String,
    pub orchard_root: String,
    pub sapling_root: String,
    pub orchard_count: u64,
    pub sapling_count: u64,
    pub schema_version: String,
    pub created_at: String,
    pub complete: bool,
    pub shards: HashMap<String, HashMap<String, ShardInfo>>,
    /// Paths from each shard root to the pool root (16 nodes each)
    /// Format: HashMap<pool_name, HashMap<shard_id_string, Vec<hex_encoded_nodes>>>
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub shard_paths: HashMap<String, HashMap<String, Vec<String>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardInfo {
    pub file: String,
    pub size: u64,
    pub blake2b: String,
}

/// Build a complete snapshot for a given height
pub struct SnapshotBuilder {
    height: u32,
    block_hash: String,
    snapshot_dir: PathBuf,
}

impl SnapshotBuilder {
    pub fn new(height: u32, block_hash: String, snapshot_dir: PathBuf) -> Self {
        Self {
            height,
            block_hash,
            snapshot_dir,
        }
    }

    /// Build all shards for a pool at the given height
    ///
    /// Returns: (total_count, shard_infos, shard_roots)
    /// where shard_roots is Vec<(shard_id, root_hash)>
    pub fn build_pool_shards(
        &self,
        pool: Pool,
        commitments: &[(u64, [u8; 32])], // (position, commitment)
    ) -> Result<(u64, Vec<ShardInfo>, Vec<(u32, [u8; 32])>)> {
        let total_start = Instant::now();
        let pool_name = match pool {
            Pool::Orchard => "orchard",
            Pool::Sapling => "sapling",
        };

        // Always create shards directory, even for empty pools
        let shards_dir = self.snapshot_dir.join("shards").join(pool_name);
        fs::create_dir_all(&shards_dir)?;

        // Group commitments by shard (use AHashMap for faster hashing)
        let grouping_start = Instant::now();
        let mut by_shard: AHashMap<u32, Vec<(u64, [u8; 32])>> = AHashMap::new();
        for (position, commitment) in commitments {
            let shard_id = shard_id_from_position(*position, pool);
            by_shard
                .entry(shard_id)
                .or_insert_with(Vec::new)
                .push((*position, *commitment));
        }
        let grouping_time = grouping_start.elapsed();
        println!(
            "  {} grouping: {:.2}s",
            pool_name,
            grouping_time.as_secs_f64()
        );

        // Handle empty pools - log and let loop run (will be empty, but directory exists)
        if commitments.is_empty() {
            println!("  No {} commitments, no shards to build", pool_name);
        }

        // Optimized: Parallel shard building with sparse filling
        // Pre-compute empty leaf once
        let empty_node = match pool {
            Pool::Orchard => *EMPTY_ORCHARD_LEAF,
            Pool::Sapling => *EMPTY_SAPLING_LEAF,
        };

        // Build shards in parallel with progress reporting
        let total_shards = by_shard.len();
        println!(
            "  Building {} {} shards in parallel...",
            total_shards, pool_name
        );

        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let progress_counter = Arc::new(AtomicUsize::new(0));
        let progress_counter_clone = progress_counter.clone();
        let pool_name_progress = pool_name.to_string();

        // Spawn progress thread for shard building
        let progress_handle = std::thread::spawn(move || {
            let start = std::time::Instant::now();
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let processed = progress_counter_clone.load(Ordering::Relaxed);
                if processed >= total_shards {
                    break;
                }
                let elapsed = start.elapsed().as_secs_f64();
                let percentage = (processed as f64 / total_shards as f64) * 100.0;
                let shards_per_sec = if elapsed > 0.0 {
                    processed as f64 / elapsed
                } else {
                    0.0
                };
                let remaining = total_shards - processed;
                let eta_secs = if shards_per_sec > 0.0 {
                    remaining as f64 / shards_per_sec
                } else {
                    0.0
                };
                println!(
                    "  {} shards: {}/{} ({:.1}%) | {:.1} shards/sec | {} remaining | ETA: {:.0}s",
                    pool_name_progress,
                    processed,
                    total_shards,
                    percentage,
                    shards_per_sec,
                    remaining,
                    eta_secs
                );
            }
        });

        let progress_counter_for_map = progress_counter.clone();
        // OPTIMIZATION: Convert to Vec and sort all shards' commitments in parallel ONCE
        let sorting_start = Instant::now();
        let mut by_shard_vec: Vec<_> = by_shard.into_iter().collect();
        by_shard_vec.par_iter_mut().for_each(|(_, commitments)| {
            commitments.sort_unstable_by_key(|(pos, _)| *pos); // Unstable is faster
        });
        let sorting_time = sorting_start.elapsed();
        println!(
            "  {} sorting: {:.2}s",
            pool_name,
            sorting_time.as_secs_f64()
        );

        let tree_build_start = Instant::now();
        let shard_results: Result<Vec<_>> = by_shard_vec
            .par_iter()
            .map(|(shard_id, shard_commitments)| {
                // Commitments already sorted!
                let start_pos = shard_start_position(*shard_id, pool);
                let _end_pos = shard_end_position(*shard_id, pool);

                // ULTRA-OPTIMIZED: Parallel sparse filling
                let mut nodes = vec![[0u8; 32]; SHARD_SIZE as usize];
                parallel_fill_nodes(&mut nodes, &empty_node);
                let mut total_count = 0u64;

                // Then overwrite positions that have commitments (sparse update)
                for (pos, commitment) in shard_commitments.iter() {
                    let pos_in_shard = (pos - start_pos) as usize;
                    if pos_in_shard < SHARD_SIZE as usize {
                        let node = commitment_to_node(pool, commitment)?;
                        nodes[pos_in_shard] = node;
                        total_count += 1;
                    }
                }

                // CRITICAL: leaf_count must match nodes.len() (what's actually written),
                // not shard_commitments.len() (actual commitments). The reader uses leaf_count
                // to compute offsets, so they must match the on-disk layout.
                let leaf_count = nodes.len() as u32;
                let actual_commitment_count = shard_commitments.len() as u32;

                // Build tree levels
                let levels = build_tree_levels_pool(&nodes, pool)?;

                // Write shard file
                let shard_filename = format!("{}.bin", shard_id);
                let shard_path = shards_dir.join(&shard_filename);

                let _shard = ShardFile::create(
                    &shard_path,
                    pool,
                    *shard_id,
                    start_pos,
                    _end_pos,
                    leaf_count, // Use nodes.len(), not commitment count
                    &levels,
                )?;

                // ULTRA-OPTIMIZED: Parallel Blake2b hashing using chunks for large files
                let file_size = fs::metadata(&shard_path)?.len();
                let file = fs::File::open(&shard_path)?;
                let mmap = unsafe { Mmap::map(&file)? };

                // Parallel chunk hashing for files > 1MB
                const CHUNK_SIZE: usize = 512 * 1024; // 512KB chunks
                let hash_hex = if mmap.len() > 1024 * 1024 {
                    // Parallel hashing for large files
                    use rayon::prelude::*;
                    let chunk_hashes: Vec<_> = mmap
                        .par_chunks(CHUNK_SIZE)
                        .map(|chunk| {
                            let mut hasher = blake2b_simd::Params::new().hash_length(32).to_state();
                            hasher.update(chunk);
                            hasher.finalize()
                        })
                        .collect();

                    // Combine chunk hashes
                    let mut final_hasher = blake2b_simd::Params::new().hash_length(32).to_state();
                    for chunk_hash in chunk_hashes {
                        final_hasher.update(chunk_hash.as_bytes());
                    }
                    hex::encode(final_hasher.finalize().as_bytes())
                } else {
                    // Sequential hashing for small files
                    let mut hasher = blake2b_simd::Params::new().hash_length(32).to_state();
                    let hash = hasher.update(&mmap).finalize();
                    hex::encode(hash.as_bytes())
                };

                // Extract shard root from levels (level 16 for 2^16 leaves)
                let shard_root = levels[16][0];

                // Update progress counter
                progress_counter_for_map.fetch_add(1, Ordering::Relaxed);

                Ok((
                    *shard_id,
                    ShardInfo {
                        file: format!("shards/{}/{}", pool_name, shard_filename),
                        size: file_size,
                        blake2b: hash_hex,
                    },
                    total_count,
                    actual_commitment_count,
                    leaf_count,
                    shard_root,
                ))
            })
            .collect();

        // Wait for progress thread to finish
        progress_counter.store(total_shards, Ordering::Relaxed);
        progress_handle.join().ok();

        let tree_build_time = tree_build_start.elapsed();
        println!(
            "  {} tree building: {:.2}s",
            pool_name,
            tree_build_time.as_secs_f64()
        );

        let shard_results = shard_results?;

        let file_write_start = Instant::now();
        let mut shard_infos = Vec::new();
        let mut shard_roots = Vec::new();
        let mut total_count = 0u64;

        for (shard_id, shard_info, count, actual_count, leaf_count, shard_root) in shard_results {
            let size = shard_info.size;
            total_count += count;
            shard_infos.push(shard_info);
            shard_roots.push((shard_id, shard_root));
            println!(
                "Built shard {} for {:?}: {} commitments, {} total leaves, {} bytes",
                shard_id, pool, actual_count, leaf_count, size
            );
        }
        let file_write_time = file_write_start.elapsed();

        // Detailed timing breakdown
        let total_time = total_start.elapsed();
        println!("\n  === {} Shard Building Performance ===", pool_name);
        println!(
            "    Grouping:        {:.2}s ({:.1}%)",
            grouping_time.as_secs_f64(),
            grouping_time.as_secs_f64() / total_time.as_secs_f64() * 100.0
        );
        println!(
            "    Sorting:         {:.2}s ({:.1}%)",
            sorting_time.as_secs_f64(),
            sorting_time.as_secs_f64() / total_time.as_secs_f64() * 100.0
        );
        println!(
            "    Tree building:   {:.2}s ({:.1}%)",
            tree_build_time.as_secs_f64(),
            tree_build_time.as_secs_f64() / total_time.as_secs_f64() * 100.0
        );
        println!(
            "    File writing:    {:.2}s ({:.1}%)",
            file_write_time.as_secs_f64(),
            file_write_time.as_secs_f64() / total_time.as_secs_f64() * 100.0
        );
        println!("    Total:           {:.2}s", total_time.as_secs_f64());
        println!(
            "    Throughput:      {:.1} shards/sec",
            shard_infos.len() as f64 / total_time.as_secs_f64()
        );

        Ok((total_count, shard_infos, shard_roots))
    }

    /// Build complete snapshot and compute roots from shard roots (optimized - no duplicate work)
    pub fn build_snapshot_with_roots(
        &self,
        orchard_commitments: &[(u64, [u8; 32])],
        sapling_commitments: &[(u64, [u8; 32])],
    ) -> Result<([u8; 32], [u8; 32])> {
        fs::create_dir_all(&self.snapshot_dir)?;

        println!("Building snapshot for height {}...", self.height);
        println!("Block hash: {}", self.block_hash);

        // Build Orchard shards (allow empty) - now returns shard roots too
        println!("\nBuilding Orchard shards...");
        let (orchard_count, orchard_shards, orchard_shard_roots) =
            self.build_pool_shards(Pool::Orchard, orchard_commitments)?;

        // Build Sapling shards (allow empty) - now returns shard roots too
        println!("\nBuilding Sapling shards...");
        let (sapling_count, sapling_shards, sapling_shard_roots) =
            self.build_pool_shards(Pool::Sapling, sapling_commitments)?;

        // Compute roots from shard roots and capture paths (no duplicate tree building!)
        println!("\n=== Computing Tree Roots from Shard Roots ===");
        let root_start = Instant::now();
        let (orchard_root, orchard_shard_paths) =
            Self::stitch_pool_root_with_paths(Pool::Orchard, orchard_shard_roots)?;
        let (sapling_root, sapling_shard_paths) =
            Self::stitch_pool_root_with_paths(Pool::Sapling, sapling_shard_roots)?;
        let root_time = root_start.elapsed();
        println!("  Root computation time: {:.1}s", root_time.as_secs_f64());

        // Convert paths to metadata format (hex-encoded strings)
        let mut shard_paths_map = HashMap::new();
        let mut orchard_paths: HashMap<String, Vec<String>> = HashMap::new();
        for (shard_id, path) in orchard_shard_paths {
            orchard_paths.insert(
                shard_id.to_string(),
                path.iter().map(|node| hex::encode(node)).collect(),
            );
        }
        shard_paths_map.insert("orchard".to_string(), orchard_paths);

        let mut sapling_paths: HashMap<String, Vec<String>> = HashMap::new();
        for (shard_id, path) in sapling_shard_paths {
            sapling_paths.insert(
                shard_id.to_string(),
                path.iter().map(|node| hex::encode(node)).collect(),
            );
        }
        shard_paths_map.insert("sapling".to_string(), sapling_paths);

        // Build metadata
        let mut shards_map = HashMap::new();

        let mut orchard_map = HashMap::new();
        for shard_info in orchard_shards {
            let shard_id = shard_info
                .file
                .strip_prefix(&format!("shards/orchard/"))
                .and_then(|s| s.strip_suffix(".bin"))
                .ok_or_else(|| eyre!("Invalid shard file path"))?
                .to_string();
            orchard_map.insert(shard_id, shard_info);
        }
        shards_map.insert("orchard".to_string(), orchard_map);

        let mut sapling_map = HashMap::new();
        for shard_info in sapling_shards {
            let shard_id = shard_info
                .file
                .strip_prefix(&format!("shards/sapling/"))
                .and_then(|s| s.strip_suffix(".bin"))
                .ok_or_else(|| eyre!("Invalid shard file path"))?
                .to_string();
            sapling_map.insert(shard_id, shard_info);
        }
        shards_map.insert("sapling".to_string(), sapling_map);

        let metadata = SnapshotMetadata {
            snapshot_height: self.height,
            block_hash: self.block_hash.clone(),
            orchard_root: hex::encode(orchard_root),
            sapling_root: hex::encode(sapling_root),
            orchard_count,
            sapling_count,
            schema_version: "1.0".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            complete: true,
            shards: shards_map,
            shard_paths: shard_paths_map,
        };

        // Write metadata.json
        let metadata_path = self.snapshot_dir.join("metadata.json");
        let metadata_json = serde_json::to_string_pretty(&metadata)?;
        fs::write(&metadata_path, metadata_json)?;

        println!("\nSnapshot built successfully!");
        println!(
            "  Orchard: {} commitments in {} shards",
            orchard_count,
            metadata.shards["orchard"].len()
        );
        println!(
            "  Sapling: {} commitments in {} shards",
            sapling_count,
            metadata.shards["sapling"].len()
        );
        println!("  Metadata: {}", metadata_path.display());

        // Try to update SNAPSHOT constant in lib.rs
        if let Err(e) = Self::update_snapshot_constant(
            self.height,
            orchard_root,
            sapling_root,
            orchard_count,
            sapling_count,
        ) {
            eprintln!(
                "Warning: Failed to update SNAPSHOT constant in lib.rs: {}",
                e
            );
            eprintln!("  You may need to manually update crates/lib/src/lib.rs");
        } else {
            println!("  Updated SNAPSHOT constant in crates/lib/src/lib.rs");
        }

        Ok((orchard_root, sapling_root))
    }

    /// Build complete snapshot and write metadata.json (legacy API - uses provided roots)
    pub fn build_snapshot(
        &self,
        orchard_commitments: &[(u64, [u8; 32])],
        sapling_commitments: &[(u64, [u8; 32])],
        orchard_root: [u8; 32],
        sapling_root: [u8; 32],
    ) -> Result<SnapshotMetadata> {
        // Build shards but ignore returned roots (using provided roots instead)
        let (orchard_count, orchard_shards, _) =
            self.build_pool_shards(Pool::Orchard, orchard_commitments)?;
        let (sapling_count, sapling_shards, _) =
            self.build_pool_shards(Pool::Sapling, sapling_commitments)?;

        fs::create_dir_all(&self.snapshot_dir)?;

        println!("Building snapshot for height {}...", self.height);
        println!("Block hash: {}", self.block_hash);

        // Build metadata
        let mut shards_map = HashMap::new();

        let mut orchard_map = HashMap::new();
        for shard_info in orchard_shards {
            let shard_id = shard_info
                .file
                .strip_prefix(&format!("shards/orchard/"))
                .and_then(|s| s.strip_suffix(".bin"))
                .ok_or_else(|| eyre!("Invalid shard file path"))?
                .to_string();
            orchard_map.insert(shard_id, shard_info);
        }
        shards_map.insert("orchard".to_string(), orchard_map);

        let mut sapling_map = HashMap::new();
        for shard_info in sapling_shards {
            let shard_id = shard_info
                .file
                .strip_prefix(&format!("shards/sapling/"))
                .and_then(|s| s.strip_suffix(".bin"))
                .ok_or_else(|| eyre!("Invalid shard file path"))?
                .to_string();
            sapling_map.insert(shard_id, shard_info);
        }
        shards_map.insert("sapling".to_string(), sapling_map);

        let metadata = SnapshotMetadata {
            snapshot_height: self.height,
            block_hash: self.block_hash.clone(),
            orchard_root: hex::encode(orchard_root),
            sapling_root: hex::encode(sapling_root),
            orchard_count,
            sapling_count,
            schema_version: "1.0".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            complete: true,
            shards: shards_map,
            shard_paths: HashMap::new(), // Legacy API doesn't have paths
        };

        // Write metadata.json
        let metadata_path = self.snapshot_dir.join("metadata.json");
        let metadata_json = serde_json::to_string_pretty(&metadata)?;
        fs::write(&metadata_path, metadata_json)?;

        println!("\nSnapshot built successfully!");
        println!(
            "  Orchard: {} commitments in {} shards",
            orchard_count,
            metadata.shards["orchard"].len()
        );
        println!(
            "  Sapling: {} commitments in {} shards",
            sapling_count,
            metadata.shards["sapling"].len()
        );
        println!("  Metadata: {}", metadata_path.display());

        Ok(metadata)
    }

    /// Stitch shard roots together into a pool root and compute paths for each shard root
    ///
    /// This takes pre-computed shard roots and combines them into the final pool root.
    /// Also computes and returns the path from each shard root to the pool root.
    /// Returns: (pool_root, shard_paths) where shard_paths is HashMap<shard_id, path>
    pub fn stitch_pool_root_with_paths(
        pool: Pool,
        mut shard_roots: Vec<(u32, [u8; 32])>,
    ) -> Result<([u8; 32], std::collections::HashMap<u32, Vec<[u8; 32]>>)> {
        use incrementalmerkletree::{Hashable, Level};
        use orchard::tree::MerkleHashOrchard;
        use sapling::Node as SaplingNode;

        const TARGET_LEVEL: u8 = 32; // Orchard/Sapling trees have depth 32

        let pool_name = match pool {
            Pool::Orchard => "Orchard",
            Pool::Sapling => "Sapling",
        };

        println!(
            "  {} root: Stitching {} shard roots together...",
            pool_name,
            shard_roots.len()
        );

        // Sort by shard ID
        shard_roots.sort_by_key(|(id, _)| *id);

        if shard_roots.is_empty() {
            return Err(eyre!("No shards to compute root from"));
        }

        // Stitch shard roots together (same logic as verifier)
        // Continue building tree until we reach level 32 (fixed tree depth)
        let mut nodes: Vec<[u8; 32]> = shard_roots.iter().map(|(_, root)| *root).collect();
        let mut level = 16u8;

        println!(
            "  {} root: Starting with {} nodes at level {}",
            pool_name,
            nodes.len(),
            level
        );
        while level < TARGET_LEVEL {
            let mut next_level = Vec::with_capacity((nodes.len() + 1) / 2);

            for chunk in nodes.chunks(2) {
                let combined = if chunk.len() == 2 {
                    match pool {
                        Pool::Orchard => {
                            let left = Option::<MerkleHashOrchard>::from(
                                MerkleHashOrchard::from_bytes(&chunk[0]),
                            )
                            .ok_or_else(|| eyre!("Invalid Orchard shard root"))?;
                            let right = Option::<MerkleHashOrchard>::from(
                                MerkleHashOrchard::from_bytes(&chunk[1]),
                            )
                            .ok_or_else(|| eyre!("Invalid Orchard shard root"))?;
                            <MerkleHashOrchard as Hashable>::combine(
                                Level::new(level + 1),
                                &left,
                                &right,
                            )
                            .to_bytes()
                        }
                        Pool::Sapling => {
                            let left =
                                Option::<SaplingNode>::from(SaplingNode::from_bytes(chunk[0]))
                                    .ok_or_else(|| eyre!("Invalid Sapling shard root"))?;
                            let right =
                                Option::<SaplingNode>::from(SaplingNode::from_bytes(chunk[1]))
                                    .ok_or_else(|| eyre!("Invalid Sapling shard root"))?;
                            <SaplingNode as Hashable>::combine(Level::new(level + 1), &left, &right)
                                .to_bytes()
                        }
                    }
                } else {
                    // Odd number: pair with empty hash (not self-pairing!)
                    let empty_hash = match pool {
                        Pool::Orchard => {
                            <MerkleHashOrchard as Hashable>::empty_root(Level::new(level))
                                .to_bytes()
                        }
                        Pool::Sapling => {
                            <SaplingNode as Hashable>::empty_root(Level::new(level)).to_bytes()
                        }
                    };

                    match pool {
                        Pool::Orchard => {
                            let x = Option::<MerkleHashOrchard>::from(
                                MerkleHashOrchard::from_bytes(&chunk[0]),
                            )
                            .ok_or_else(|| eyre!("Invalid Orchard shard root"))?;
                            let empty = Option::<MerkleHashOrchard>::from(
                                MerkleHashOrchard::from_bytes(&empty_hash),
                            )
                            .ok_or_else(|| eyre!("Invalid empty hash"))?;
                            <MerkleHashOrchard as Hashable>::combine(
                                Level::new(level + 1),
                                &x,
                                &empty,
                            )
                            .to_bytes()
                        }
                        Pool::Sapling => {
                            let x = Option::<SaplingNode>::from(SaplingNode::from_bytes(chunk[0]))
                                .ok_or_else(|| eyre!("Invalid Sapling shard root"))?;
                            let empty =
                                Option::<SaplingNode>::from(SaplingNode::from_bytes(empty_hash))
                                    .ok_or_else(|| eyre!("Invalid empty hash"))?;
                            <SaplingNode as Hashable>::combine(Level::new(level + 1), &x, &empty)
                                .to_bytes()
                        }
                    }
                };
                next_level.push(combined);
            }

            nodes = next_level;
            level += 1;
            if level % 5 == 0 || nodes.len() <= 10 {
                println!(
                    "  {} root: Level {} complete, {} nodes remaining",
                    pool_name,
                    level,
                    nodes.len()
                );
            }
        }

        println!(
            "  {} root: Final root computed at level {}",
            pool_name, level
        );
        let pool_root = nodes[0];

        // Now compute paths for each shard root
        // Rebuild the tree structure and track paths
        let mut shard_paths: std::collections::HashMap<u32, Vec<[u8; 32]>> =
            std::collections::HashMap::new();

        // Sort shard roots by ID (clone first since we need original for path computation)
        let mut sorted_shard_roots = shard_roots.clone();
        sorted_shard_roots.sort_by_key(|(id, _)| *id);

        // Initialize paths for each shard (empty initially)
        for (shard_id, _) in &sorted_shard_roots {
            shard_paths.insert(*shard_id, Vec::new());
        }

        // Build tree and track paths
        // Track ALL shard IDs that each node represents (not just one)
        let mut current_level: Vec<(Vec<u32>, [u8; 32])> = sorted_shard_roots
            .into_iter()
            .map(|(id, hash)| (vec![id], hash))
            .collect();
        let mut level = 16u8;

        // Continue building tree until we reach the target level (32)
        while level < TARGET_LEVEL {
            let mut next_level = Vec::new();

            for chunk in current_level.chunks(2) {
                if chunk.len() == 2 {
                    let (left_ids, left_hash) = &chunk[0];
                    let (right_ids, right_hash) = &chunk[1];

                    // Debug: log shard-level siblings during path construction
                    if level < 20 || (left_ids.contains(&754) || right_ids.contains(&754)) {
                        println!(
                            "Level {}: left_ids={:?}, right_ids={:?}, left_hash={}, right_hash={}",
                            level,
                            left_ids,
                            right_ids,
                            hex::encode(left_hash),
                            hex::encode(right_hash)
                        );
                    }

                    let combined = match pool {
                        Pool::Orchard => {
                            let left = Option::<MerkleHashOrchard>::from(
                                MerkleHashOrchard::from_bytes(left_hash),
                            )
                            .ok_or_else(|| eyre!("Invalid Orchard shard root"))?;
                            let right = Option::<MerkleHashOrchard>::from(
                                MerkleHashOrchard::from_bytes(right_hash),
                            )
                            .ok_or_else(|| eyre!("Invalid Orchard shard root"))?;
                            <MerkleHashOrchard as Hashable>::combine(
                                Level::new(level + 1),
                                &left,
                                &right,
                            )
                            .to_bytes()
                        }
                        Pool::Sapling => {
                            let left =
                                Option::<SaplingNode>::from(SaplingNode::from_bytes(*left_hash))
                                    .ok_or_else(|| eyre!("Invalid Sapling shard root"))?;
                            let right =
                                Option::<SaplingNode>::from(SaplingNode::from_bytes(*right_hash))
                                    .ok_or_else(|| eyre!("Invalid Sapling shard root"))?;
                            <SaplingNode as Hashable>::combine(Level::new(level + 1), &left, &right)
                                .to_bytes()
                        }
                    };

                    // Add sibling to ALL shards in left subtree and right subtree
                    for left_id in left_ids {
                        if let Some(path) = shard_paths.get_mut(left_id) {
                            // Shard is in left chunk, so it's on the left at this level
                            // Need right sibling for reconstruction
                            path.push(*right_hash);
                        }
                    }
                    for right_id in right_ids {
                        if let Some(path) = shard_paths.get_mut(right_id) {
                            // Shard is in right chunk, so it's on the right at this level
                            // Need left sibling for reconstruction
                            path.push(*left_hash);
                        }
                    }

                    // Merge the shard ID lists for the combined node
                    let mut merged_ids = left_ids.clone();
                    merged_ids.extend(right_ids.clone());
                    next_level.push((merged_ids, combined));
                } else {
                    // Odd number: pair with empty hash (not self-pairing!)
                    let (last_ids, last_hash) = &chunk[0];

                    // Compute empty hash for this level
                    let empty_hash = match pool {
                        Pool::Orchard => {
                            <MerkleHashOrchard as Hashable>::empty_root(Level::new(level))
                                .to_bytes()
                        }
                        Pool::Sapling => {
                            <SaplingNode as Hashable>::empty_root(Level::new(level)).to_bytes()
                        }
                    };

                    // Debug: log empty hash used for odd-node padding
                    if level < 20 || last_ids.contains(&754) {
                        println!(
                            "Level {}: odd node {:?}, empty_sibling={}",
                            level,
                            last_ids,
                            hex::encode(&empty_hash)
                        );
                    }

                    let combined = match pool {
                        Pool::Orchard => {
                            let x = Option::<MerkleHashOrchard>::from(
                                MerkleHashOrchard::from_bytes(last_hash),
                            )
                            .ok_or_else(|| eyre!("Invalid Orchard shard root"))?;
                            let empty = Option::<MerkleHashOrchard>::from(
                                MerkleHashOrchard::from_bytes(&empty_hash),
                            )
                            .ok_or_else(|| eyre!("Invalid empty hash"))?;
                            <MerkleHashOrchard as Hashable>::combine(
                                Level::new(level + 1),
                                &x,
                                &empty,
                            )
                            .to_bytes()
                        }
                        Pool::Sapling => {
                            let x =
                                Option::<SaplingNode>::from(SaplingNode::from_bytes(*last_hash))
                                    .ok_or_else(|| eyre!("Invalid Sapling shard root"))?;
                            let empty =
                                Option::<SaplingNode>::from(SaplingNode::from_bytes(empty_hash))
                                    .ok_or_else(|| eyre!("Invalid empty hash"))?;
                            <SaplingNode as Hashable>::combine(Level::new(level + 1), &x, &empty)
                                .to_bytes()
                        }
                    };

                    // Add empty hash as sibling to ALL shards in this subtree
                    for last_id in last_ids {
                        if let Some(path) = shard_paths.get_mut(last_id) {
                            path.push(empty_hash);
                        }
                    }

                    next_level.push((last_ids.clone(), combined));
                }
            }

            current_level = next_level;
            level += 1;
        }

        // Verify all paths have exactly 16 nodes (from level 16 to level 32)
        for (shard_id, path) in &shard_paths {
            if path.len() != 16 {
                return Err(eyre!(
                    "Shard {} path has {} nodes, expected 16",
                    shard_id,
                    path.len()
                ));
            }
        }

        Ok((pool_root, shard_paths))
    }

    /// Update the SNAPSHOT constant in crates/lib/src/lib.rs with new values
    fn update_snapshot_constant(
        height: u32,
        orchard_root: [u8; 32],
        sapling_root: [u8; 32],
        orchard_count: u64,
        sapling_count: u64,
    ) -> Result<()> {
        use std::env;

        // Find workspace root by looking for Cargo.toml
        let current_dir = env::current_dir()?;
        let mut workspace_root = current_dir.clone();

        // Walk up to find workspace root
        loop {
            let cargo_toml = workspace_root.join("Cargo.toml");
            if cargo_toml.exists() {
                // Check if this is the workspace root (has [workspace] or multiple crates)
                let cargo_content = fs::read_to_string(&cargo_toml)?;
                if cargo_content.contains("[workspace]") || workspace_root.join("crates").exists() {
                    break;
                }
            }
            match workspace_root.parent() {
                Some(parent) => workspace_root = parent.to_path_buf(),
                None => return Err(eyre!("Could not find workspace root")),
            }
        }

        let lib_rs_path = workspace_root.join("crates/lib/src/lib.rs");
        if !lib_rs_path.exists() {
            return Err(eyre!("lib.rs not found at {}", lib_rs_path.display()));
        }

        // Read current file
        let mut content = fs::read_to_string(&lib_rs_path)?;

        // Find and replace individual fields using simple string replacement
        let orchard_root_hex = hex::encode(orchard_root);
        let sapling_root_hex = hex::encode(sapling_root);

        // Replace height
        if let Some(pos) = content.find("height:") {
            if let Some(end) = content[pos..].find(',') {
                let new_height = format!("height: {}", height);
                content.replace_range(pos..pos + end, &new_height);
            }
        }

        // Replace sapling_root
        if let Some(pos) = content.find("sapling_root: hex!(\"") {
            if let Some(end) = content[pos..].find("\")") {
                let end_pos = pos + end + 2; // Include the closing ")
                let new_sapling = format!("sapling_root: hex!(\"{}\")", sapling_root_hex);
                content.replace_range(pos..end_pos, &new_sapling);
            }
        }

        // Replace orchard_root
        if let Some(pos) = content.find("orchard_root: hex!(\"") {
            if let Some(end) = content[pos..].find("\")") {
                let end_pos = pos + end + 2; // Include the closing ")
                let new_orchard = format!("orchard_root: hex!(\"{}\")", orchard_root_hex);
                content.replace_range(pos..end_pos, &new_orchard);
            }
        }

        // Replace sapling_count
        if let Some(pos) = content.find("sapling_count:") {
            if let Some(end) = content[pos..].find(',') {
                let end_pos = pos + end;
                let new_count = format!("sapling_count: {}", sapling_count);
                content.replace_range(pos..end_pos, &new_count);
            }
        }

        // Replace orchard_count
        if let Some(pos) = content.find("orchard_count:") {
            if let Some(end) = content[pos..].find(',') {
                let end_pos = pos + end;
                let new_count = format!("orchard_count: {}", orchard_count);
                content.replace_range(pos..end_pos, &new_count);
            }
        }

        fs::write(&lib_rs_path, content)?;

        Ok(())
    }

    /// Stitch shard roots together into a pool root (backwards compatible)
    pub fn stitch_pool_root(pool: Pool, shard_roots: Vec<(u32, [u8; 32])>) -> Result<[u8; 32]> {
        Ok(Self::stitch_pool_root_with_paths(pool, shard_roots)?.0)
    }

    /// Compute pool root from commitments (parallelized version)
    ///
    /// This builds shards (matching the snapshot format) and stitches their roots together.
    /// This ensures the computed root matches what the verifier will compute from shard files.
    ///
    /// NOTE: This is only used when roots are not already computed from build_pool_shards.
    /// Prefer using stitch_pool_root with roots from build_pool_shards to avoid duplicate work.
    pub fn compute_pool_root(pool: Pool, commitments: &[(u64, [u8; 32])]) -> Result<[u8; 32]> {
        use incrementalmerkletree::{Hashable, Level};
        use orchard::tree::MerkleHashOrchard;
        use sapling::Node as SaplingNode;
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        let pool_name = match pool {
            Pool::Orchard => "Orchard",
            Pool::Sapling => "Sapling",
        };

        println!(
            "  Computing {} root from {} commitments...",
            pool_name,
            commitments.len()
        );

        if commitments.is_empty() {
            println!("  {} pool is empty, returning empty root", pool_name);
            let root_level = Level::new(32);
            return Ok(match pool {
                Pool::Orchard => <MerkleHashOrchard as Hashable>::empty_root(root_level).to_bytes(),
                Pool::Sapling => <SaplingNode as Hashable>::empty_root(root_level).to_bytes(),
            });
        }

        // Group commitments by shard (same logic as build_pool_shards)
        println!(
            "  Grouping {} commitments into shards...",
            commitments.len()
        );
        let mut by_shard: AHashMap<u32, Vec<(u64, [u8; 32])>> = AHashMap::new();
        for (position, commitment) in commitments {
            let shard_id = shard_id_from_position(*position, pool);
            by_shard
                .entry(shard_id)
                .or_insert_with(Vec::new)
                .push((*position, *commitment));
        }
        println!("  Grouped into {} shards", by_shard.len());

        let empty_node = match pool {
            Pool::Orchard => *EMPTY_ORCHARD_LEAF,
            Pool::Sapling => *EMPTY_SAPLING_LEAF,
        };

        let total_shards = by_shard.len();
        println!(
            "  Computing {} root: processing {} shards in parallel...",
            pool_name, total_shards
        );

        // Move into Vec so rayon can parallelize easily
        let shards_vec: Vec<(u32, Vec<(u64, [u8; 32])>)> = by_shard.into_iter().collect();

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_for_thread = counter.clone();
        let pool_name_for_thread = pool_name.to_string();

        // Background progress printer
        let progress_handle = std::thread::spawn(move || {
            let start = std::time::Instant::now();
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let processed = counter_for_thread.load(Ordering::Relaxed);
                if processed >= total_shards {
                    break;
                }
                let elapsed = start.elapsed().as_secs_f64();
                let pct = processed as f64 * 100.0 / total_shards as f64;
                let shards_per_sec = if elapsed > 0.0 {
                    processed as f64 / elapsed
                } else {
                    0.0
                };
                let remaining = total_shards - processed;
                let eta = if shards_per_sec > 0.0 {
                    remaining as f64 / shards_per_sec
                } else {
                    0.0
                };
                println!(
                    "  {} root: {}/{} shards processed ({:.1}%) | {:.1} shards/sec | {} remaining | ETA: {:.0}s",
                    pool_name_for_thread,
                    processed,
                    total_shards,
                    pct,
                    shards_per_sec,
                    remaining,
                    eta,
                );
            }
        });

        // Compute shard roots in parallel
        let shard_roots: Vec<(u32, [u8; 32])> = shards_vec
            .into_par_iter()
            .map(
                |(shard_id, mut shard_commitments)| -> Result<(u32, [u8; 32])> {
                    shard_commitments.sort_by_key(|(pos, _)| *pos);
                    let start_pos = shard_start_position(shard_id, pool);
                    let mut nodes = vec![empty_node; SHARD_SIZE as usize];

                    for (pos, commitment) in shard_commitments.iter() {
                        let pos_in_shard = (pos - start_pos) as usize;
                        if pos_in_shard < SHARD_SIZE as usize {
                            let node = commitment_to_node(pool, commitment)?;
                            nodes[pos_in_shard] = node;
                        }
                    }

                    let levels = build_tree_levels_pool(&nodes, pool)?;
                    if levels.len() <= 16 || levels[16].is_empty() {
                        return Err(eyre!("Failed to compute shard {} root", shard_id));
                    }
                    let shard_root = levels[16][0];
                    counter.fetch_add(1, Ordering::Relaxed);
                    Ok((shard_id, shard_root))
                },
            )
            .collect::<Result<Vec<_>>>()?;

        // Stop progress thread
        counter.store(total_shards, Ordering::Relaxed);
        progress_handle.join().ok();

        // Stitch shard roots together
        Self::stitch_pool_root(pool, shard_roots)
    }
}
