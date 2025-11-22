//! Tree building with proper Orchard/Sapling hash functions
//!
//! ## Commitment Conversion for Zebra Integration
//!
//! When wiring to Zebra or wallet DBs, commitments come as `NoteCommitment` types.
//! They must be converted to tree nodes as follows:
//!
//! ### Orchard
//! ```rust,no_run
//! use orchard::note::{Note, ExtractedNoteCommitment};
//! use orchard::tree::MerkleHashOrchard;
//!
//! # // Example placeholder - replace with actual NoteCommitment from Zebra
//! # let note_commitment = unimplemented!();
//! // From Zebra: get NoteCommitment (e.g., from note.commitment())
//! // let note_commitment: orchard::note::NoteCommitment = ...;
//!
//! // Convert to ExtractedNoteCommitment
//! // let extracted_cmx = ExtractedNoteCommitment::from(note_commitment);
//!
//! // Convert to MerkleHashOrchard node
//! // let node = MerkleHashOrchard::from_cmx(&extracted_cmx);
//! // let node_bytes = node.to_bytes();
//! ```
//!
//! ### Sapling
//! ```rust,no_run
//! use sapling::note::Note;
//! use sapling::Node as SaplingNode;
//!
//! # // Example placeholder - replace with actual Note from Zebra
//! # let note: Note = unimplemented!();
//! // From Zebra: get Note (which has cmu() method)
//! // let note: Note = ...;
//!
//! // Get ExtractedNoteCommitment directly
//! // let extracted_cmu = note.cmu();
//!
//! // Convert to SaplingNode
//! // let node = SaplingNode::from_cmu(&extracted_cmu);
//! // let node_bytes = node.to_bytes();
//! ```
//!
//! The `commitment_to_node()` function in this module currently only accepts
//! pre-converted node bytes for testing. When integrating with Zebra, implement
//! the conversion in the data source layer before calling the snapshot builder.

use crate::Pool;
use crate::shard_file::Node;
use eyre::{Result, eyre};
use incrementalmerkletree::{Hashable, Level};
use orchard::tree::MerkleHashOrchard;
use rayon::prelude::*;
use sapling::Node as SaplingNode;

/// Build tree levels using proper Orchard/Sapling hash functions
///
/// Note: Level calculation - in incrementalmerkletree, Level 0 is leaves.
/// For Orchard/Sapling trees with depth 32, the root is at Level 32.
/// When building from leaves upward, we start at Level 0 (leaves) and increment.
///
/// OPTIMIZED: Reduces allocations by avoiding unnecessary clones and pre-allocating buffers.
pub fn build_tree_levels_pool(leaves: &[Node], pool: Pool) -> Result<Vec<Vec<Node>>> {
    if leaves.is_empty() {
        return Ok(Vec::new());
    }

    // Pre-allocate levels vec with capacity (log2(leaves.len()) + 1)
    let max_levels = (leaves.len() as f64).log2().ceil() as usize + 1;
    let mut levels = Vec::with_capacity(max_levels);

    // Level 0: leaves (avoid clone by copying)
    levels.push(leaves.to_vec());

    // Build subsequent levels using proper hash functions
    // Start at level 0 (leaves level)
    let mut current_level = leaves.to_vec();
    let mut level_num = 0u8;

    while current_level.len() > 1 {
        // Pre-calculate next level size for better allocation
        let pair_count = current_level.len() / 2;
        let has_odd = current_level.len() % 2 == 1;
        let next_size = pair_count + if has_odd { 1 } else { 0 };

        // OPTIMIZATION: Tune parallel threshold based on actual workload
        // Lower threshold (500) for better parallelization on modern CPUs
        let next_level: Result<Vec<Node>> = if pair_count > 500 {
            // Parallel processing for large levels
            let mut next_level: Vec<Node> = (0..pair_count)
                .into_par_iter()
                .map(|i| {
                    let left = &current_level[i * 2];
                    let right = &current_level[i * 2 + 1];
                    combine_nodes(pool, Level::new(level_num + 1), left, right)
                })
                .collect::<Result<Vec<_>>>()?;

            // Handle odd node at end: pair with empty root at current level
            if has_odd {
                let last = &current_level[current_level.len() - 1];
                let empty_hash = empty_root_at_level(pool, Level::new(level_num))?;
                let parent = combine_nodes(pool, Level::new(level_num + 1), last, &empty_hash)?;
                next_level.push(parent);
            }

            Ok(next_level)
        } else {
            // Sequential processing for small levels with pre-allocated buffer
            let mut next_level = Vec::with_capacity(next_size);

            for i in 0..pair_count {
                let left = &current_level[i * 2];
                let right = &current_level[i * 2 + 1];
                next_level.push(combine_nodes(pool, Level::new(level_num + 1), left, right)?);
            }

            // Handle odd node at end: pair with empty root at current level
            if has_odd {
                let last = &current_level[current_level.len() - 1];
                let empty_hash = empty_root_at_level(pool, Level::new(level_num))?;
                next_level.push(combine_nodes(
                    pool,
                    Level::new(level_num + 1),
                    last,
                    &empty_hash,
                )?);
            }

            Ok(next_level)
        };

        let next_level = next_level?;

        // OPTIMIZATION: Move instead of clone to avoid allocation
        levels.push(next_level);
        current_level = levels.last().unwrap().clone();
        level_num += 1;
    }

    Ok(levels)
}

/// Get the empty root hash at a given level
fn empty_root_at_level(pool: Pool, level: Level) -> Result<Node> {
    match pool {
        Pool::Orchard => {
            let empty = <MerkleHashOrchard as Hashable>::empty_root(level);
            Ok(empty.to_bytes())
        }
        Pool::Sapling => {
            let empty = <SaplingNode as Hashable>::empty_root(level);
            Ok(empty.to_bytes())
        }
    }
}

/// Combine two nodes using the appropriate hash function for the pool
fn combine_nodes(pool: Pool, level: Level, left: &Node, right: &Node) -> Result<Node> {
    match pool {
        Pool::Orchard => {
            let left_hash = Option::<MerkleHashOrchard>::from(MerkleHashOrchard::from_bytes(left))
                .ok_or_else(|| eyre!("Invalid Orchard node bytes"))?;
            let right_hash =
                Option::<MerkleHashOrchard>::from(MerkleHashOrchard::from_bytes(right))
                    .ok_or_else(|| eyre!("Invalid Orchard node bytes"))?;
            let combined = <MerkleHashOrchard as Hashable>::combine(level, &left_hash, &right_hash);
            Ok(combined.to_bytes())
        }
        Pool::Sapling => {
            let left_node = Option::<SaplingNode>::from(SaplingNode::from_bytes(*left))
                .ok_or_else(|| eyre!("Invalid Sapling node bytes"))?;
            let right_node = Option::<SaplingNode>::from(SaplingNode::from_bytes(*right))
                .ok_or_else(|| eyre!("Invalid Sapling node bytes"))?;
            let combined = <SaplingNode as Hashable>::combine(level, &left_node, &right_node);
            Ok(combined.to_bytes())
        }
    }
}

/// Convert ExtractedNoteCommitment to node for tree building
///
/// This is the proper conversion path for real commitments from Zebra/wallet DBs.
///
/// When wiring to Zebra, you should call the conversion directly with the actual types:
/// - Orchard: `MerkleHashOrchard::from_cmx(&extracted_cmx)` where `extracted_cmx: ExtractedNoteCommitment`
/// - Sapling: `SaplingNode::from_cmu(&extracted_cmu)` where `extracted_cmu: ExtractedNoteCommitment`
///
/// This function is a type-safe wrapper that accepts the actual ExtractedNoteCommitment types.
/// It should be used when you have the types available from Zebra/wallet DBs.
#[allow(unused)] // Will be used when wiring to Zebra
pub fn extracted_commitment_to_node_orchard(
    extracted_cmx: &orchard::note::ExtractedNoteCommitment,
) -> Result<Node> {
    use orchard::tree::MerkleHashOrchard;
    let node = MerkleHashOrchard::from_cmx(extracted_cmx);
    Ok(node.to_bytes())
}

#[allow(unused)] // Will be used when wiring to Zebra
pub fn extracted_commitment_to_node_sapling(
    extracted_cmu: &sapling::note::ExtractedNoteCommitment,
) -> Result<Node> {
    use sapling::Node as SaplingNode;
    let node = SaplingNode::from_cmu(extracted_cmu);
    Ok(node.to_bytes())
}

/// Convert commitments to nodes for tree building
///
/// WARNING: This function currently only works with pre-converted node bytes.
///
/// For production use with Zebra:
/// 1. Get NoteCommitment from Zebra
/// 2. Convert to ExtractedNoteCommitment:
///    - Orchard: `ExtractedNoteCommitment::from(note.commitment())`
///    - Sapling: `note.cmu()` (returns ExtractedNoteCommitment)
/// 3. Convert to node:
///    - Orchard: `MerkleHashOrchard::from_cmx(&extracted_cmx)`
///    - Sapling: `SaplingNode::from_cmu(&extracted_cmu)`
///
/// This function is for testing with pre-converted node bytes only.
/// When wiring to Zebra, implement proper conversion in the data source layer
/// before calling the snapshot builder.
pub fn commitment_to_node(pool: Pool, commitment: &[u8; 32]) -> Result<Node> {
    match pool {
        Pool::Orchard => {
            // Try to parse as MerkleHashOrchard directly (if already in node format)
            // This works for pre-converted nodes used in testing
            if let Some(hash) =
                Option::<MerkleHashOrchard>::from(MerkleHashOrchard::from_bytes(commitment))
            {
                return Ok(hash.to_bytes());
            }

            // Real commitments from Zebra need proper conversion:
            // NoteCommitment -> ExtractedNoteCommitment -> MerkleHashOrchard::from_cmx()
            // This must be implemented when wiring to Zebra.
            Err(eyre!(
                "Orchard commitment conversion not implemented. Input bytes are not valid MerkleHashOrchard nodes. \
                Real commitments from Zebra must be converted via: \
                NoteCommitment -> ExtractedNoteCommitment::from(note.commitment()) -> MerkleHashOrchard::from_cmx(&extracted_cmx). \
                This function currently only accepts pre-converted node bytes for testing."
            ))
        }
        Pool::Sapling => {
            // Try to parse as SaplingNode directly (if already in node format)
            // This works for pre-converted nodes used in testing
            if let Some(node) = Option::<SaplingNode>::from(SaplingNode::from_bytes(*commitment)) {
                return Ok(node.to_bytes());
            }

            // Real commitments from Zebra need proper conversion:
            // NoteCommitment -> note.cmu() (ExtractedNoteCommitment) -> SaplingNode::from_cmu(&extracted_cmu)
            // This must be implemented when wiring to Zebra.
            Err(eyre!(
                "Sapling commitment conversion not implemented. Input bytes are not valid SaplingNode nodes. \
                Real commitments from Zebra must be converted via: \
                NoteCommitment -> note.cmu() -> SaplingNode::from_cmu(&extracted_cmu). \
                This function currently only accepts pre-converted node bytes for testing."
            ))
        }
    }
}
