use hex;
use sha2::{Digest, Sha256};

/// Verify a Merkle inclusion proof
///
/// # Arguments
/// * `leaf_hash` - The hash of the leaf to verify
/// * `proof_path` - Sibling hashes along the path from leaf to root
/// * `root` - Expected root hash
/// * `leaf_index` - Position of the leaf in the tree
///
/// # Returns
/// `true` if the proof is valid, `false` otherwise
pub fn verify_merkle_inclusion(
    leaf_hash: &[u8; 32],
    proof_path: &[[u8; 32]],
    root: &[u8; 32],
    leaf_index: u64,
) -> bool {
    let mut current_hash = *leaf_hash;
    let mut idx = leaf_index;

    for (level, sibling) in proof_path.iter().enumerate() {
        let mut hasher = Sha256::new();

        // If idx is even, we're on the left, sibling on right
        let is_left = idx.is_multiple_of(2);
        if is_left {
            hasher.update(current_hash);
            hasher.update(sibling);
        } else {
            hasher.update(sibling);
            hasher.update(current_hash);
        }

        let old_hash = current_hash;
        current_hash = hasher.finalize().into();
        idx /= 2;

        // Debug logging for first few and last few levels
        if level < 3 || level >= proof_path.len().saturating_sub(3) {
            println!(
                "      [verify_merkle_inclusion] Level {}: idx={}, is_left={}, old={}..., sibling={}..., new={}...",
                level,
                idx * 2 + (if is_left { 0 } else { 1 }),
                is_left,
                &hex::encode(old_hash)[..16],
                &hex::encode(sibling)[..16],
                &hex::encode(current_hash)[..16]
            );
        }
    }

    let matches = &current_hash == root;
    if !matches {
        println!("      [verify_merkle_inclusion] Final hash mismatch:");
        println!("        Computed: {}", hex::encode(current_hash));
        println!("        Expected: {}", hex::encode(root));
    }
    matches
}

/// Hash two sibling nodes together
#[inline]
pub fn hash_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_merkle_inclusion_single_leaf() {
        let leaf = [1u8; 32];
        let proof = vec![];
        let root = leaf;

        assert!(verify_merkle_inclusion(&leaf, &proof, &root, 0));
    }

    #[test]
    fn test_verify_merkle_inclusion_two_leaves() {
        let leaf0 = [1u8; 32];
        let leaf1 = [2u8; 32];
        let root = hash_pair(&leaf0, &leaf1);

        // Verify leaf 0 with leaf 1 as sibling
        assert!(verify_merkle_inclusion(&leaf0, &[leaf1], &root, 0));

        // Verify leaf 1 with leaf 0 as sibling
        assert!(verify_merkle_inclusion(&leaf1, &[leaf0], &root, 1));
    }

    #[test]
    fn test_verify_merkle_inclusion_four_leaves() {
        let leaves = [[1u8; 32], [2u8; 32], [3u8; 32], [4u8; 32]];

        // Build tree
        let h01 = hash_pair(&leaves[0], &leaves[1]);
        let h23 = hash_pair(&leaves[2], &leaves[3]);
        let root = hash_pair(&h01, &h23);

        // Verify leaf 0: sibling is leaf1, then h23
        let proof0 = vec![leaves[1], h23];
        assert!(verify_merkle_inclusion(&leaves[0], &proof0, &root, 0));

        // Verify leaf 3: sibling is leaf2, then h01
        let proof3 = vec![leaves[2], h01];
        assert!(verify_merkle_inclusion(&leaves[3], &proof3, &root, 3));
    }

    #[test]
    fn test_verify_merkle_inclusion_invalid_proof() {
        let leaf = [1u8; 32];
        let wrong_sibling = [99u8; 32];
        let correct_sibling = [2u8; 32];
        let root = hash_pair(&leaf, &correct_sibling);

        // Wrong sibling should fail
        assert!(!verify_merkle_inclusion(&leaf, &[wrong_sibling], &root, 0));
    }
}
