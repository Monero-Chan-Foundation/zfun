use clap::Parser;
use eyre::{Result, ensure, eyre};
use sha2::{Digest, Sha256};
use shard_builder::{Pool, ShardFile, shard_end_position, shard_start_position};
use std::path::PathBuf;

/// Build shard files for Zcash shielded commitment trees
///
/// NOTE: This tool currently uses SHA-256 for tree construction, which is INCORRECT
/// for real Orchard/Sapling commitments. Orchard requires Sinsemilla/Poseidon hashing
/// and Sapling uses Pedersen hashing. This tool is SYNTHETIC-ONLY and should not be
/// used to build shards from real commitments until proper hash functions are integrated.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Pool type: orchard or sapling
    #[arg(long)]
    pool: String,

    /// Shard ID (u32)
    #[arg(long)]
    shard_id: u32,

    /// Output file path
    #[arg(long)]
    output: PathBuf,

    /// Verify the shard after building (recompute root and compare)
    #[arg(long, default_value = "false")]
    verify: bool,

    /// Input file with commitments (32 bytes each, one per line as hex)
    /// If not provided, will generate a synthetic shard for testing.
    /// WARNING: Even with real commitments, this tool uses SHA-256 which will
    /// produce incorrect roots/paths that won't match consensus trees.
    #[arg(long)]
    commitments_file: Option<PathBuf>,
}

type Node = [u8; 32];

fn main() -> Result<()> {
    let args = Args::parse();

    let pool: Pool = args.pool.parse()?;
    let shard_id = args.shard_id;
    let start_position = shard_start_position(shard_id, pool);
    let end_position = shard_end_position(shard_id, pool);
    let shard_size = pool.shard_size();

    println!("Building shard:");
    println!("  Pool: {:?}", pool);
    println!("  Shard ID: {}", shard_id);
    println!("  Start position: {}", start_position);
    println!("  End position: {}", end_position);
    println!("  Shard size: {}", shard_size);
    println!("  Output: {}", args.output.display());
    println!("\n⚠️  WARNING: This tool uses SHA-256 for tree construction.");
    println!("   For real commitments, Orchard requires Sinsemilla/Poseidon");
    println!("   and Sapling requires Pedersen hashing. This is SYNTHETIC-ONLY.");

    // Load or generate commitments
    let commitments = if let Some(ref commitments_file) = args.commitments_file {
        load_commitments_from_file(commitments_file)?
    } else {
        println!("No commitments file provided, generating synthetic shard for testing");
        generate_synthetic_commitments(shard_size as usize)
    };

    let leaf_count = commitments.len() as u32;
    ensure!(
        leaf_count <= shard_size as u32,
        "Too many commitments: {} > {}",
        leaf_count,
        shard_size
    );

    println!("  Leaf count: {}", leaf_count);

    // Build tree levels
    let levels = build_tree_levels(&commitments)?;

    // Create shard file
    let shard = ShardFile::create(
        &args.output,
        pool,
        shard_id,
        start_position,
        end_position,
        leaf_count,
        &levels,
    )?;

    println!("Shard file created successfully");

    // Verify if requested
    if args.verify {
        verify_shard(&shard, pool)?;
        println!("Verification passed");
    }

    Ok(())
}

fn load_commitments_from_file(path: &PathBuf) -> Result<Vec<Node>> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut commitments = Vec::new();

    for (line_num, line) in reader.lines().enumerate() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let bytes =
            hex::decode(line).map_err(|e| eyre!("Invalid hex on line {}: {}", line_num + 1, e))?;

        ensure!(
            bytes.len() == 32,
            "Commitment on line {} must be 32 bytes, got {}",
            line_num + 1,
            bytes.len()
        );

        let mut node = [0u8; 32];
        node.copy_from_slice(&bytes);
        commitments.push(node);
    }

    Ok(commitments)
}

fn generate_synthetic_commitments(count: usize) -> Vec<Node> {
    let mut commitments = Vec::with_capacity(count);
    for i in 0..count {
        let mut node = [0u8; 32];
        // Use a simple pattern: first 8 bytes are the index, rest are zeros
        node[0..8].copy_from_slice(&(i as u64).to_le_bytes());
        commitments.push(node);
    }
    commitments
}

/// Build all tree levels from leaves
/// Returns levels from leaves (level 0) to root (last level)
fn build_tree_levels(leaves: &[Node]) -> Result<Vec<Vec<Node>>> {
    let mut levels = Vec::new();

    if leaves.is_empty() {
        return Ok(levels);
    }

    // Level 0: leaves
    levels.push(leaves.to_vec());

    // Build subsequent levels
    let mut current_level = leaves.to_vec();
    while current_level.len() > 1 {
        let mut next_level = Vec::new();

        // Hash pairs
        for i in 0..(current_level.len() / 2) {
            let left = &current_level[i * 2];
            let right = &current_level[i * 2 + 1];
            let parent = hash_pair(left, right);
            next_level.push(parent);
        }

        // Handle odd node at end
        if current_level.len() % 2 == 1 {
            let last = &current_level[current_level.len() - 1];
            let parent = hash_pair(last, last); // Hash with itself
            next_level.push(parent);
        }

        levels.push(next_level.clone());
        current_level = next_level;
    }

    Ok(levels)
}

/// Hash two nodes together
///
/// WARNING: This uses SHA-256 which is INCORRECT for real Orchard/Sapling commitments.
/// - Orchard requires Sinsemilla/Poseidon hashing via MerkleHashOrchard::combine()
/// - Sapling requires Pedersen hashing via SaplingNode::combine()
///
/// This function is only valid for synthetic/test data.
fn hash_pair(left: &Node, right: &Node) -> Node {
    let mut hasher = Sha256::new();
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().into()
}

/// Verify shard by recomputing root and comparing
fn verify_shard(shard: &ShardFile, _pool: Pool) -> Result<()> {
    println!("Verifying shard...");

    let header = shard.header();
    let leaf_count = header.leaf_count as usize;

    if leaf_count == 0 {
        println!("Shard is empty, skipping verification");
        return Ok(());
    }

    // Rebuild tree from stored data
    let mut levels = Vec::new();

    // Read level 0 (leaves)
    let mut leaves = Vec::new();
    for i in 0..leaf_count {
        leaves.push(shard.read_node(0, i)?);
    }
    levels.push(leaves);

    // Build remaining levels
    let mut current_level = levels[0].clone();
    while current_level.len() > 1 {
        let mut next_level = Vec::new();
        for i in 0..(current_level.len() / 2) {
            let left = &current_level[i * 2];
            let right = &current_level[i * 2 + 1];
            next_level.push(hash_pair(left, right));
        }
        if current_level.len() % 2 == 1 {
            let last = &current_level[current_level.len() - 1];
            next_level.push(hash_pair(last, last));
        }
        levels.push(next_level.clone());
        current_level = next_level;
    }

    // Compare root
    let computed_root = &levels[levels.len() - 1][0];
    let stored_root = shard.root()?;

    ensure!(
        computed_root == &stored_root,
        "Root mismatch: computed {:?}, stored {:?}",
        hex::encode(computed_root),
        hex::encode(&stored_root)
    );

    println!("Root matches: {}", hex::encode(computed_root));

    // Verify a few paths
    let test_positions = vec![0, leaf_count / 2, leaf_count - 1];
    for pos in test_positions {
        if pos < leaf_count {
            let path = shard.extract_path(pos as u64)?;
            println!("  Path for position {}: {} siblings", pos, path.len());
        }
    }

    Ok(())
}
