use eyre::Result;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};

/// Build a Merkle tree from a sorted file of 32-byte hashes
/// Returns the root hash if the file is non-empty
pub fn build_merkle_tree_from_file(sorted_file: &str, tree_dir: &str) -> Result<Option<[u8; 32]>> {
    std::fs::create_dir_all(tree_dir)?;

    let file_size = std::fs::metadata(sorted_file)?.len();
    if file_size == 0 {
        return Ok(None);
    }

    let num_hashes = file_size / 32;
    if num_hashes == 0 {
        return Ok(None);
    }

    // Copy sorted file as level 0
    let level0_file = format!("{}/level_0.bin", tree_dir);
    std::fs::copy(sorted_file, &level0_file)?;

    let mut level_size = num_hashes;
    let mut level = 0;

    // Build tree bottom-up
    while level_size > 1 {
        let current_file = format!("{}/level_{}.bin", tree_dir, level);
        let next_file = format!("{}/level_{}.bin", tree_dir, level + 1);

        let mut input = BufReader::new(File::open(&current_file)?);
        let mut output = BufWriter::new(File::create(&next_file)?);

        let mut buf1 = [0u8; 32];
        let mut buf2 = [0u8; 32];
        let mut next_level_size = 0;

        while input.read_exact(&mut buf1).is_ok() {
            let hash: [u8; 32] = if input.read_exact(&mut buf2).is_ok() {
                // Hash pair
                let mut hasher = Sha256::new();
                hasher.update(buf1);
                hasher.update(buf2);
                hasher.finalize().into()
            } else {
                // Odd one out - hash with itself
                let mut hasher = Sha256::new();
                hasher.update(buf1);
                hasher.update(buf1);
                hasher.finalize().into()
            };
            output.write_all(&hash)?;
            next_level_size += 1;
        }

        output.flush()?;
        level_size = next_level_size;
        level += 1;
    }

    // Read final root
    let root_file = format!("{}/level_{}.bin", tree_dir, level);
    let mut file = File::open(&root_file)?;
    let mut root = [0u8; 32];
    file.read_exact(&mut root)?;

    Ok(Some(root))
}

/// Binary search for a hash in a sorted file
pub fn find_hash_index(sorted_file: &str, target_hash: &[u8; 32]) -> Result<Option<u64>> {
    let file_size = std::fs::metadata(sorted_file)?.len();
    let num_hashes = file_size / 32;

    if num_hashes == 0 {
        return Ok(None);
    }

    let mut file = File::open(sorted_file)?;
    let mut left = 0u64;
    let mut right = num_hashes - 1;

    while left <= right {
        let mid = left + (right - left) / 2;

        file.seek(SeekFrom::Start(mid * 32))?;
        let mut hash = [0u8; 32];
        file.read_exact(&mut hash)?;

        match hash.cmp(target_hash) {
            std::cmp::Ordering::Equal => return Ok(Some(mid)),
            std::cmp::Ordering::Less => left = mid + 1,
            std::cmp::Ordering::Greater => {
                if mid == 0 {
                    break;
                }
                right = mid - 1;
            }
        }
    }

    Ok(None)
}

/// Generate Merkle proof for a hash at given index
pub fn generate_merkle_proof(tree_dir: &str, leaf_index: u64) -> Result<Vec<[u8; 32]>> {
    let mut proof = Vec::new();
    let mut idx = leaf_index;
    let mut level = 0;

    loop {
        let level_file = format!("{}/level_{}.bin", tree_dir, level);
        let next_level_file = format!("{}/level_{}.bin", tree_dir, level + 1);

        // Check if this level exists
        if !std::path::Path::new(&level_file).exists() {
            break;
        }

        // Check if next level exists - if not, we're at the root level, stop
        if !std::path::Path::new(&next_level_file).exists() {
            break;
        }

        // Get sibling index
        let sibling_idx = idx ^ 1; // XOR with 1 to get sibling

        // Read sibling hash
        let mut file = File::open(&level_file)?;
        file.seek(SeekFrom::Start(sibling_idx * 32))?;
        let mut sibling_hash = [0u8; 32];
        if file.read_exact(&mut sibling_hash).is_ok() {
            proof.push(sibling_hash);
        } else {
            // No sibling (odd node at end), duplicate self
            file.seek(SeekFrom::Start(idx * 32))?;
            let mut self_hash = [0u8; 32];
            file.read_exact(&mut self_hash)?;
            proof.push(self_hash);
        }

        idx /= 2;
        level += 1;
    }

    Ok(proof)
}
