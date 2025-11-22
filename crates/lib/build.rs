use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    // Register the custom cfg flag
    println!("cargo:rustc-check-cfg=cfg(snapshot_from_json)");

    // Look for snapshot_metadata.json in repo root
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let repo_root = PathBuf::from(&manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let metadata_path = repo_root.join("snapshot_metadata.json");

    // Tell cargo to rerun if snapshot_metadata.json changes
    println!("cargo:rerun-if-changed={}", metadata_path.display());

    if metadata_path.exists() {
        println!(
            "cargo:warning=Using snapshot metadata from: {}",
            metadata_path.display()
        );

        // Read and parse the JSON
        let metadata_str =
            fs::read_to_string(&metadata_path).expect("Failed to read snapshot_metadata.json");

        let metadata: serde_json::Value =
            serde_json::from_str(&metadata_str).expect("Failed to parse snapshot_metadata.json");

        // Extract values, handling both "height" and "snapshot_height"
        let height = metadata
            .get("height")
            .or_else(|| metadata.get("snapshot_height"))
            .and_then(|v| v.as_u64())
            .expect("Missing height in snapshot_metadata.json") as u32;

        let orchard_root = metadata
            .get("orchard_root")
            .and_then(|v| v.as_str())
            .expect("Missing orchard_root in snapshot_metadata.json");

        let sapling_root = metadata
            .get("sapling_root")
            .and_then(|v| v.as_str())
            .expect("Missing sapling_root in snapshot_metadata.json");

        let orchard_count = metadata
            .get("orchard_count")
            .and_then(|v| v.as_u64())
            .expect("Missing orchard_count in snapshot_metadata.json")
            as u64;

        let sapling_count = metadata
            .get("sapling_count")
            .and_then(|v| v.as_u64())
            .expect("Missing sapling_count in snapshot_metadata.json")
            as u64;

        // Optional fields with defaults
        let utxo_root = metadata
            .get("utxo_root")
            .and_then(|v| v.as_str())
            .unwrap_or("3602a0a9fb1b3b7751783d08995c7b5581b882666a168394fbdfca0b645d9fe0");

        let nullifier_root = metadata
            .get("nullifier_root")
            .and_then(|v| v.as_str())
            .unwrap_or("b11dbaad2c64f957594a129757a24e0a7ae85a3a5672cb3248f3ca48784ebebd");

        let utxo_count = metadata
            .get("utxo_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(27918949) as u64;

        let nullifier_count = metadata
            .get("nullifier_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(51852159) as u64;

        // Generate Rust code file with constants
        let out_dir = env::var("OUT_DIR").unwrap();
        let dest_path = PathBuf::from(out_dir).join("snapshot_constants.rs");

        let code = format!(
            r#"// Auto-generated from snapshot_metadata.json
pub const SNAPSHOT: SnapshotMetadata = SnapshotMetadata {{
    height: {},
    utxo_root: hex!("{}"),
    nullifier_root: hex!("{}"),
    sapling_root: hex!("{}"),
    orchard_root: hex!("{}"),
    utxo_count: {},
    nullifier_count: {},
    sapling_count: {},
    orchard_count: {},
}};
"#,
            height,
            utxo_root,
            nullifier_root,
            sapling_root,
            orchard_root,
            utxo_count,
            nullifier_count,
            sapling_count,
            orchard_count
        );

        fs::write(&dest_path, code).expect("Failed to write snapshot_constants.rs");

        // Enable the cfg flag so code knows to include the generated file
        println!("cargo:rustc-cfg=snapshot_from_json");

        println!(
            "cargo:warning=Snapshot metadata loaded: height={}, orchard={}, sapling={}",
            height, orchard_count, sapling_count
        );
    } else {
        println!("cargo:warning=snapshot_metadata.json not found, using hardcoded values");
    }
}
