use crate::optimized_ops::prefetch_read;
use crate::{Pool, ShardHeader, position_within_shard};
use eyre::{Result, ensure, eyre};
use memmap2::MmapOptions;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

/// Node type for shard files (32 bytes)
pub type Node = [u8; 32];

/// Shard file reader/writer
/// Supports memory-mapped reading for efficient witness extraction
pub struct ShardFile {
    header: ShardHeader,
    pool: Pool,
    _file: Option<File>,
    mmap: Option<memmap2::Mmap>,
    memory: Option<Vec<u8>>,
}

impl ShardFile {
    /// Open an existing shard file for reading
    pub fn open<P: AsRef<Path>>(path: P, pool: Pool) -> Result<Self> {
        let file =
            File::open(path.as_ref()).map_err(|e| eyre!("Failed to open shard file: {}", e))?;

        // Read header
        let mut header_bytes = [0u8; ShardHeader::HEADER_SIZE];
        file.try_clone()?
            .read_exact(&mut header_bytes)
            .map_err(|e| eyre!("Failed to read shard header: {}", e))?;

        let header = ShardHeader::from_bytes(&header_bytes)?;
        header.validate()?;

        // Memory map the file for efficient access
        let mmap = unsafe {
            MmapOptions::new()
                .map(&file)
                .map_err(|e| eyre!("Failed to memory-map shard file: {}", e))?
        };

        Ok(Self {
            header,
            pool,
            _file: Some(file),
            mmap: Some(mmap),
            memory: None,
        })
    }

    /// Create a shard from in-memory bytes
    pub fn from_bytes(bytes: Vec<u8>, pool: Pool) -> Result<Self> {
        ensure!(
            bytes.len() >= ShardHeader::HEADER_SIZE,
            "Shard data too short for header"
        );

        let header = ShardHeader::from_bytes(&bytes[..ShardHeader::HEADER_SIZE])?;
        header.validate()?;

        Ok(Self {
            header,
            pool,
            _file: None,
            mmap: None,
            memory: Some(bytes),
        })
    }

    /// Get a reference to the shard data
    fn get_data(&self) -> Result<&[u8]> {
        if let Some(ref mmap) = self.mmap {
            Ok(&mmap[..])
        } else if let Some(ref memory) = self.memory {
            Ok(&memory[..])
        } else {
            Err(eyre!("Shard data not available"))
        }
    }

    /// Create a new shard file and write header + tree levels
    pub fn create<P: AsRef<Path>>(
        path: P,
        pool: Pool,
        shard_id: u32,
        start_position: u64,
        end_position: u64,
        leaf_count: u32,
        levels: &[Vec<Node>],
    ) -> Result<Self> {
        // Validate levels structure
        Self::validate_levels(leaf_count, levels)?;

        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .read(true)
            .open(path.as_ref())
            .map_err(|e| eyre!("Failed to create shard file: {}", e))?;

        let header = ShardHeader::new(shard_id, start_position, end_position, leaf_count);
        header.validate()?;

        // Write header
        file.write_all(&header.to_bytes())
            .map_err(|e| eyre!("Failed to write shard header: {}", e))?;

        // OPTIMIZED: Batch write all levels for better I/O performance
        // Pre-calculate total size and write in larger chunks
        let total_nodes: usize = levels.iter().map(|level| level.len()).sum();
        let buffer_size = (total_nodes * 32).min(8 * 1024 * 1024); // Max 8MB buffer
        let mut write_buffer = Vec::with_capacity(buffer_size);

        for level in levels {
            for node in level {
                write_buffer.extend_from_slice(node);

                // Flush buffer when it gets large enough
                if write_buffer.len() >= buffer_size - 32 {
                    file.write_all(&write_buffer)
                        .map_err(|e| eyre!("Failed to write shard nodes: {}", e))?;
                    write_buffer.clear();
                }
            }
        }

        // Write remaining data
        if !write_buffer.is_empty() {
            file.write_all(&write_buffer)
                .map_err(|e| eyre!("Failed to write shard nodes: {}", e))?;
        }

        file.flush()?;
        file.sync_all()?;

        // Reopen for reading
        drop(file);
        Self::open(path, pool)
    }

    /// Validate that levels structure is correct
    fn validate_levels(leaf_count: u32, levels: &[Vec<Node>]) -> Result<()> {
        if leaf_count == 0 {
            ensure!(levels.is_empty(), "Empty shard should have no levels");
            return Ok(());
        }

        ensure!(
            !levels.is_empty(),
            "Non-empty shard must have at least one level (leaves)"
        );

        // Check level 0 (leaves) matches leaf_count
        ensure!(
            levels[0].len() == leaf_count as usize,
            "Level 0 (leaves) size {} does not match leaf_count {}",
            levels[0].len(),
            leaf_count
        );

        // Validate each subsequent level halves (ceil) the previous
        let mut expected_size = leaf_count as usize;
        for (level_idx, level) in levels.iter().enumerate().skip(1) {
            expected_size = (expected_size + 1) / 2; // Ceiling division
            ensure!(
                level.len() == expected_size,
                "Level {} size {} does not match expected size {}",
                level_idx,
                level.len(),
                expected_size
            );
        }

        // Last level should contain exactly one node (root)
        ensure!(
            levels[levels.len() - 1].len() == 1,
            "Last level must contain exactly one node (root), got {}",
            levels[levels.len() - 1].len()
        );

        Ok(())
    }

    /// Get the header
    pub fn header(&self) -> &ShardHeader {
        &self.header
    }

    /// Extract Merkle path for a position within the shard
    /// Returns siblings from leaf to root (excluding the leaf itself)
    pub fn extract_path(&self, position_within_shard: u64) -> Result<Vec<Node>> {
        ensure!(
            position_within_shard < self.header.leaf_count as u64,
            "Position {} is out of bounds (leaf_count: {})",
            position_within_shard,
            self.header.leaf_count
        );

        let data = self.get_data()?;
        let mut path = Vec::new();
        let mut level_offset = ShardHeader::HEADER_SIZE;
        let mut level_size = self.header.leaf_count as usize;
        let mut idx = position_within_shard as usize;

        // Extract siblings from each level
        while level_size > 1 {
            // Calculate sibling index
            let sibling_idx = idx ^ 1;

            // If sibling exists at this level, read it
            if sibling_idx < level_size {
                let node_offset = level_offset + (sibling_idx * 32);
                ensure!(
                    node_offset + 32 <= data.len(),
                    "Node offset out of bounds: {} + 32 > {}",
                    node_offset,
                    data.len()
                );

                // OPTIMIZATION: Prefetch next level's data for better cache performance
                // Only useful for mmap
                if self.mmap.is_some() {
                    let next_level_offset = level_offset + level_size * 32;
                    if next_level_offset + 32 <= data.len() {
                        unsafe {
                            prefetch_read(data[next_level_offset..].as_ptr());
                        }
                    }
                }

                let mut node = [0u8; 32];
                node.copy_from_slice(&data[node_offset..node_offset + 32]);
                path.push(node);
            } else {
                // Odd node at end - duplicate self
                let node_offset = level_offset + (idx * 32);
                ensure!(
                    node_offset + 32 <= data.len(),
                    "Node offset out of bounds: {} + 32 > {}",
                    node_offset,
                    data.len()
                );
                let mut node = [0u8; 32];
                node.copy_from_slice(&data[node_offset..node_offset + 32]);
                path.push(node);
            }

            // Move to next level
            level_offset += level_size * 32;
            level_size = (level_size + 1) / 2; // Ceiling division
            idx /= 2;
        }

        Ok(path)
    }

    /// Extract Merkle path for a global position
    pub fn extract_path_global(&self, global_position: u64) -> Result<Vec<Node>> {
        let pos_within_shard = position_within_shard(global_position, self.pool);
        self.extract_path(pos_within_shard)
    }

    /// Get the root node of this shard
    pub fn root(&self) -> Result<Node> {
        // Handle empty shards
        if self.header.leaf_count == 0 {
            return Err(eyre!("Cannot get root of empty shard (leaf_count == 0)"));
        }

        let data = self.get_data()?;

        // Find the root level (last level with size 1)
        let mut level_offset = ShardHeader::HEADER_SIZE;
        let mut level_size = self.header.leaf_count as usize;

        while level_size > 1 {
            level_offset += level_size * 32;
            level_size = (level_size + 1) / 2;
        }

        // Read root
        ensure!(
            level_offset + 32 <= data.len(),
            "Root offset out of bounds: {} + 32 > {}",
            level_offset,
            data.len()
        );

        let mut root = [0u8; 32];
        root.copy_from_slice(&data[level_offset..level_offset + 32]);
        Ok(root)
    }

    /// Read a specific node from a level
    pub fn read_node(&self, level: usize, index: usize) -> Result<Node> {
        let data = self.get_data()?;

        // Calculate level offset
        let mut level_offset = ShardHeader::HEADER_SIZE;
        let mut level_size = self.header.leaf_count as usize;

        for _ in 0..level {
            level_offset += level_size * 32;
            level_size = (level_size + 1) / 2;
        }

        let node_offset = level_offset + (index * 32);
        ensure!(
            node_offset + 32 <= data.len(),
            "Node offset out of bounds: level {}, index {}, offset {} + 32 > {}",
            level,
            index,
            node_offset,
            data.len()
        );

        let mut node = [0u8; 32];
        node.copy_from_slice(&data[node_offset..node_offset + 32]);
        Ok(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_shard_file_create_and_read() {
        let temp_dir = TempDir::new().unwrap();
        let shard_path = temp_dir.path().join("test_shard.bin");

        let pool = Pool::Orchard;
        let shard_id = 0;
        let start_pos = 0;
        let end_pos = 65536;
        let leaf_count = 4;

        // Create a simple 4-leaf tree
        let mut levels = Vec::new();

        // Level 0: 4 leaves
        levels.push(vec![[1u8; 32], [2u8; 32], [3u8; 32], [4u8; 32]]);

        // Level 1: 2 nodes (hash of pairs)
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update([1u8; 32]);
        hasher.update([2u8; 32]);
        let h12: Node = hasher.finalize().into();

        hasher = Sha256::new();
        hasher.update([3u8; 32]);
        hasher.update([4u8; 32]);
        let h34: Node = hasher.finalize().into();

        levels.push(vec![h12, h34]);

        // Level 2: 1 root
        hasher = Sha256::new();
        hasher.update(h12);
        hasher.update(h34);
        let root: Node = hasher.finalize().into();
        levels.push(vec![root]);

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
        assert_eq!(shard.header().shard_id, shard_id);
        assert_eq!(shard.header().leaf_count, leaf_count);

        // Extract path for position 0
        let path = shard.extract_path(0).unwrap();
        assert_eq!(path.len(), 2); // Should have 2 siblings (leaf 1, then h34)

        // Verify root
        let computed_root = shard.root().unwrap();
        assert_eq!(computed_root, root);
    }
}
