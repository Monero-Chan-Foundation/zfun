//! CLI for building snapshots from Zebra state database or synthetic data
//!
//! Usage:
//!   - With Zebra: cargo run --bin snapshot_build --features zebra -- --zebra-cache-dir <path> --height <height> ...
//!   - Synthetic: cargo run --bin snapshot_build -- --height <height> --orchard-count <n> --sapling-count <n> ...

#[cfg(not(target_arch = "wasm32"))]
#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

use clap::Parser;
use eyre::{Result, eyre};
use std::path::PathBuf;
use std::time::Instant;

use shard_builder::SnapshotBuilder;

#[cfg(feature = "zebra")]
use shard_builder::zebra_adapter;
#[cfg(feature = "zebra")]
use zebra_state::ZebraDb;

/// Build a snapshot from Zebra state database or synthetic data
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Target block height
    #[arg(long)]
    height: u32,

    /// Output directory for snapshot
    #[arg(long)]
    output_dir: PathBuf,

    /// Block hash (hex string)
    /// If not provided and using Zebra, will be fetched from Zebra DB
    #[arg(long)]
    block_hash: Option<String>,

    /// Path to Zebra cache directory (enables Zebra adapter)
    #[cfg(feature = "zebra")]
    #[arg(long)]
    zebra_cache_dir: Option<PathBuf>,

    /// Network (mainnet or testnet). Defaults to mainnet.
    #[cfg(feature = "zebra")]
    #[arg(long, default_value = "mainnet")]
    network: String,

    /// Number of synthetic Orchard commitments to generate (only used if --zebra-cache-dir not provided)
    #[arg(long, default_value = "1000")]
    orchard_count: u64,

    /// Number of synthetic Sapling commitments to generate (only used if --zebra-cache-dir not provided)
    #[arg(long, default_value = "1000")]
    sapling_count: u64,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let overall_start = Instant::now();

    println!("Building snapshot for height {}...", args.height);
    println!("Output: {}", args.output_dir.display());

    // Phase 1: Fetch commitments
    println!("\n=== Phase 1: Fetching Commitments ===");
    let fetch_start = Instant::now();
    let (orchard_commitments, sapling_commitments, block_hash) = fetch_commitments(&args)?;
    let fetch_time = fetch_start.elapsed();

    println!("\n=== Phase 1: Fetch Complete ===");
    println!("  Time: {:.1}s", fetch_time.as_secs_f64());
    println!("  Orchard: {} commitments", orchard_commitments.len());
    println!("  Sapling: {} commitments", sapling_commitments.len());

    // Phase 2: Build snapshot (which computes roots from shard roots)
    println!("\n=== Phase 2: Building Snapshot ===");
    let build_start = Instant::now();
    let builder = SnapshotBuilder::new(args.height, block_hash.clone(), args.output_dir.clone());

    // Build snapshot - this now computes roots from shard roots (no duplicate work)
    let (orchard_root, sapling_root) =
        builder.build_snapshot_with_roots(&orchard_commitments, &sapling_commitments)?;
    let build_time = build_start.elapsed();

    println!("  Time: {:.1}s", build_time.as_secs_f64());
    println!("  Orchard root: {}", hex::encode(orchard_root));
    println!("  Sapling root: {}", hex::encode(sapling_root));

    // Final summary
    let total_time = overall_start.elapsed();
    println!("\n=== Snapshot Build Complete ===");
    println!(
        "  Phase 1 (Fetch):       {:.1}s ({:.1}%)",
        fetch_time.as_secs_f64(),
        fetch_time.as_secs_f64() / total_time.as_secs_f64() * 100.0
    );
    println!(
        "  Phase 2 (Build):       {:.1}s ({:.1}%)",
        build_time.as_secs_f64(),
        build_time.as_secs_f64() / total_time.as_secs_f64() * 100.0
    );
    println!("  Total:                 {:.1}s", total_time.as_secs_f64());
    println!(
        "  Throughput:            {:.1} blocks/sec",
        args.height as f64 / total_time.as_secs_f64()
    );

    Ok(())
}

#[cfg(feature = "zebra")]
fn fetch_commitments(args: &Args) -> Result<(Vec<(u64, [u8; 32])>, Vec<(u64, [u8; 32])>, String)> {
    if let Some(ref zebra_cache_dir) = args.zebra_cache_dir {
        // Use Zebra adapter
        println!("Opening Zebra database at {}...", zebra_cache_dir.display());

        use std::sync::Arc;
        use zebra_chain::parameters::Network;
        use zebra_state::{Config, constants};

        // Parse network - Network is an enum
        // Mainnet is a unit variant, Testnet requires Parameters
        let network: Network = match args.network.to_lowercase().as_str() {
            "mainnet" => Network::Mainnet,
            "testnet" => {
                use zebra_chain::parameters::testnet;
                Network::Testnet(Arc::new(testnet::Parameters::default()))
            }
            _ => {
                return Err(eyre!(
                    "Invalid network: {}. Must be 'mainnet' or 'testnet'",
                    args.network
                ));
            }
        };

        // OPTIMIZATION: Configure RocksDB for maximum read performance
        unsafe {
            std::env::set_var("ROCKSDB_MAX_OPEN_FILES", "100000");
            std::env::set_var("ROCKSDB_BLOCK_CACHE_SIZE", "8589934592");
        }

        // Create Config with the specified cache directory
        let mut config = Config::default();
        config.cache_dir = zebra_cache_dir.clone();

        // OPTIMIZATION: Enable direct I/O for sequential scans
        // Note: These settings may need to be applied via RocksDB config if exposed by Zebra
        // config.use_direct_io_for_flush_and_compaction = true;
        // config.use_direct_reads = true;

        // Zebra 2.5.0+ uses "state" instead of "finalized"
        let db_kind = "state";
        // Get format version from Zebra's constants
        let format_version = constants::state_database_format_version_in_code();
        let column_families = vec![]; // Empty - ZebraDb will handle existing CFs

        let db = Arc::new(ZebraDb::new(
            &config,
            db_kind,
            &format_version,
            &network,
            false, // debug_skip_format_upgrades
            column_families,
            true, // read_only
        ));

        // Fetch block hash if not provided
        let block_hash = if let Some(ref hash) = args.block_hash {
            hash.clone()
        } else {
            println!("Fetching block hash from Zebra...");
            zebra_adapter::get_block_hash(&db, args.height)?
        };

        println!(
            "Fetching commitments from Zebra (height {})...",
            args.height
        );
        let (orchard, sapling) = zebra_adapter::fetch_all_commitments_to_height(&db, args.height)?;

        Ok((orchard, sapling, block_hash))
    } else {
        // Use synthetic data
        Ok(generate_synthetic_data(args))
    }
}

#[cfg(not(feature = "zebra"))]
fn fetch_commitments(args: &Args) -> Result<(Vec<(u64, [u8; 32])>, Vec<(u64, [u8; 32])>, String)> {
    // Synthetic data only
    Ok(generate_synthetic_data(args))
}

fn generate_synthetic_data(args: &Args) -> (Vec<(u64, [u8; 32])>, Vec<(u64, [u8; 32])>, String) {
    println!("NOTE: Using synthetic data (real adapters not yet implemented)");
    println!(
        "\nGenerating {} Orchard and {} Sapling synthetic commitments...",
        args.orchard_count, args.sapling_count
    );

    let mut orchard_commitments = Vec::new();
    let mut sapling_commitments = Vec::new();

    // Generate synthetic Orchard commitments
    for i in 0..args.orchard_count {
        let mut node = [0u8; 32];
        node[0..8].copy_from_slice(&(i as u64).to_le_bytes());
        node[8..15].copy_from_slice(b"ORCHARD"); // 7 bytes
        orchard_commitments.push((i, node));
    }

    // Generate synthetic Sapling commitments
    for i in 0..args.sapling_count {
        let mut node = [0u8; 32];
        node[0..8].copy_from_slice(&(i as u64).to_le_bytes());
        node[8..15].copy_from_slice(b"SAPLING"); // 7 bytes
        sapling_commitments.push((i, node));
    }

    let block_hash = args.block_hash.clone().unwrap_or_else(|| {
        "0000000000000000000000000000000000000000000000000000000000000000".to_string()
    });

    (orchard_commitments, sapling_commitments, block_hash)
}
