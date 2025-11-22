// Test to verify MerklePath::root() behavior with known-good paths
// This helps debug why our path construction doesn't match MerklePath::root()

use hex;
use incrementalmerkletree::{Hashable, Level, MerklePath, Position};
use sapling::Node as SaplingNode;

#[test]
fn test_merkle_path_root_simple() {
    // Create a simple 3-level tree manually
    // Level 0: leaves [A, B, C, D]
    // Level 1: [AB, CD]
    // Level 2: [ABCD]

    // Position 0 (binary: 00) - leftmost leaf
    // Path should be: [B (sibling at level 0), CD (sibling at level 1)]

    let leaf_a = SaplingNode::from_bytes([0u8; 32]).unwrap();
    let leaf_b = SaplingNode::from_bytes([1u8; 32]).unwrap();
    let leaf_c = SaplingNode::from_bytes([2u8; 32]).unwrap();
    let leaf_d = SaplingNode::from_bytes([3u8; 32]).unwrap();

    // Build tree manually
    let ab = SaplingNode::combine(Level::new(0), &leaf_a, &leaf_b);
    let cd = SaplingNode::combine(Level::new(0), &leaf_c, &leaf_d);
    let abcd = SaplingNode::combine(Level::new(1), &ab, &cd);

    // For position 0, path is [B, CD]
    let path = vec![leaf_b, cd];
    let position = Position::from(0);

    // Test MerklePath::root()
    let merkle_path = MerklePath::<SaplingNode, 2>::from_parts(path.clone(), position).unwrap();
    let root_from_merklepath = merkle_path.root(leaf_a);

    // Manual computation matching MerklePath::root() logic
    let mut current = leaf_a;
    for (i, sibling) in path.iter().enumerate() {
        let bit = (u64::from(position) >> i) & 1;
        current = if bit == 0 {
            SaplingNode::combine(Level::new(i as u8), &current, sibling)
        } else {
            SaplingNode::combine(Level::new(i as u8), sibling, &current)
        };
    }

    println!("Expected root: {}", hex::encode(abcd.to_bytes()));
    println!(
        "MerklePath::root() result: {}",
        hex::encode(root_from_merklepath.to_bytes())
    );
    println!(
        "Manual computation result: {}",
        hex::encode(current.to_bytes())
    );

    assert_eq!(
        root_from_merklepath.to_bytes(),
        abcd.to_bytes(),
        "MerklePath::root() should match expected"
    );
    assert_eq!(
        current.to_bytes(),
        abcd.to_bytes(),
        "Manual computation should match expected"
    );
    assert_eq!(
        root_from_merklepath.to_bytes(),
        current.to_bytes(),
        "Both methods should agree"
    );
}

#[test]
fn test_merkle_path_root_position_1() {
    // Position 1 (binary: 01) - second leaf
    // Path should be: [A (sibling at level 0), CD (sibling at level 1)]

    let leaf_a = SaplingNode::from_bytes([0u8; 32]).unwrap();
    let leaf_b = SaplingNode::from_bytes([1u8; 32]).unwrap();
    let leaf_c = SaplingNode::from_bytes([2u8; 32]).unwrap();
    let leaf_d = SaplingNode::from_bytes([3u8; 32]).unwrap();

    let ab = SaplingNode::combine(Level::new(0), &leaf_a, &leaf_b);
    let cd = SaplingNode::combine(Level::new(0), &leaf_c, &leaf_d);
    let abcd = SaplingNode::combine(Level::new(1), &ab, &cd);

    // For position 1, path is [A, CD]
    let path = vec![leaf_a, cd];
    let position = Position::from(1);

    let merkle_path = MerklePath::<SaplingNode, 2>::from_parts(path.clone(), position).unwrap();
    let root_from_merklepath = merkle_path.root(leaf_b);

    // Manual computation
    let mut current = leaf_b;
    for (i, sibling) in path.iter().enumerate() {
        let bit = (u64::from(position) >> i) & 1;
        current = if bit == 0 {
            SaplingNode::combine(Level::new(i as u8), &current, sibling)
        } else {
            SaplingNode::combine(Level::new(i as u8), sibling, &current)
        };
    }

    println!(
        "Position 1 - Expected root: {}",
        hex::encode(abcd.to_bytes())
    );
    println!(
        "Position 1 - MerklePath::root() result: {}",
        hex::encode(root_from_merklepath.to_bytes())
    );
    println!(
        "Position 1 - Manual computation result: {}",
        hex::encode(current.to_bytes())
    );

    assert_eq!(root_from_merklepath.to_bytes(), abcd.to_bytes());
    assert_eq!(current.to_bytes(), abcd.to_bytes());
    assert_eq!(root_from_merklepath.to_bytes(), current.to_bytes());
}
