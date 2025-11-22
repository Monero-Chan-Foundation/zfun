pub mod in_memory_witness_source;
pub mod optimized_ops;
pub mod shard_file;
pub mod shard_header;
pub mod snapshot_builder;
pub mod tree_builder;
pub mod witness_source;

#[cfg(feature = "zebra")]
pub mod zebra_adapter;
#[cfg(feature = "zebra")]
pub use zebra_adapter::{fetch_all_commitments_to_height, get_block_hash, get_tree_roots};

#[cfg(feature = "wallet-db")]
compile_error!(
    "The 'wallet-db' feature is not yet implemented. \
    The wallet database only stores wallet-owned notes, not all commitments. \
    Use the default build (without --features wallet-db) for synthetic data testing."
);

pub use in_memory_witness_source::InMemoryWitnessSource;
pub use shard_file::ShardFile;
pub use shard_header::ShardHeader;
pub use snapshot_builder::{ShardInfo, SnapshotBuilder, SnapshotMetadata};
pub use tree_builder::{
    build_tree_levels_pool, commitment_to_node, extracted_commitment_to_node_orchard,
    extracted_commitment_to_node_sapling,
};
pub use witness_source::{
    CacheConfig, CacheStats, MerklePath, SnapshotWitnessSource, WitnessSource,
};

/// Commitment with its position in the tree
pub type CommitmentWithPosition = (u64, [u8; 32]);

use eyre::{Result, eyre};

/// Shard size constant - uniform for both pools per architecture spec
/// The architecture doc assumes uniform SHARD_SIZE (65,536 leaves) per shard
pub const SHARD_SIZE: u64 = 1 << 16; // 65,536

/// Deprecated: Use SHARD_SIZE instead. Kept for backwards compatibility.
pub const SHARD_SIZE_ORCHARD: u64 = SHARD_SIZE;
/// Deprecated: Use SHARD_SIZE instead. Kept for backwards compatibility.
pub const SHARD_SIZE_SAPLING: u64 = SHARD_SIZE;

/// Pool type for shards
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pool {
    Orchard,
    Sapling,
}

impl Pool {
    pub fn shard_size(&self) -> u64 {
        SHARD_SIZE // Uniform shard size for both pools
    }

    pub fn tree_depth(&self) -> u8 {
        match self {
            Pool::Orchard => 32,
            Pool::Sapling => 32,
        }
    }
}

impl std::str::FromStr for Pool {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "orchard" => Ok(Pool::Orchard),
            "sapling" => Ok(Pool::Sapling),
            _ => Err(eyre!(
                "Invalid pool type: {}. Must be 'orchard' or 'sapling'",
                s
            )),
        }
    }
}

/// Calculate shard ID from a position
///
/// OPTIMIZATION: Use bitshift instead of division for power-of-2 shard size
#[inline(always)]
pub fn shard_id_from_position(position: u64, _pool: Pool) -> u32 {
    // SHARD_SIZE is 2^16, so we can use right shift by 16 instead of division
    (position >> 16) as u32
}

/// Calculate start position for a shard
///
/// OPTIMIZATION: Use bitshift instead of multiplication
#[inline(always)]
pub fn shard_start_position(shard_id: u32, _pool: Pool) -> u64 {
    // Multiply by 2^16 using left shift
    (shard_id as u64) << 16
}

/// Calculate end position for a shard
///
/// OPTIMIZATION: Use bitshift operations
#[inline(always)]
pub fn shard_end_position(shard_id: u32, _pool: Pool) -> u64 {
    ((shard_id + 1) as u64) << 16
}

/// Position within a shard (0-indexed within the shard)
///
/// OPTIMIZATION: Use bitwise AND instead of modulo for power-of-2
#[inline(always)]
pub fn position_within_shard(position: u64, _pool: Pool) -> u64 {
    // SHARD_SIZE is 2^16, so mask with 0xFFFF (2^16 - 1)
    position & 0xFFFF
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shard_file::Node;
    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    // Synthetic hash function for testing only - not valid for real commitments
    fn hash_pair(left: &Node, right: &Node) -> Node {
        let mut hasher = Sha256::new();
        hasher.update(left);
        hasher.update(right);
        hasher.finalize().into()
    }

    fn build_tree_levels(leaves: &[Node]) -> Vec<Vec<Node>> {
        let mut levels = Vec::new();
        if leaves.is_empty() {
            return levels;
        }

        levels.push(leaves.to_vec());
        let mut current_level = leaves.to_vec();

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

        levels
    }

    #[test]
    fn test_shard_id_calculation() {
        // Both pools use uniform shard size
        assert_eq!(shard_id_from_position(0, Pool::Orchard), 0);
        assert_eq!(shard_id_from_position(65535, Pool::Orchard), 0);
        assert_eq!(shard_id_from_position(65536, Pool::Orchard), 1);
        assert_eq!(shard_id_from_position(0, Pool::Sapling), 0);
        assert_eq!(shard_id_from_position(65535, Pool::Sapling), 0);
        assert_eq!(shard_id_from_position(65536, Pool::Sapling), 1);

        // Verify both pools map the same positions to same shard IDs
        for pos in [0, 1000, 32768, 65535, 65536, 100000] {
            assert_eq!(
                shard_id_from_position(pos, Pool::Orchard),
                shard_id_from_position(pos, Pool::Sapling),
                "Position {} should map to same shard for both pools",
                pos
            );
        }
    }

    #[test]
    fn test_synthetic_shard_write_read_verify() {
        let temp_dir = TempDir::new().unwrap();
        let shard_path = temp_dir.path().join("test_shard.bin");

        let pool = Pool::Orchard;
        let shard_id = 0;
        let start_pos = 0;
        let end_pos = 65536;
        let leaf_count = 16; // Small tree for testing

        // Generate synthetic leaves
        let mut leaves = Vec::new();
        for i in 0..leaf_count {
            let mut node = [0u8; 32];
            node[0..8].copy_from_slice(&(i as u64).to_le_bytes());
            leaves.push(node);
        }

        // Build tree levels
        let levels = build_tree_levels(&leaves);

        // Create shard file
        let shard = ShardFile::create(
            &shard_path,
            pool,
            shard_id,
            start_pos,
            end_pos,
            leaf_count,
            &levels,
        )
        .unwrap();

        // Verify header
        let header = shard.header();
        assert_eq!(header.shard_id, shard_id);
        assert_eq!(header.start_position, start_pos);
        assert_eq!(header.end_position, end_pos);
        assert_eq!(header.leaf_count, leaf_count);

        // Verify root matches expected
        let stored_root = shard.root().unwrap();
        let expected_root = &levels[levels.len() - 1][0];
        assert_eq!(stored_root, *expected_root);

        // Test path extraction for all positions
        for pos in 0..leaf_count {
            let path = shard.extract_path(pos as u64).unwrap();

            // Verify path length (should be log2(leaf_count) rounded up)
            let expected_path_len = (leaf_count as f64).log2().ceil() as usize;
            assert_eq!(
                path.len(),
                expected_path_len,
                "Position {} path length mismatch",
                pos
            );

            // Reconstruct root from path and verify
            let mut current = leaves[pos as usize];
            let mut idx = pos as usize;

            for sibling in &path {
                if idx % 2 == 0 {
                    current = hash_pair(&current, sibling);
                } else {
                    current = hash_pair(sibling, &current);
                }
                idx /= 2;
            }

            assert_eq!(
                current, stored_root,
                "Position {} root reconstruction failed",
                pos
            );
        }

        // Test invalid position
        assert!(shard.extract_path(leaf_count as u64).is_err());
        assert!(shard.extract_path(1000).is_err());
    }

    #[test]
    fn test_empty_shard() {
        let temp_dir = TempDir::new().unwrap();
        let shard_path = temp_dir.path().join("empty_shard.bin");

        let pool = Pool::Orchard;
        let shard_id = 0;
        let start_pos = 0;
        let end_pos = 65536;
        let leaf_count = 0;

        // Create empty shard
        let shard = ShardFile::create(
            &shard_path,
            pool,
            shard_id,
            start_pos,
            end_pos,
            leaf_count,
            &[], // Empty levels
        )
        .unwrap();

        // Verify header
        let header = shard.header();
        assert_eq!(header.leaf_count, 0);

        // Root should return error for empty shard
        assert!(shard.root().is_err());

        // Extract path should fail for any position
        assert!(shard.extract_path(0).is_err());
    }

    #[test]
    fn test_partial_shard() {
        let temp_dir = TempDir::new().unwrap();
        let shard_path = temp_dir.path().join("partial_shard.bin");

        let pool = Pool::Orchard;
        let shard_id = 754;
        let start_pos = shard_start_position(shard_id, pool);
        let end_pos = shard_end_position(shard_id, pool);
        let leaf_count = 39405; // Partial shard as mentioned in spec

        // Generate synthetic leaves
        let mut leaves = Vec::new();
        for i in 0..leaf_count {
            let mut node = [0u8; 32];
            node[0..8].copy_from_slice(&(i as u64).to_le_bytes());
            leaves.push(node);
        }

        // Build tree levels
        let levels = build_tree_levels(&leaves);

        // Create shard file
        let shard = ShardFile::create(
            &shard_path,
            pool,
            shard_id,
            start_pos,
            end_pos,
            leaf_count,
            &levels,
        )
        .unwrap();

        // Verify partial shard properties
        let header = shard.header();
        assert_eq!(header.leaf_count, leaf_count);
        assert!(header.leaf_count < (end_pos - start_pos) as u32);

        // Test positions at boundaries
        assert!(shard.extract_path(0).is_ok());
        assert!(shard.extract_path((leaf_count - 1) as u64).is_ok());
        assert!(shard.extract_path(leaf_count as u64).is_err());
    }
}
