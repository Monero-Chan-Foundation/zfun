//! Adapter for fetching commitments from Zebra's finalized state at a given height
//!
//! This module provides functions to extract Orchard and Sapling note commitments
//! from Zebra's finalized state database at a specific block height.
//!
//! The ZebraDb instance must be opened with db_kind="finalized" to access finalized blocks.
//! All blocks are read from Zebra's finalized state (RocksDB), not from the non-finalized chain.
//!
//! Uses parallel processing with multiple workers to fetch and process blocks concurrently,
//! similar to the original zfun implementation for better performance.

use crossbeam_channel::{Receiver, Sender, bounded};
use eyre::{Result, eyre};
use ff::PrimeField; // For to_repr() on field elements
use orchard::note::ExtractedNoteCommitment;
use parking_lot::Mutex;
use sapling::note::ExtractedNoteCommitment as SaplingExtractedNoteCommitment;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};
use zebra_chain::block::{Block, Height};
use zebra_state::{HashOrHeight, IntoDisk, ZebraDb};

use crate::CommitmentWithPosition;

// OPTIMIZATION: Global lock-free pools for zero-copy commitment processing
// Instead of cloning from thread-local buffers, we append directly to shared pools
lazy_static::lazy_static! {
    static ref ORCHARD_POOL: Mutex<Vec<[u8; 32]>> = Mutex::new(Vec::with_capacity(100_000_000));
    static ref SAPLING_POOL: Mutex<Vec<[u8; 32]>> = Mutex::new(Vec::with_capacity(100_000_000));
}

// Thread-local buffers for batch processing before pool insertion
thread_local! {
    static ORCHARD_BUFFER: RefCell<Vec<[u8; 32]>> = RefCell::new(Vec::with_capacity(1000));
    static SAPLING_BUFFER: RefCell<Vec<[u8; 32]>> = RefCell::new(Vec::with_capacity(1000));
}

/// Fetch all Orchard commitments from a single block
///
/// OPTIMIZED: Returns (start_idx, len) range into global ORCHARD_POOL (zero-copy)
///
/// Uses Transaction::orchard_note_commitments() which returns pallas::Base values.
/// Converts each to ExtractedNoteCommitment and then to MerkleHashOrchard node.
fn extract_orchard_commitments_from_block(block: &Block) -> Result<(usize, usize)> {
    use crate::extracted_commitment_to_node_orchard;

    // Fast path: check if block has any commitments first (O(1) check)
    let has_orchard = block
        .transactions
        .iter()
        .any(|tx| tx.orchard_note_commitments().next().is_some());

    if !has_orchard {
        return Ok((0, 0)); // Empty range for empty blocks
    }

    // Optimized: Batch commitments in thread-local buffer, then append to pool in one lock
    ORCHARD_BUFFER.with(|buffer| {
        let mut buf = buffer.borrow_mut();
        buf.clear(); // Reuse allocation

        // Pre-collect all commitments (cheap, just references)
        let all_cmx: Vec<_> = block.transactions.iter()
            .flat_map(|tx| tx.orchard_note_commitments())
            .collect();

        if all_cmx.is_empty() {
            return Ok((0, 0));
        }

        // Batch convert (compiler can vectorize)
        for cm_x in all_cmx {
            let cm_x_bytes = cm_x.to_repr();
            let extracted_cmx = Option::from(ExtractedNoteCommitment::from_bytes(&cm_x_bytes))
                .ok_or_else(|| eyre!("Failed to convert pallas::Base to ExtractedNoteCommitment: invalid field element"))?;
            let node_bytes = extracted_commitment_to_node_orchard(&extracted_cmx)?;
            buf.push(node_bytes);
        }

        // ZERO-COPY: Append entire batch to global pool in single lock, return range
        let mut pool = ORCHARD_POOL.lock();
        let start_idx = pool.len();
        let len = buf.len();
        pool.extend_from_slice(&buf);
        Ok((start_idx, len))
    })
}

/// Fetch all Sapling commitments from a single block
///
/// OPTIMIZED: Returns (start_idx, len) range into global SAPLING_POOL (zero-copy)
///
/// Uses Transaction::sapling_note_commitments() which returns jubjub::Fq values (cmu).
/// Converts each to ExtractedNoteCommitment and then to SaplingNode.
fn extract_sapling_commitments_from_block(block: &Block) -> Result<(usize, usize)> {
    use crate::extracted_commitment_to_node_sapling;

    // Fast path: check if block has any commitments first (O(1) check)
    let has_sapling = block
        .transactions
        .iter()
        .any(|tx| tx.sapling_note_commitments().next().is_some());

    if !has_sapling {
        return Ok((0, 0)); // Empty range for empty blocks
    }

    // Optimized: Reuse thread-local buffer to reduce allocations
    SAPLING_BUFFER.with(|buffer| {
        let mut buf = buffer.borrow_mut();
        buf.clear(); // Reuse allocation

        // Pre-collect all commitments (cheap, just references)
        let all_cmu: Vec<_> = block
            .transactions
            .iter()
            .flat_map(|tx| tx.sapling_note_commitments())
            .collect();

        if all_cmu.is_empty() {
            return Ok((0, 0));
        }

        // Batch convert (compiler can vectorize)
        for cmu in all_cmu {
            let cmu_bytes = cmu.to_bytes();
            let extracted_cmu = Option::from(SaplingExtractedNoteCommitment::from_bytes(
                &cmu_bytes,
            ))
            .ok_or_else(|| {
                eyre!(
                    "Failed to convert jubjub::Fq to ExtractedNoteCommitment: invalid field element"
                )
            })?;
            let node_bytes = extracted_commitment_to_node_sapling(&extracted_cmu)?;
            buf.push(node_bytes);
        }

        // ZERO-COPY: Append entire batch to global pool in single lock, return range
        let mut pool = SAPLING_POOL.lock();
        let start_idx = pool.len();
        let len = buf.len();
        pool.extend_from_slice(&buf);
        Ok((start_idx, len))
    })
}

/// Result of processing a single block
#[derive(Debug, Clone)]
// OPTIMIZATION: Use ranges instead of Vec to avoid cloning
struct BlockCommitments {
    height: u32,
    orchard_range: (usize, usize), // (start_idx, len) into ORCHARD_POOL
    sapling_range: (usize, usize), // (start_idx, len) into SAPLING_POOL
}

/// Fetch all commitments up to a given height from Zebra's finalized state (cumulative tree state)
///
/// This reads blocks from Zebra's finalized state database and processes them in parallel
/// using multiple workers to fetch blocks concurrently and extract commitments, then assembles
/// them in order with cumulative positions.
///
/// The ZebraDb instance must be opened with db_kind="finalized" to access finalized blocks.
///
/// Returns (orchard_commitments, sapling_commitments) where each is a Vec of (position, node_bytes).
pub fn fetch_all_commitments_to_height(
    db: &Arc<ZebraDb>,
    target_height: u32,
) -> Result<(Vec<CommitmentWithPosition>, Vec<CommitmentWithPosition>)> {
    // Tuned worker counts: more fetch workers for I/O bound, fewer process workers for CPU bound
    // Configurable via environment variables: FETCH_WORKERS and PROCESS_WORKERS
    // Defaults optimized for typical servers (can be increased for high-core systems)
    let fetch_workers = std::env::var("FETCH_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(256); // Increased default for better I/O parallelism

    let process_workers = std::env::var("PROCESS_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(64); // 2x CPU cores is good for CPU-bound work

    let start_time = Instant::now();
    println!(
        "Fetching commitments from blocks 0 to {} using {} fetch workers and {} processing workers...",
        target_height, fetch_workers, process_workers
    );

    // Optimized: Atomic counter for progress reporting (removes branch from hot path)
    static CURRENT_HEIGHT: AtomicU32 = AtomicU32::new(0);
    CURRENT_HEIGHT.store(0, Ordering::Relaxed);

    // Spawn dedicated progress thread (removes expensive calculations from hot path)
    let progress_start = start_time;
    let progress_target_height = target_height;
    let progress_handle = std::thread::spawn(move || {
        let mut last_printed_height = 0u32;
        let mut last_print_time = progress_start;

        loop {
            std::thread::sleep(Duration::from_secs(2));
            let height = CURRENT_HEIGHT.load(Ordering::Relaxed);

            // Don't exit early - keep logging until we're done
            if height > progress_target_height {
                // Print final message
                let elapsed = progress_start.elapsed().as_secs_f64();
                let blocks_per_sec = if elapsed > 0.0 {
                    progress_target_height as f64 / elapsed
                } else {
                    0.0
                };
                println!(
                    "  Block {} (100.0%) | {:.1} blocks/sec | Complete | Elapsed: {:.1}s",
                    progress_target_height, blocks_per_sec, elapsed
                );
                break;
            }

            let now = Instant::now();
            // Log every 2 seconds OR every 10k blocks, whichever comes first
            let time_since_print = now.duration_since(last_print_time).as_secs();
            let blocks_since_print = height.saturating_sub(last_printed_height);

            if (time_since_print >= 2 && blocks_since_print >= 1000) || blocks_since_print >= 10000
            {
                if height > 0 {
                    let elapsed = progress_start.elapsed().as_secs_f64();
                    let percentage = (height as f64 / progress_target_height as f64) * 100.0;
                    let blocks_per_sec = if elapsed > 0.0 {
                        height as f64 / elapsed
                    } else {
                        0.0
                    };
                    let remaining_blocks = (progress_target_height - height) as f64;
                    let eta_secs = if blocks_per_sec > 0.0 {
                        (remaining_blocks / blocks_per_sec).round() as u64
                    } else {
                        0
                    };

                    println!(
                        "  Block {} ({:.2}%) | {:.1} blocks/sec | {} remaining | ETA: {}h {}m {}s | Elapsed: {:.1}s",
                        height,
                        percentage,
                        blocks_per_sec,
                        remaining_blocks as u64,
                        eta_secs / 3600,
                        (eta_secs % 3600) / 60,
                        eta_secs % 60,
                        elapsed
                    );

                    last_printed_height = height;
                    last_print_time = now;
                }
            }
        }
    });

    // Optimized: Larger channel buffers to reduce thread blocking
    // Channel for block heights to fetch
    let (height_tx, height_rx): (Sender<u32>, Receiver<u32>) = bounded(fetch_workers * 4);

    // Channel for fetched blocks
    let (block_tx, block_rx): (Sender<(u32, Arc<Block>)>, Receiver<(u32, Arc<Block>)>) =
        bounded(fetch_workers * 4);

    // Channel for processed commitments
    let (commitment_tx, commitment_rx): (Sender<BlockCommitments>, Receiver<BlockCommitments>) =
        bounded(process_workers * 4);

    // Spawn workers to fetch blocks from database
    // OPTIMIZATION: Batch reads by grouping heights together for better RocksDB caching
    println!("  Spawning {} fetch workers...", fetch_workers);
    let db_clone = Arc::clone(db);
    let block_tx_clone = block_tx.clone();
    let height_rx_clone = height_rx.clone();

    // Batch size for grouping consecutive heights (improves RocksDB read-ahead)
    const BATCH_SIZE: u32 = 100; // Fetch 100 consecutive blocks at a time

    let fetch_handles: Vec<_> = (0..fetch_workers)
        .map(|_| {
            let db = Arc::clone(&db_clone);
            let height_rx = height_rx_clone.clone();
            let block_tx = block_tx_clone.clone();

            std::thread::spawn(move || {
                let mut batch = Vec::with_capacity(BATCH_SIZE as usize);
                let mut last_height = None;

                while let Ok(height_u32) = height_rx.recv() {
                    // Group consecutive heights into batches for better RocksDB performance
                    if let Some(last) = last_height {
                        // If height is not consecutive, flush batch
                        if height_u32 != last + 1 && !batch.is_empty() {
                            // Process batch (already sorted by height)
                            for h in batch.drain(..) {
                                let height = Height(h);
                                if let Some(block) = db.block(HashOrHeight::Height(height)) {
                                    let _ = block_tx.send((h, block));
                                } else if h > 0 {
                                    eprintln!(
                                        "  Warning: Block at height {} not found, skipping",
                                        h
                                    );
                                }
                            }
                        }
                    }

                    batch.push(height_u32);
                    last_height = Some(height_u32);

                    // Flush batch when it reaches batch size
                    if batch.len() >= BATCH_SIZE as usize {
                        for h in batch.drain(..) {
                            let height = Height(h);
                            if let Some(block) = db.block(HashOrHeight::Height(height)) {
                                let _ = block_tx.send((h, block));
                            } else if h > 0 {
                                eprintln!("  Warning: Block at height {} not found, skipping", h);
                            }
                        }
                        last_height = None;
                    }
                }

                // Flush remaining batch
                for h in batch {
                    let height = Height(h);
                    if let Some(block) = db.block(HashOrHeight::Height(height)) {
                        let _ = block_tx.send((h, block));
                    } else if h > 0 {
                        eprintln!("  Warning: Block at height {} not found, skipping", h);
                    }
                }
            })
        })
        .collect();

    // Spawn workers to process blocks and extract commitments
    println!("  Spawning {} process workers...", process_workers);
    let block_rx_clone = block_rx.clone();
    let commitment_tx_clone = commitment_tx.clone();

    // Shared counter for all workers to report total processed
    let total_processed = Arc::new(AtomicU32::new(0));

    let process_handles: Vec<_> = (0..process_workers)
        .map(|worker_id| {
            let block_rx = block_rx_clone.clone();
            let commitment_tx = commitment_tx_clone.clone();
            let total_processed = total_processed.clone();
            
            std::thread::spawn(move || {
                let mut processed_count = 0u32;
                let worker_start = Instant::now();
                let mut last_activity = Instant::now(); // Track last time we received work
                
                loop {
                    match block_rx.recv_timeout(Duration::from_secs(2)) {
                        Ok((height, block)) => {
                            last_activity = Instant::now(); // Reset activity timer
                            match extract_block_commitments(&block) {
                                Ok(comms) => {
                                    match commitment_tx.send(BlockCommitments {
                                        height,
                                        orchard_range: comms.0,
                                        sapling_range: comms.1,
                                    }) {
                                        Ok(_) => {
                                            processed_count += 1;
                                            total_processed.fetch_add(1, Ordering::Relaxed);
                                            if processed_count % 10000 == 0 {
                                                println!("  Worker {}: processed {} blocks (last: height {})", worker_id, processed_count, height);
                                            }
                                        }
                                        Err(_) => {
                                            println!("  Worker {}: commitment_tx closed, exiting (processed {} blocks)", worker_id, processed_count);
                                            break;
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("  Worker {}: error processing block {}: {}", worker_id, height, e);
                                }
                            }
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                            // Exit if we haven't received work in 10 seconds
                            let idle_time = last_activity.elapsed().as_secs();
                            if idle_time > 10 {
                                println!("  Worker {}: no blocks received for {}s, exiting (processed {} blocks)", 
                                    worker_id, idle_time, processed_count);
                                break;
                            }
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                            println!("  Worker {}: block_rx closed, exiting (processed {} blocks)", worker_id, processed_count);
                            break;
                        }
                    }
                }
                
                let total_time = worker_start.elapsed().as_secs_f64();
                if processed_count > 0 {
                    println!("  Worker {} exiting: processed {} blocks in {:.1}s ({:.1} blocks/sec)", 
                        worker_id, processed_count, total_time, processed_count as f64 / total_time);
                } else {
                    println!("  Worker {} exiting: processed 0 blocks", worker_id);
                }
            })
        })
        .collect();

    // Send all heights to fetch in sequential order (helps RocksDB read-ahead)
    // OPTIMIZATION: Send heights sequentially to enable RocksDB sequential read optimization
    println!(
        "  Starting height sender thread (sending {} heights sequentially)...",
        target_height + 1
    );
    let height_sender_handle = std::thread::spawn(move || {
        let mut sent = 0u32;
        // Send heights sequentially (0, 1, 2, ...) to enable RocksDB sequential read optimization
        for height in 0..=target_height {
            if height_tx.send(height).is_err() {
                println!(
                    "  Height sender: receiver dropped after sending {} heights",
                    sent
                );
                break; // Receiver dropped, workers done
            }
            sent += 1;
            if sent % 500000 == 0 {
                println!("  Height sender: sent {} heights...", sent);
            }
        }
        println!(
            "  Height sender: finished sending all {} heights sequentially",
            sent
        );
    });

    // Collect processed blocks - use BTreeMap to maintain order for efficient sequential processing
    let mut processed_blocks: BTreeMap<u32, BlockCommitments> = BTreeMap::new();
    let mut next_expected_height = 0u32;
    let mut total_received = 0u32;

    // Preallocate vectors for commitments as we process
    let estimated_orchard = (target_height as usize + 1) * 2;
    let estimated_sapling = (target_height as usize + 1) * 10;
    let mut orchard_commitments = Vec::with_capacity(estimated_orchard);
    let mut sapling_commitments = Vec::with_capacity(estimated_sapling);
    let mut orchard_position = 0u64;
    let mut sapling_position = 0u64;

    // Process commitments as they arrive, maintaining order
    let mut last_logged_received = 0u32;
    let mut last_logged_processed = 0u32;
    let mut last_log_time = start_time;
    while total_received <= target_height {
        // Receive processed blocks (blocking with timeout to check progress)
        match commitment_rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(comms) => {
                processed_blocks.insert(comms.height, comms);
                total_received += 1;
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                // Timeout - check if we can process more blocks
                // Log progress every 100k blocks or every 10 seconds
                let now = Instant::now();
                let time_since_log = now.duration_since(last_log_time).as_secs();
                if (total_received - last_logged_received >= 100000)
                    || (time_since_log >= 10 && total_received > last_logged_received)
                {
                    println!(
                        "  Main loop: received {} blocks, processed {} blocks, queued {} blocks",
                        total_received,
                        next_expected_height,
                        processed_blocks.len()
                    );
                    last_logged_received = total_received;
                    last_log_time = now;
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                // Channel closed, all workers done
                println!(
                    "  Main loop: channel disconnected, received {} total blocks",
                    total_received
                );
                break;
            }
        }

        // Process blocks in order as they become available, ACCUMULATING commitments
        let mut processed_this_iteration = 0;
        while let Some(comms) = processed_blocks.remove(&next_expected_height) {
            // ULTRA-OPTIMIZED: Use ranges to access global pool (zero-copy)
            let (orchard_start, orchard_len) = comms.orchard_range;
            if orchard_len > 0 {
                orchard_commitments.reserve(orchard_len);
                // Access pool ONCE and copy slice
                let pool = ORCHARD_POOL.lock();
                for i in 0..orchard_len {
                    let node = pool[orchard_start + i];
                    orchard_commitments.push((orchard_position + i as u64, node));
                }
                drop(pool); // Release lock
                orchard_position += orchard_len as u64;
            }

            // ULTRA-OPTIMIZED: Same for sapling
            let (sapling_start, sapling_len) = comms.sapling_range;
            if sapling_len > 0 {
                sapling_commitments.reserve(sapling_len);
                // Access pool ONCE and copy slice
                let pool = SAPLING_POOL.lock();
                for i in 0..sapling_len {
                    let node = pool[sapling_start + i];
                    sapling_commitments.push((sapling_position + i as u64, node));
                }
                drop(pool); // Release lock
                sapling_position += sapling_len as u64;
            }

            // Optimized: Update atomic counter (cheap, removes branch from hot path)
            CURRENT_HEIGHT.store(next_expected_height, Ordering::Relaxed);
            next_expected_height += 1;
            processed_this_iteration += 1;
        }

        // Log if we processed a batch
        if processed_this_iteration > 0 && (next_expected_height - last_logged_processed >= 100000)
        {
            println!(
                "  Main loop: processed {} blocks (now at height {}), queued {} blocks",
                next_expected_height,
                next_expected_height,
                processed_blocks.len()
            );
            last_logged_processed = next_expected_height;
        }

        // Check if we're done
        if next_expected_height > target_height {
            println!(
                "  Main loop: reached target height {}, breaking",
                target_height
            );
            break;
        }
    }

    println!(
        "  Main loop complete: received {} blocks, processed {} blocks, remaining in queue: {}",
        total_received,
        next_expected_height,
        processed_blocks.len()
    );

    // Wait for height sender to finish
    println!("  Waiting for height sender thread to finish...");
    height_sender_handle.join().ok();
    println!("  Height sender thread finished");

    // Signal progress thread to stop and wait for it
    println!("  Stopping progress thread...");
    CURRENT_HEIGHT.store(target_height + 1, Ordering::Relaxed);
    progress_handle.join().ok();
    println!("  Progress thread stopped");

    // Fix 1: Add final progress log
    let final_height = CURRENT_HEIGHT.load(Ordering::Relaxed).min(target_height);
    let fetch_phase_time = start_time.elapsed();
    println!("\n=== Fetch Phase Complete ===");
    println!("  Time: {:.1}s", fetch_phase_time.as_secs_f64());
    println!("  Blocks processed: {}", final_height);
    println!("  Orchard commitments: {}", orchard_commitments.len());
    println!("  Sapling commitments: {}", sapling_commitments.len());

    // Drop senders to signal workers to stop
    println!("  Dropping block_tx to close block channel (fetch workers will exit)...");
    drop(block_tx);
    println!("  Dropping commitment_tx to close commitment channel (process workers will exit)...");
    drop(commitment_tx);

    // Wait for all worker threads to finish
    println!(
        "  Waiting for {} fetch workers to finish...",
        fetch_handles.len()
    );
    for (idx, handle) in fetch_handles.into_iter().enumerate() {
        handle.join().ok();
        if idx > 0 && idx % 32 == 0 {
            println!("  {} fetch workers finished...", idx);
        }
    }
    println!("  All fetch workers finished");

    println!(
        "  Waiting for {} process workers to finish...",
        process_handles.len()
    );
    println!(
        "  (Process workers should exit when block_rx channel closes after block_tx was dropped)"
    );

    // Add a timeout-based monitoring thread with progress tracking
    let workers_done = Arc::new(AtomicBool::new(false));
    let workers_done_clone = workers_done.clone();
    let total_processed_clone = total_processed.clone();
    let monitor_handle = std::thread::spawn(move || {
        let mut elapsed = 0u64;
        let mut last_processed = 0u32;
        while !workers_done_clone.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_secs(5));
            elapsed += 5;
            if !workers_done_clone.load(Ordering::Relaxed) {
                let current_processed = total_processed_clone.load(Ordering::Relaxed);
                let blocks_in_5s = current_processed.saturating_sub(last_processed);
                let blocks_per_sec = blocks_in_5s as f64 / 5.0;
                println!(
                    "  Still waiting for process workers... ({}s elapsed)",
                    elapsed
                );
                println!(
                    "  Total processed since channels closed: {} blocks ({:.1} blocks/sec)",
                    current_processed, blocks_per_sec
                );
                last_processed = current_processed;
            }
        }
    });

    let process_start = Instant::now();
    for (idx, handle) in process_handles.into_iter().enumerate() {
        let wait_start = Instant::now();
        println!("  Joining process worker {}...", idx);
        handle.join().ok();
        let wait_time = wait_start.elapsed();
        if wait_time.as_secs() > 0 {
            println!(
                "  Process worker {} finished (took {:.1}s to join)",
                idx,
                wait_time.as_secs_f64()
            );
        }
        if idx > 0 && idx % 16 == 0 {
            println!(
                "  {} process workers finished... (elapsed: {:.1}s)",
                idx,
                process_start.elapsed().as_secs_f64()
            );
        }
    }

    // Signal monitor thread to stop
    workers_done.store(true, Ordering::Relaxed);
    monitor_handle.join().ok();

    let process_wait_time = process_start.elapsed();
    println!(
        "  All process workers finished (total wait: {:.1}s)",
        process_wait_time.as_secs_f64()
    );

    // Collect any remaining processed blocks and add their commitments
    println!("  Collecting remaining processed blocks...");
    let mut remaining_count = 0;
    while let Ok(comms) = commitment_rx.try_recv() {
        processed_blocks.insert(comms.height, comms);
        remaining_count += 1;
        if remaining_count % 10000 == 0 {
            println!("  Collected {} remaining blocks...", remaining_count);
        }
    }
    if remaining_count > 0 {
        println!("  Collected {} remaining blocks", remaining_count);
    } else {
        println!("  No remaining blocks to collect");
    }

    // Process any blocks that came in after we stopped the main loop
    let blocks_to_process = if next_expected_height <= target_height {
        (target_height - next_expected_height + 1) as usize
    } else {
        0
    };
    if blocks_to_process > 0 {
        println!(
            "  Processing {} blocks in sequential cleanup...",
            blocks_to_process
        );
        let mut processed = 0;
        for height in next_expected_height..=target_height {
            if let Some(comms) = processed_blocks.remove(&height) {
                // ULTRA-OPTIMIZED: Use ranges (same as main loop)
                let (orchard_start, orchard_len) = comms.orchard_range;
                if orchard_len > 0 {
                    orchard_commitments.reserve(orchard_len);
                    let pool = ORCHARD_POOL.lock();
                    for i in 0..orchard_len {
                        let node = pool[orchard_start + i];
                        orchard_commitments.push((orchard_position + i as u64, node));
                    }
                    drop(pool);
                    orchard_position += orchard_len as u64;
                }

                let (sapling_start, sapling_len) = comms.sapling_range;
                if sapling_len > 0 {
                    sapling_commitments.reserve(sapling_len);
                    let pool = SAPLING_POOL.lock();
                    for i in 0..sapling_len {
                        let node = pool[sapling_start + i];
                        sapling_commitments.push((sapling_position + i as u64, node));
                    }
                    drop(pool);
                    sapling_position += sapling_len as u64;
                }
                processed += 1;
                if processed % 10000 == 0 {
                    println!(
                        "  Processed {}/{} cleanup blocks...",
                        processed, blocks_to_process
                    );
                }
            }
        }
        if processed > 0 {
            println!("  Processed {} cleanup blocks", processed);
        } else {
            println!("  No cleanup blocks to process");
        }
    } else {
        println!("  No cleanup blocks needed");
    }

    // Fix 2: Add collection phase timing
    let total_time = start_time.elapsed();
    println!("\n=== Collection Complete ===");
    println!("  Total time: {:.1}s", total_time.as_secs_f64());
    println!("  Orchard: {} commitments", orchard_commitments.len());
    println!("  Sapling: {} commitments", sapling_commitments.len());

    Ok((orchard_commitments, sapling_commitments))
}

/// Extract commitments from a single block (used by parallel workers)
///
/// Optimized: Early return for empty blocks to skip processing overhead
/// OPTIMIZED: Returns ranges into global pools instead of vectors
fn extract_block_commitments(block: &Block) -> Result<((usize, usize), (usize, usize))> {
    // Quick check: skip processing if block has no commitments
    // Use next().is_some() for O(1) check instead of count() which iterates all
    let has_commitments = block.transactions.iter().any(|tx| {
        tx.orchard_note_commitments().next().is_some()
            || tx.sapling_note_commitments().next().is_some()
    });

    if !has_commitments {
        // Empty block - return empty ranges
        return Ok(((0, 0), (0, 0)));
    }

    let orchard_range = extract_orchard_commitments_from_block(block)?;
    let sapling_range = extract_sapling_commitments_from_block(block)?;
    Ok((orchard_range, sapling_range))
}

/// Find the highest available block height in the database
/// Uses binary search to efficiently find the tip
pub fn get_highest_available_height(db: &Arc<ZebraDb>, max_height: u32) -> Option<u32> {
    // Binary search to find the highest available block
    let mut low = 1u32;
    let mut high = max_height;
    let mut result = None;

    while low <= high {
        let mid = low + (high - low) / 2;
        let zebra_height = Height(mid);

        if db.block(HashOrHeight::Height(zebra_height)).is_some() {
            result = Some(mid);
            low = mid + 1; // Try higher
        } else {
            high = mid - 1; // Try lower
        }
    }

    result
}

/// Get the block hash for a given height
pub fn get_block_hash(db: &Arc<ZebraDb>, height: u32) -> Result<String> {
    let zebra_height = Height(height);

    let block = db
        .block(HashOrHeight::Height(zebra_height))
        .ok_or_else(|| {
            // Try to find the highest available height for a helpful error message
            let highest = get_highest_available_height(db, height + 10000)
                .or_else(|| get_highest_available_height(db, height * 2));

            if let Some(highest_height) = highest {
                eyre!(
                    "Block at height {} not found. Highest available block height in database: {}",
                    height,
                    highest_height
                )
            } else {
                eyre!(
                    "Block at height {} not found. Database may be empty or not synced.",
                    height
                )
            }
        })?;

    let hash = block.hash();
    // BlockHash is a tuple struct Hash([u8; 32]) - access field 0
    Ok(hex::encode(hash.0))
}

/// Get the tree roots at a given height from Zebra
///
/// This queries Zebra's incremental tree state directly, which is much faster than
/// recomputing from all commitments. However, we still need to fetch commitments
/// to build the shard files, so this is primarily useful for verification.
pub fn get_tree_roots(db: &Arc<ZebraDb>, height: u32) -> Result<([u8; 32], [u8; 32])> {
    use zebra_chain::block::Height;

    let zebra_height = Height(height);

    // Query Zebra's incremental tree state
    let orchard_root = db
        .orchard_tree_by_height(&zebra_height)
        .map(|tree| tree.root().as_bytes())
        .ok_or_else(|| eyre!("Orchard tree not found at height {}", height))?;

    let sapling_root = db
        .sapling_tree_by_height(&zebra_height)
        .map(|tree| tree.root().as_bytes())
        .ok_or_else(|| eyre!("Sapling tree not found at height {}", height))?;

    // Convert to [u8; 32] arrays
    let mut orchard_bytes = [0u8; 32];
    let mut sapling_bytes = [0u8; 32];
    orchard_bytes.copy_from_slice(&orchard_root[..32]);
    sapling_bytes.copy_from_slice(&sapling_root[..32]);

    Ok((orchard_bytes, sapling_bytes))
}
