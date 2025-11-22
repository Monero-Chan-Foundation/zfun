use blake2b_simd;
use clap::Parser;
use eyre::{Result, ensure, eyre};
use hex;
use shard_builder::{Pool, ShardFile, SnapshotMetadata};
use std::convert::TryInto;
use std::path::PathBuf;

/// Verify a snapshot by checking file hashes and recomputing roots
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to snapshot directory (containing metadata.json)
    #[arg(short, long)]
    snapshot_dir: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();

    println!("Verifying snapshot at: {}", args.snapshot_dir.display());

    // Load metadata
    let metadata_path = args.snapshot_dir.join("metadata.json");
    ensure!(
        metadata_path.exists(),
        "metadata.json not found at {}",
        metadata_path.display()
    );

    let metadata_json = std::fs::read_to_string(&metadata_path)?;
    let metadata: SnapshotMetadata = serde_json::from_str(&metadata_json)
        .map_err(|e| eyre!("Failed to parse metadata.json: {}", e))?;

    println!("Snapshot height: {}", metadata.snapshot_height);
    println!("Block hash: {}", metadata.block_hash);
    println!("Orchard count: {}", metadata.orchard_count);
    println!("Sapling count: {}", metadata.sapling_count);
    println!("Complete: {}", metadata.complete);

    if !metadata.complete {
        return Err(eyre!(
            "Snapshot is marked as incomplete. Do not use for proofs."
        ));
    }

    // Verify all shard files
    let mut orchard_shard_roots = Vec::new();
    let mut sapling_shard_roots = Vec::new();

    println!("\nVerifying Orchard shards...");
    if let Some(orchard_shards) = metadata.shards.get("orchard") {
        for (shard_id, shard_info) in orchard_shards {
            let shard_path = args.snapshot_dir.join(&shard_info.file);

            ensure!(
                shard_path.exists(),
                "Shard file not found: {}",
                shard_path.display()
            );

            // Verify file size
            let actual_size = std::fs::metadata(&shard_path)?.len();
            ensure!(
                actual_size == shard_info.size,
                "Shard {} size mismatch: expected {}, got {}",
                shard_id,
                shard_info.size,
                actual_size
            );

            // Verify file hash
            let file_bytes = std::fs::read(&shard_path)?;
            let hash = blake2b_simd::Params::new()
                .hash_length(32)
                .hash(&file_bytes);
            let hash_hex = hex::encode(hash.as_bytes());

            ensure!(
                hash_hex == shard_info.blake2b,
                "Shard {} hash mismatch: expected {}, got {}",
                shard_id,
                shard_info.blake2b,
                hash_hex
            );

            // Read shard and get root
            let shard = ShardFile::open(&shard_path, Pool::Orchard)?;
            if shard.header().leaf_count > 0 {
                let root = shard.root()?;
                orchard_shard_roots.push((shard_id.parse::<u32>()?, root));
            }

            println!(
                "  ✓ Shard {}: {} bytes, hash matches",
                shard_id, shard_info.size
            );
        }
    }

    println!("\nVerifying Sapling shards...");
    if let Some(sapling_shards) = metadata.shards.get("sapling") {
        for (shard_id, shard_info) in sapling_shards {
            let shard_path = args.snapshot_dir.join(&shard_info.file);

            ensure!(
                shard_path.exists(),
                "Shard file not found: {}",
                shard_path.display()
            );

            // Verify file size
            let actual_size = std::fs::metadata(&shard_path)?.len();
            ensure!(
                actual_size == shard_info.size,
                "Shard {} size mismatch: expected {}, got {}",
                shard_id,
                shard_info.size,
                actual_size
            );

            // Verify file hash
            let file_bytes = std::fs::read(&shard_path)?;
            let hash = blake2b_simd::Params::new()
                .hash_length(32)
                .hash(&file_bytes);
            let hash_hex = hex::encode(hash.as_bytes());

            ensure!(
                hash_hex == shard_info.blake2b,
                "Shard {} hash mismatch: expected {}, got {}",
                shard_id,
                shard_info.blake2b,
                hash_hex
            );

            // Read shard and get root
            let shard = ShardFile::open(&shard_path, Pool::Sapling)?;
            if shard.header().leaf_count > 0 {
                let root = shard.root()?;
                sapling_shard_roots.push((shard_id.parse::<u32>()?, root));
            }

            println!(
                "  ✓ Shard {}: {} bytes, hash matches",
                shard_id, shard_info.size
            );
        }
    }

    // Stitch shard roots to verify pool roots match metadata
    println!("\nVerifying pool roots...");

    // Sort shard roots by shard ID
    orchard_shard_roots.sort_by_key(|(id, _)| *id);
    sapling_shard_roots.sort_by_key(|(id, _)| *id);

    // Verify Orchard root
    let expected_orchard_root = hex::decode(&metadata.orchard_root)
        .map_err(|e| eyre!("Invalid orchard_root hex in metadata: {}", e))?;
    ensure!(
        expected_orchard_root.len() == 32,
        "Orchard root length mismatch: expected 32 bytes, got {}",
        expected_orchard_root.len()
    );

    if !orchard_shard_roots.is_empty() {
        let computed_orchard_root = stitch_shard_roots(Pool::Orchard, &orchard_shard_roots)?;
        ensure!(
            computed_orchard_root == expected_orchard_root,
            "Orchard root mismatch: computed {}, expected {}",
            hex::encode(&computed_orchard_root),
            metadata.orchard_root
        );
        println!(
            "  ✓ Orchard root matches: {}",
            hex::encode(&computed_orchard_root)
        );
    } else {
        // Verify empty tree root matches metadata
        let empty_orchard_root = get_empty_tree_root(Pool::Orchard)?;
        let expected_root_array: [u8; 32] = expected_orchard_root
            .try_into()
            .map_err(|_| eyre!("Expected Orchard root must be 32 bytes"))?;
        ensure!(
            empty_orchard_root == expected_root_array,
            "Empty Orchard root mismatch: computed {}, expected {}",
            hex::encode(&empty_orchard_root),
            metadata.orchard_root
        );
        println!(
            "  ✓ Empty Orchard root matches: {}",
            hex::encode(&empty_orchard_root)
        );
    }

    // Verify Sapling root
    let expected_sapling_root = hex::decode(&metadata.sapling_root)
        .map_err(|e| eyre!("Invalid sapling_root hex in metadata: {}", e))?;
    ensure!(
        expected_sapling_root.len() == 32,
        "Sapling root length mismatch: expected 32 bytes, got {}",
        expected_sapling_root.len()
    );

    if !sapling_shard_roots.is_empty() {
        let computed_sapling_root = stitch_shard_roots(Pool::Sapling, &sapling_shard_roots)?;
        ensure!(
            computed_sapling_root == expected_sapling_root,
            "Sapling root mismatch: computed {}, expected {}",
            hex::encode(&computed_sapling_root),
            metadata.sapling_root
        );
        println!(
            "  ✓ Sapling root matches: {}",
            hex::encode(&computed_sapling_root)
        );
    } else {
        // Verify empty tree root matches metadata
        let empty_sapling_root = get_empty_tree_root(Pool::Sapling)?;
        let expected_root_array: [u8; 32] = expected_sapling_root
            .try_into()
            .map_err(|_| eyre!("Expected Sapling root must be 32 bytes"))?;
        ensure!(
            empty_sapling_root == expected_root_array,
            "Empty Sapling root mismatch: computed {}, expected {}",
            hex::encode(&empty_sapling_root),
            metadata.sapling_root
        );
        println!(
            "  ✓ Empty Sapling root matches: {}",
            hex::encode(&empty_sapling_root)
        );
    }

    println!("\n✓ All shard files and pool roots verified successfully!");
    println!("  Orchard shards: {}", orchard_shard_roots.len());
    println!("  Sapling shards: {}", sapling_shard_roots.len());

    Ok(())
}

/// Stitch shard roots together to compute the pool root
/// Shards are combined in order using the appropriate hash function
fn stitch_shard_roots(pool: Pool, shard_roots: &[(u32, [u8; 32])]) -> Result<Vec<u8>> {
    use incrementalmerkletree::{Hashable, Level};
    use orchard::tree::MerkleHashOrchard;
    use sapling::Node as SaplingNode;

    if shard_roots.is_empty() {
        return Err(eyre!("Cannot stitch empty shard roots"));
    }

    // Convert shard roots to nodes
    let mut nodes: Vec<[u8; 32]> = shard_roots.iter().map(|(_, root)| *root).collect();

    // Build tree from shard roots
    // Shard roots are at level 16 (since each shard has 2^16 leaves, depth 16)
    // We need to combine them at level 17, 18, ... up to level 32
    let mut level = 16u8;

    while nodes.len() > 1 {
        let mut next_level = Vec::new();

        for i in 0..(nodes.len() / 2) {
            let left = &nodes[i * 2];
            let right = &nodes[i * 2 + 1];

            let combined = match pool {
                Pool::Orchard => {
                    let left_hash =
                        Option::<MerkleHashOrchard>::from(MerkleHashOrchard::from_bytes(left))
                            .ok_or_else(|| eyre!("Invalid Orchard shard root"))?;
                    let right_hash =
                        Option::<MerkleHashOrchard>::from(MerkleHashOrchard::from_bytes(right))
                            .ok_or_else(|| eyre!("Invalid Orchard shard root"))?;
                    <MerkleHashOrchard as Hashable>::combine(
                        Level::new(level + 1),
                        &left_hash,
                        &right_hash,
                    )
                    .to_bytes()
                }
                Pool::Sapling => {
                    let left_node = Option::<SaplingNode>::from(SaplingNode::from_bytes(*left))
                        .ok_or_else(|| eyre!("Invalid Sapling shard root"))?;
                    let right_node = Option::<SaplingNode>::from(SaplingNode::from_bytes(*right))
                        .ok_or_else(|| eyre!("Invalid Sapling shard root"))?;
                    <SaplingNode as Hashable>::combine(
                        Level::new(level + 1),
                        &left_node,
                        &right_node,
                    )
                    .to_bytes()
                }
            };
            next_level.push(combined);
        }

        // Handle odd node at end
        if nodes.len() % 2 == 1 {
            let last = &nodes[nodes.len() - 1];
            let combined = match pool {
                Pool::Orchard => {
                    let last_hash =
                        Option::<MerkleHashOrchard>::from(MerkleHashOrchard::from_bytes(last))
                            .ok_or_else(|| eyre!("Invalid Orchard shard root"))?;
                    <MerkleHashOrchard as Hashable>::combine(
                        Level::new(level + 1),
                        &last_hash,
                        &last_hash,
                    )
                    .to_bytes()
                }
                Pool::Sapling => {
                    let last_node = Option::<SaplingNode>::from(SaplingNode::from_bytes(*last))
                        .ok_or_else(|| eyre!("Invalid Sapling shard root"))?;
                    <SaplingNode as Hashable>::combine(
                        Level::new(level + 1),
                        &last_node,
                        &last_node,
                    )
                    .to_bytes()
                }
            };
            next_level.push(combined);
        }

        nodes = next_level;
        level += 1;
    }

    Ok(nodes[0].to_vec())
}

/// Get the empty tree root for a pool (tree with zero leaves)
/// This is the root at level 32 (the maximum depth)
fn get_empty_tree_root(pool: Pool) -> Result<[u8; 32]> {
    use incrementalmerkletree::{Hashable, Level};
    use orchard::tree::MerkleHashOrchard;
    use sapling::Node as SaplingNode;

    // Empty tree root is at level 32 (maximum depth)
    let root_level = Level::new(32);

    match pool {
        Pool::Orchard => {
            let empty_root = <MerkleHashOrchard as Hashable>::empty_root(root_level);
            Ok(empty_root.to_bytes())
        }
        Pool::Sapling => {
            let empty_root = <SaplingNode as Hashable>::empty_root(root_level);
            Ok(empty_root.to_bytes())
        }
    }
}
