mod block_processor;
mod merkle;
mod sorting;
mod utxo;

use block_processor::{ProcessedBlock, UtxoUpdate, process_block_sync};
use merkle::{build_merkle_tree_from_file, find_hash_index, generate_merkle_proof};
use sorting::external_sort;
use utxo::{merge_and_compute_utxos, sort_creates_in_chunks, sort_spends_in_chunks};

use std::{
    fs::{self, File},
    io::{BufWriter, Seek, Write},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use eyre::{Result, eyre};
use tokio::sync::{Mutex, mpsc};
use zebra_chain::{
    block::{Block, Height},
    parameters::Network,
};
use zebra_state::{HashOrHeight, ZebraDb, state_database_format_version_in_code};
use zfun_lib::SnapshotMetadata;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let cache_dir = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| eyre!("Usage: zfun-setup-script <zebra-cache-dir> <target-data-dir>"))?;

    let data_dir = args.next().map(PathBuf::from).ok_or_else(|| {
        eyre!("Usage: zfun-setup-script <zebra-cache-dir> <target-data-dir> [snapshot-height]")
    })?;

    // Optional snapshot height argument
    let snapshot_height_arg = args.next();

    // Ensure data directory is empty or doesn't exist
    if data_dir.exists() {
        let entries = fs::read_dir(&data_dir)?;
        if entries.count() > 0 {
            return Err(eyre!(
                "Target data directory '{}' exists and is not empty. Please remove it or choose a different directory.",
                data_dir.display()
            ));
        }
    } else {
        fs::create_dir_all(&data_dir)?;
    }

    println!("=== Startup & Configuration ===");
    println!("Data directory: {}", data_dir.display());
    println!("Zebra cache directory: {}", cache_dir.display());

    // System info
    if let Ok(metadata) = std::fs::metadata(&cache_dir) {
        println!("Zebra cache directory exists: {}", metadata.is_dir());
    }
    if let Ok(metadata) = std::fs::metadata(&data_dir) {
        println!("Data directory exists: {}", metadata.is_dir());
    }

    let config = zebra_state::Config {
        cache_dir: cache_dir.clone(),
        ..Default::default()
    };

    println!("\n=== Database Operations ===");
    println!("Opening Zebra database at {}...", cache_dir.display());
    let db_open_start = Instant::now();
    let db = Arc::new(ZebraDb::new(
        &config,
        "state",
        &state_database_format_version_in_code(),
        &Network::Mainnet,
        false,
        [],
        true,
    ));
    println!(
        "Database opened in {:.2}s",
        db_open_start.elapsed().as_secs_f64()
    );

    let tip_height = db
        .finalized_tip_height()
        .ok_or_else(|| eyre!("No finalized tip found"))?;
    println!("finalized tip height: {:?}", tip_height);

    const CONCURRENCY: usize = 64;
    const NUM_PROCESSORS: usize = 32;

    // Use snapshot height to match the snapshot shards
    // Default to 3136748 if not provided
    let snapshot_height = if let Some(height_str) = snapshot_height_arg {
        height_str
            .parse::<u32>()
            .map_err(|e| eyre!("Invalid snapshot height '{}': {}", height_str, e))?
    } else {
        3136748 // Default height
    };

    println!("\n=== Configuration Parameters ===");
    println!("Concurrency (block fetchers): {}", CONCURRENCY);
    println!("Processing workers: {}", NUM_PROCESSORS);
    println!("Snapshot height: {}", snapshot_height);
    println!("Zebra tip height: {}", tip_height.0);

    let start_time = Instant::now();
    let start = 0; // Process from block 0 to snapshot height
    let end = snapshot_height.min(tip_height.0); // Use snapshot height, but don't exceed tip
    let total_blocks = end - start + 1;

    println!("\n=== Processing Plan ===");
    println!(
        "Block range: {} to {} ({} blocks)",
        start, end, total_blocks
    );
    println!(
        "Estimated time (at ~100 blocks/s): {:.1} minutes",
        total_blocks as f64 / 100.0 / 60.0
    );

    if end < snapshot_height {
        return Err(eyre!(
            "Zebra tip height {} is less than snapshot height {}. Cannot build proof data.",
            tip_height.0,
            snapshot_height
        ));
    }

    println!("\n=== Database Query ===");
    println!("Querying block at height {} to verify availability...", end);
    let query_start = Instant::now();
    db.block(HashOrHeight::Height(Height(end))).unwrap();
    println!(
        "Block query completed in {:.2}ms",
        query_start.elapsed().as_secs_f64() * 1000.0
    );

    println!("\n=== Phase 1: Block Fetching & Processing ===");
    println!(
        "Starting {} block fetchers and {} processing workers...",
        CONCURRENCY, NUM_PROCESSORS
    );

    // Channel for fetched blocks: (height, Option<Block>)
    let (block_tx, mut block_rx) = mpsc::channel::<(u32, Option<Arc<Block>>)>(CONCURRENCY);

    let next_block = Arc::new(Mutex::new(start));

    let workers = (0..CONCURRENCY)
        .map(|worker_id| {
            let mut db = Arc::clone(&db);
            let tx = block_tx.clone();
            let next_block = next_block.clone();
            tokio::spawn(async move {
                let mut blocks_fetched = 0;
                let worker_start = Instant::now();
                loop {
                    let num = {
                        let mut num = next_block.lock().await;
                        if *num > end {
                            break;
                        }
                        let height = *num;
                        *num = height + 1;
                        height
                    };
                    let fetch_start = Instant::now();
                    let (block, old_db) = tokio::task::spawn_blocking(move || {
                        (db.block(HashOrHeight::Height(Height(num))), db)
                    })
                    .await
                    .unwrap();
                    let fetch_time = fetch_start.elapsed();
                    let block = block.unwrap();
                    blocks_fetched += 1;
                    if blocks_fetched % 10000 == 0 {
                        let rate = blocks_fetched as f64 / worker_start.elapsed().as_secs_f64();
                        println!(
                            "  [Fetcher {}] Fetched {} blocks ({:.0} blocks/s, avg {:.2}ms/block)",
                            worker_id,
                            blocks_fetched,
                            rate,
                            fetch_time.as_secs_f64() * 1000.0
                        );
                    }
                    let _ = tx.send((num, Some(block))).await;
                    db = old_db;
                }
                let total_time = worker_start.elapsed();
                let rate = blocks_fetched as f64 / total_time.as_secs_f64();
                println!(
                    "  [Fetcher {}] Completed: {} blocks in {:.2}s ({:.0} blocks/s)",
                    worker_id,
                    blocks_fetched,
                    total_time.as_secs_f64(),
                    rate
                );
            })
        })
        .collect::<Vec<_>>();
    drop(block_tx);

    // Use flume MPMC channel for blocks
    let (flume_tx, flume_rx) = flume::bounded::<(u32, Option<Arc<Block>>)>(CONCURRENCY);

    // Channel for processed blocks
    let (processed_tx, mut processed_rx) = mpsc::channel::<(u32, ProcessedBlock)>(CONCURRENCY);

    // Forward from tokio mpsc to flume
    tokio::spawn(async move {
        while let Some(msg) = block_rx.recv().await {
            let _ = flume_tx.send(msg);
        }
    });

    // Spawn processing workers
    let processing_workers = (0..NUM_PROCESSORS)
        .map(|worker_id| {
            let block_rx_clone = flume_rx.clone();
            let processed_tx = processed_tx.clone();

            let data_dir_clone = data_dir.clone();
            tokio::spawn(async move {
                println!("  [Processor {}] Starting...", worker_id);
                let worker_start = Instant::now();
                
                let creates_path =
                    data_dir_clone.join(format!("utxo_creates.worker{}.bin", worker_id));
                let spends_path =
                    data_dir_clone.join(format!("utxo_spends.worker{}.bin", worker_id));
                let nullifiers_path =
                    data_dir_clone.join(format!("nullifiers.worker{}.bin", worker_id));

                println!("  [Processor {}] Creating worker files:", worker_id);
                println!("    Creates: {}", creates_path.display());
                println!("    Spends: {}", spends_path.display());
                println!("    Nullifiers: {}", nullifiers_path.display());

                let mut creates_file = BufWriter::with_capacity(
                    16 * 1024 * 1024,
                    File::create(&creates_path).unwrap(),
                );
                let mut spends_file =
                    BufWriter::with_capacity(16 * 1024 * 1024, File::create(&spends_path).unwrap());
                let mut nullifiers_file = BufWriter::with_capacity(
                    16 * 1024 * 1024,
                    File::create(&nullifiers_path).unwrap(),
                );
                
                let mut total_creates = 0u64;
                let mut total_spends = 0u64;
                let mut total_nullifiers = 0u64;
                let mut blocks_processed = 0u32;

                while let Ok((height, block_opt)) = block_rx_clone.recv_async().await {
                    if let Some(block) = block_opt {
                        let processed =
                            tokio::task::spawn_blocking(move || process_block_sync(block))
                                .await
                                .unwrap();

                        // Write UTXO updates to worker's file
                        let mut creates_count = 0;
                        let mut spends_count = 0;
                        for update in &processed.utxo_updates {
                            match update {
                                UtxoUpdate::Create {
                                    txid,
                                    vout,
                                    value,
                                    lock_script,
                                } => {
                                    let txid_bytes: [u8; 32] = txid.0;
                                    creates_file.write_all(&txid_bytes).unwrap();
                                    creates_file.write_all(&vout.to_le_bytes()).unwrap();
                                    creates_file.write_all(&value.to_le_bytes()).unwrap();
                                    creates_file
                                        .write_all(&(lock_script.len() as u32).to_le_bytes())
                                        .unwrap();
                                    creates_file.write_all(lock_script).unwrap();
                                    creates_count += 1;
                                }
                                UtxoUpdate::Spend { txid, vout } => {
                                    let txid_bytes: [u8; 32] = txid.0;
                                    spends_file.write_all(&txid_bytes).unwrap();
                                    spends_file.write_all(&vout.to_le_bytes()).unwrap();
                                    spends_count += 1;
                                }
                            }
                        }

                        // Write nullifiers
                        for nullifier in &processed.nullifiers {
                            nullifiers_file.write_all(nullifier).unwrap();
                        }
                        total_creates += creates_count;
                        total_spends += spends_count;
                        total_nullifiers += processed.nullifiers.len() as u64;
                        blocks_processed += 1;
                        
                        if height % 10000 == 0 {
                            let elapsed = worker_start.elapsed().as_secs_f64();
                            let rate = blocks_processed as f64 / elapsed;
                            println!(
                                "  [Processor {}] Block {}: {} creates, {} spends, {} nullifiers | Total: {} creates, {} spends, {} nullifiers | Rate: {:.0} blocks/s",
                                worker_id,
                                height,
                                creates_count,
                                spends_count,
                                processed.nullifiers.len(),
                                total_creates,
                                total_spends,
                                total_nullifiers,
                                rate
                            );
                        }

                        // Send height completion signal
                        let _ = processed_tx.send((height, processed)).await;
                    }
                }

                creates_file.flush().unwrap();
                spends_file.flush().unwrap();
                nullifiers_file.flush().unwrap();
                
                // Get final file sizes
                let creates_size = std::fs::metadata(&creates_path).map(|m| m.len()).unwrap_or(0);
                let spends_size = std::fs::metadata(&spends_path).map(|m| m.len()).unwrap_or(0);
                let nullifiers_size = std::fs::metadata(&nullifiers_path).map(|m| m.len()).unwrap_or(0);
                
                let total_time = worker_start.elapsed();
                let rate = blocks_processed as f64 / total_time.as_secs_f64();
                println!("  [Processor {}] Completed:", worker_id);
                println!("    Blocks processed: {}", blocks_processed);
                println!("    Total creates: {} ({:.2} MB)", total_creates, creates_size as f64 / 1_000_000.0);
                println!("    Total spends: {} ({:.2} MB)", total_spends, spends_size as f64 / 1_000_000.0);
                println!("    Total nullifiers: {} ({:.2} MB)", total_nullifiers, nullifiers_size as f64 / 1_000_000.0);
                println!("    Processing rate: {:.0} blocks/s", rate);
                println!("    Total time: {:.2}s", total_time.as_secs_f64());
            })
        })
        .collect::<Vec<_>>();

    drop(processed_tx);

    // Track processed heights
    let next_expected_height_shared = Arc::new(Mutex::new(start));
    let next_expected_clone = next_expected_height_shared.clone();

    let mut processed_heights = std::collections::HashSet::new();
    let mut next_expected_height = start;

    let progress_handle = {
        let next_block = next_block.clone();
        tokio::spawn(async move {
            let mut last_fetched = start;
            let mut last_processed = start;
            let mut last_time = Instant::now();

            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let fetched = *next_block.lock().await;
                let processed = *next_expected_clone.lock().await;
                let now = Instant::now();
                let elapsed = now.duration_since(last_time).as_secs_f64();

                let fetched_this_interval = fetched.saturating_sub(last_fetched);
                let processed_this_interval = processed.saturating_sub(last_processed);
                let fetch_rate = fetched_this_interval as f64 / elapsed;
                let process_rate = processed_this_interval as f64 / elapsed;
                let percent =
                    ((processed.saturating_sub(start)) as f64 / total_blocks as f64) * 100.0;

                println!(
                    "  Progress: {:.2}% | Fetched: {} ({:.0}/s) | Processed: {} ({:.0}/s) | Buffer: {}",
                    percent,
                    fetched,
                    fetch_rate,
                    processed,
                    process_rate,
                    fetched.saturating_sub(processed)
                );

                last_fetched = fetched;
                last_processed = processed;
                last_time = now;

                if processed >= end {
                    break;
                }
            }
        })
    };

    // Collect processed block heights
    while let Some((height, _processed)) = processed_rx.recv().await {
        processed_heights.insert(height);

        // Update next_expected_height to be continuous
        while processed_heights.contains(&next_expected_height) {
            processed_heights.remove(&next_expected_height);
            next_expected_height += 1;
        }

        *next_expected_height_shared.lock().await = next_expected_height;

        if next_expected_height > end {
            break;
        }
    }

    for worker in workers {
        worker.await.unwrap();
    }

    for worker in processing_workers {
        worker.await.unwrap();
    }

    // Stop progress reporter
    progress_handle.abort();

    let elapsed = start_time.elapsed();
    println!("\n=== Phase 1 Complete ===");
    println!(
        "Finished processing {} blocks in {:.2}s ({:.2} minutes)",
        total_blocks,
        elapsed.as_secs_f64(),
        elapsed.as_secs_f64() / 60.0
    );
    println!(
        "Average processing rate: {:.0} blocks/s",
        total_blocks as f64 / elapsed.as_secs_f64()
    );

    // Merge worker files
    println!("\n=== Phase 2: File Merging ===");
    let merge_start = Instant::now();
    println!("Merging worker chunk files...");

    let creates_path = data_dir.join("utxo_creates.bin");
    let spends_path = data_dir.join("utxo_spends.bin");
    let nullifiers_path = data_dir.join("nullifiers_unsorted.bin");

    println!("Creating merged files:");
    println!("  Creates: {}", creates_path.display());
    println!("  Spends: {}", spends_path.display());
    println!("  Nullifiers: {}", nullifiers_path.display());

    let mut creates_file = BufWriter::with_capacity(16 * 1024 * 1024, File::create(&creates_path)?);
    let mut spends_file = BufWriter::with_capacity(16 * 1024 * 1024, File::create(&spends_path)?);
    let mut nullifiers_file =
        BufWriter::with_capacity(16 * 1024 * 1024, File::create(&nullifiers_path)?);

    let mut total_creates_size = 0u64;
    let mut total_spends_size = 0u64;
    let mut total_nullifiers_size = 0u64;

    for worker_id in 0..NUM_PROCESSORS {
        // Merge creates
        let worker_creates = data_dir.join(format!("utxo_creates.worker{}.bin", worker_id));
        if let Ok(mut f) = File::open(&worker_creates) {
            let size_before = creates_file
                .get_ref()
                .metadata()
                .map(|m| m.len())
                .unwrap_or(0);
            std::io::copy(&mut f, &mut creates_file)?;
            let size_after = creates_file
                .get_ref()
                .metadata()
                .map(|m| m.len())
                .unwrap_or(0);
            let copied = size_after - size_before;
            total_creates_size += copied;
            println!(
                "  Merged creates from worker {}: {:.2} MB",
                worker_id,
                copied as f64 / 1_000_000.0
            );
            std::fs::remove_file(worker_creates)?;
        }

        // Merge spends
        let worker_spends = data_dir.join(format!("utxo_spends.worker{}.bin", worker_id));
        if let Ok(mut f) = File::open(&worker_spends) {
            let size_before = spends_file
                .get_ref()
                .metadata()
                .map(|m| m.len())
                .unwrap_or(0);
            std::io::copy(&mut f, &mut spends_file)?;
            let size_after = spends_file
                .get_ref()
                .metadata()
                .map(|m| m.len())
                .unwrap_or(0);
            let copied = size_after - size_before;
            total_spends_size += copied;
            println!(
                "  Merged spends from worker {}: {:.2} MB",
                worker_id,
                copied as f64 / 1_000_000.0
            );
            std::fs::remove_file(worker_spends)?;
        }

        // Merge nullifiers
        let worker_nullifiers = data_dir.join(format!("nullifiers.worker{}.bin", worker_id));
        if let Ok(mut f) = File::open(&worker_nullifiers) {
            let size_before = nullifiers_file
                .get_ref()
                .metadata()
                .map(|m| m.len())
                .unwrap_or(0);
            std::io::copy(&mut f, &mut nullifiers_file)?;
            let size_after = nullifiers_file
                .get_ref()
                .metadata()
                .map(|m| m.len())
                .unwrap_or(0);
            let copied = size_after - size_before;
            total_nullifiers_size += copied;
            println!(
                "  Merged nullifiers from worker {}: {:.2} MB",
                worker_id,
                copied as f64 / 1_000_000.0
            );
            std::fs::remove_file(worker_nullifiers)?;
        }
    }

    creates_file.flush()?;
    spends_file.flush()?;
    nullifiers_file.flush()?;
    drop(creates_file);
    drop(spends_file);
    drop(nullifiers_file);

    let merge_time = merge_start.elapsed();
    println!("File merging completed in {:.2}s", merge_time.as_secs_f64());
    println!("Total merged sizes:");
    println!(
        "  Creates: {:.2} MB",
        total_creates_size as f64 / 1_000_000.0
    );
    println!("  Spends: {:.2} MB", total_spends_size as f64 / 1_000_000.0);
    println!(
        "  Nullifiers: {:.2} MB",
        total_nullifiers_size as f64 / 1_000_000.0
    );

    // Sort creates and spends in chunks
    println!("\n=== Phase 3: Sorting ===");
    let sort_start = Instant::now();

    println!("Sorting creates in chunks...");
    let creates_path_str = creates_path.to_string_lossy().to_string();
    let create_chunk_start = Instant::now();
    let create_chunks = sort_creates_in_chunks(&creates_path_str)?;
    let create_chunk_time = create_chunk_start.elapsed();
    println!(
        "Created {} create chunks in {:.2}s",
        create_chunks.len(),
        create_chunk_time.as_secs_f64()
    );

    println!("Sorting spends in chunks...");
    let spends_path_str = spends_path.to_string_lossy().to_string();
    let spend_chunk_start = Instant::now();
    let spend_chunks = sort_spends_in_chunks(&spends_path_str)?;
    let spend_chunk_time = spend_chunk_start.elapsed();
    println!(
        "Created {} spend chunks in {:.2}s",
        spend_chunks.len(),
        spend_chunk_time.as_secs_f64()
    );

    let sort_time = sort_start.elapsed();
    println!("Sorting phase completed in {:.2}s", sort_time.as_secs_f64());

    // K-way merge with set difference
    println!("\n=== Phase 4: UTXO Computation ===");
    let utxo_start = Instant::now();
    println!("Merging creates/spends and computing UTXO hashes...");
    let utxo_file_path = data_dir.join("utxo_hashes.bin");
    let utxo_file_path_str = utxo_file_path.to_string_lossy().to_string();
    let utxo_count = merge_and_compute_utxos(&create_chunks, &spend_chunks, &utxo_file_path_str)?;
    let utxo_time = utxo_start.elapsed();
    println!("Total UTXOs: {}", utxo_count);
    println!(
        "UTXO computation completed in {:.2}s ({:.0} UTXOs/s)",
        utxo_time.as_secs_f64(),
        utxo_count as f64 / utxo_time.as_secs_f64()
    );

    // Clean up intermediate files
    std::fs::remove_file(creates_path)?;
    std::fs::remove_file(spends_path)?;
    for chunk in create_chunks {
        std::fs::remove_file(chunk)?;
    }
    for chunk in spend_chunks {
        std::fs::remove_file(chunk)?;
    }

    // External sort and merkle tree for UTXOs
    println!("\n=== Phase 5: UTXO Merkle Tree ===");
    let utxo_tree_start = Instant::now();

    println!("Sorting UTXO hashes (external sort)...");
    let utxo_sort_start = Instant::now();
    external_sort(&utxo_file_path_str)?;
    let utxo_sort_time = utxo_sort_start.elapsed();
    println!(
        "UTXO sorting completed in {:.2}s",
        utxo_sort_time.as_secs_f64()
    );

    println!("Building UTXO merkle tree from sorted file...");
    let utxo_tree_dir = data_dir.join("utxo_tree");
    fs::create_dir_all(&utxo_tree_dir)?;
    let utxo_tree_dir_str = utxo_tree_dir.to_string_lossy().to_string();
    let tree_build_start = Instant::now();
    let utxo_root = build_merkle_tree_from_file(&utxo_file_path_str, &utxo_tree_dir_str)?;
    let tree_build_time = tree_build_start.elapsed();
    if let Some(root) = utxo_root {
        println!("UTXO Merkle Root: {}", hex::encode(root));
        println!(
            "UTXO tree building completed in {:.2}s",
            tree_build_time.as_secs_f64()
        );
    } else {
        println!("No UTXOs found");
    }
    let utxo_tree_time = utxo_tree_start.elapsed();
    println!(
        "UTXO Merkle tree phase completed in {:.2}s",
        utxo_tree_time.as_secs_f64()
    );

    // External sort and merkle tree for nullifiers
    println!("\n=== Phase 6: Nullifier Merkle Tree ===");
    let nullifier_tree_start = Instant::now();

    println!("Sorting nullifiers (external sort)...");
    let nullifiers_path_str = nullifiers_path.to_string_lossy().to_string();
    let nullifier_sort_start = Instant::now();
    external_sort(&nullifiers_path_str)?;
    let nullifier_sort_time = nullifier_sort_start.elapsed();
    println!(
        "Nullifier sorting completed in {:.2}s",
        nullifier_sort_time.as_secs_f64()
    );

    println!("Building nullifier merkle tree from sorted file...");
    let nullifier_tree_dir = data_dir.join("nullifier_tree");
    fs::create_dir_all(&nullifier_tree_dir)?;
    let nullifier_tree_dir_str = nullifier_tree_dir.to_string_lossy().to_string();
    let nullifier_tree_build_start = Instant::now();
    let nullifier_root =
        build_merkle_tree_from_file(&nullifiers_path_str, &nullifier_tree_dir_str)?;
    let nullifier_tree_build_time = nullifier_tree_build_start.elapsed();
    if let Some(root) = nullifier_root {
        println!("Nullifier Merkle Root: {}", hex::encode(root));
        println!(
            "Nullifier tree building completed in {:.2}s",
            nullifier_tree_build_time.as_secs_f64()
        );
    } else {
        println!("No nullifiers found");
    }
    let nullifier_tree_time = nullifier_tree_start.elapsed();
    println!(
        "Nullifier Merkle tree phase completed in {:.2}s",
        nullifier_tree_time.as_secs_f64()
    );

    // Example: Generate and verify proofs for multiple positions
    if let Some(utxo_root) = utxo_root {
        println!("\n=== UTXO Merkle Proof Tests ===");

        // Get total number of UTXOs
        let file_size = std::fs::metadata(&utxo_file_path)?.len();
        let total_utxos = file_size / 32;
        println!("Total UTXOs: {}", total_utxos);

        // Test cases: first, second, last, second-to-last
        let test_cases = vec![
            (0u64, "First leaf"),
            (1u64, "Second leaf"),
            (total_utxos - 1, "Last leaf"),
            (total_utxos - 2, "Second-to-last leaf"),
        ];

        for (test_idx, description) in test_cases {
            println!("\n--- Testing: {} (index {}) ---", description, test_idx);

            // Read hash at this index
            let mut file = File::open(&utxo_file_path)?;
            file.seek(std::io::SeekFrom::Start(test_idx * 32))?;
            let mut hash = [0u8; 32];
            if std::io::Read::read_exact(&mut file, &mut hash).is_ok() {
                println!("Hash: {}", hex::encode(&hash[..8]));

                // Verify we can find it
                if let Some(found_idx) = find_hash_index(&utxo_file_path_str, &hash)? {
                    if found_idx != test_idx {
                        println!(
                            "ERROR: Found at index {} but expected {}",
                            found_idx, test_idx
                        );
                        continue;
                    }

                    // Generate proof
                    let proof = generate_merkle_proof(&utxo_tree_dir_str, test_idx)?;
                    println!("Proof length: {} hashes", proof.len());

                    // Verify proof
                    let valid = zfun_lib::merkle::verify_merkle_inclusion(
                        &hash, &proof, &utxo_root, test_idx,
                    );
                    println!("Proof valid: {}", valid);

                    if !valid {
                        println!("ERROR: Proof verification failed!");
                        // Show first few proof elements for debugging
                        for (i, p) in proof.iter().take(3).enumerate() {
                            println!("  Proof[{}]: {}", i, hex::encode(&p[..8]));
                        }
                    }
                } else {
                    println!("ERROR: Could not find hash in sorted file");
                }
            }
        }
    }

    // Save snapshot metadata
    if let (Some(utxo_root), Some(nullifier_root)) = (utxo_root, nullifier_root) {
        let sapling = db
            .sapling_tree_by_height(&Height(end))
            .map(|tree| (tree.count(), tree.root()))
            .unwrap_or_default();
        let orchard = db
            .orchard_tree_by_height(&Height(end))
            .map(|tree| (tree.count(), tree.root()))
            .unwrap_or_default();
        let nullifier_count = std::fs::metadata(&nullifiers_path)?.len() / 32;

        let metadata = SnapshotMetadata {
            height: end,
            sapling_count: sapling.0,
            orchard_count: orchard.0,
            sapling_root: sapling.1.into(),
            orchard_root: orchard.1.into(),
            utxo_root,
            nullifier_root,
            utxo_count: utxo_count as u64,
            nullifier_count,
        };

        let metadata_json = serde_json::to_string_pretty(&metadata)?;
        let metadata_path = data_dir.join("snapshot_metadata.json");
        std::fs::write(&metadata_path, metadata_json)?;
        println!("\nSaved snapshot metadata to {}", metadata_path.display());
    }

    let total_time = start_time.elapsed();

    println!("\n=== Final Summary ===");
    println!(
        "Total execution time: {:.2}s ({:.2} minutes)",
        total_time.as_secs_f64(),
        total_time.as_secs_f64() / 60.0
    );
    println!("Total blocks processed: {}", total_blocks);
    println!(
        "Overall processing rate: {:.0} blocks/s",
        total_blocks as f64 / total_time.as_secs_f64()
    );

    println!("\n=== Data Statistics ===");
    println!("Total UTXOs: {}", utxo_count);
    if let Some(root) = utxo_root {
        println!("UTXO Merkle Root: {}", hex::encode(root));
        println!("UTXO tree saved to: {}", utxo_tree_dir.display());
        println!("Sorted UTXO hashes: {}", utxo_file_path.display());

        // Get file sizes
        if let Ok(metadata) = std::fs::metadata(&utxo_file_path) {
            println!(
                "UTXO hashes file size: {:.2} MB",
                metadata.len() as f64 / 1_000_000.0
            );
        }
        if let Ok(metadata) = std::fs::metadata(&utxo_tree_dir) {
            println!(
                "UTXO tree directory size: {:.2} MB",
                metadata.len() as f64 / 1_000_000.0
            );
        }
    }
    if let Some(root) = nullifier_root {
        println!("Nullifier Merkle Root: {}", hex::encode(root));
        println!("Nullifier tree saved to: {}", nullifier_tree_dir.display());
        println!("Sorted nullifier hashes: {}", nullifiers_path.display());

        // Get file sizes
        if let Ok(metadata) = std::fs::metadata(&nullifiers_path) {
            println!(
                "Nullifier hashes file size: {:.2} MB",
                metadata.len() as f64 / 1_000_000.0
            );
        }
        if let Ok(metadata) = std::fs::metadata(&nullifier_tree_dir) {
            println!(
                "Nullifier tree directory size: {:.2} MB",
                metadata.len() as f64 / 1_000_000.0
            );
        }
    }

    println!("\n=== Phase Timing Breakdown ===");
    println!(
        "Phase 1 (Block Fetching & Processing): {:.2}s ({:.1}%)",
        elapsed.as_secs_f64(),
        elapsed.as_secs_f64() / total_time.as_secs_f64() * 100.0
    );
    println!(
        "Phase 2 (File Merging): {:.2}s ({:.1}%)",
        merge_time.as_secs_f64(),
        merge_time.as_secs_f64() / total_time.as_secs_f64() * 100.0
    );
    println!(
        "Phase 3 (Sorting): {:.2}s ({:.1}%)",
        sort_time.as_secs_f64(),
        sort_time.as_secs_f64() / total_time.as_secs_f64() * 100.0
    );
    println!(
        "Phase 4 (UTXO Computation): {:.2}s ({:.1}%)",
        utxo_time.as_secs_f64(),
        utxo_time.as_secs_f64() / total_time.as_secs_f64() * 100.0
    );
    println!(
        "Phase 5 (UTXO Merkle Tree): {:.2}s ({:.1}%)",
        utxo_tree_time.as_secs_f64(),
        utxo_tree_time.as_secs_f64() / total_time.as_secs_f64() * 100.0
    );
    println!(
        "Phase 6 (Nullifier Merkle Tree): {:.2}s ({:.1}%)",
        nullifier_tree_time.as_secs_f64(),
        nullifier_tree_time.as_secs_f64() / total_time.as_secs_f64() * 100.0
    );

    Ok(())
}
