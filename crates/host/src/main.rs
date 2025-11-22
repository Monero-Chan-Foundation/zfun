use eyre::{Result, eyre};
use rusqlite::Connection;

use monerochan::network::{FulfillmentStrategy, NetworkMode};
use monerochan::{EnvProver, MONEROCHANStdin, Prover, ProverClient};

use std::{env, path::PathBuf, process};
use zfun_elf::ZFUN_PROGRAM_ELF;
use zfun_host_lib::create_witness;
use zfun_lib::{PROOF_DOMAIN, SNAPSHOT, format_hex, verify_holdings};

fn log_decoded_public_values(bytes: &[u8]) {
    const EXPECTED_LEN: usize = 8 + 8 + 32 + 8;
    if bytes.len() < EXPECTED_LEN {
        println!(
            "⚠️  Unable to decode public values: expected at least {} bytes, got {}",
            EXPECTED_LEN,
            bytes.len()
        );
        return;
    }

    let total_zatoshis = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
    let note_count = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    let alt_nf = &bytes[16..48];
    let padding = u64::from_le_bytes(bytes[48..56].try_into().unwrap());

    println!("Decoded public values:");
    println!("  total_zatoshis: {}", total_zatoshis);
    println!("  shielded_note_count: {}", note_count);
    println!("  alternate_nullifier: {}", hex::encode(alt_nf));
    println!("  padding: {}", padding);
}

struct CliOptions {
    db_path: PathBuf,
    prove: bool,
    use_network: bool,
    snapshot_dir: Option<PathBuf>,
}

fn print_usage() {
    println!("Usage: zfun-host <db-path> [--prove] [--network] [--snapshot-dir <dir>]");
    println!("  <db-path>              Path to the wallet SQLite database (required)");
    println!("  --prove                Execute the proof (defaults to false)");
    println!("  --prove=true|false     Explicitly enable or disable proof execution");
    println!("  --network              Use Monerochan Network Prover (defaults to local EnvProver)");
    println!("  --network=true|false   Explicitly enable or disable monerochan network prover");
    println!("  --snapshot-dir <dir>   Use snapshot shards for Merkle witnesses (required)");
    println!("  -h, --help             Show this help message");
    println!();
    println!("Environment Variables:");
    println!("  BASE_PRIVATE_KEY       Monerochan Network API key (required for --network)");
    println!("                        Works with Monerochan backend");
    println!(
        "  MONEROCHAN_PROVER      (Monerochan only) Can be set to 'network', 'local', 'mock', or 'cpu'"
    );
    println!("                        (used by EnvProver for Monerochan backend)");
}

fn parse_args() -> Result<CliOptions> {
    let mut args = env::args().skip(1);
    let mut db_path: Option<PathBuf> = None;
    let mut prove = false;
    let mut use_network = false;
    let mut snapshot_dir: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                process::exit(0);
            }
            _ if arg.starts_with("--prove=") => {
                let value = &arg["--prove=".len()..];
                prove = match value {
                    "true" | "1" => true,
                    "false" | "0" => false,
                    _ => {
                        return Err(eyre!(
                            "invalid value for --prove: {} (expected true/false)",
                            value
                        ));
                    }
                };
            }
            "--prove" => {
                prove = true;
            }
            _ if arg.starts_with("--network=") => {
                let value = &arg["--network=".len()..];
                use_network = match value {
                    "true" | "1" => true,
                    "false" | "0" => false,
                    _ => {
                        return Err(eyre!(
                            "invalid value for --network: {} (expected true/false)",
                            value
                        ));
                    }
                };
            }
            "--network" => {
                use_network = true;
            }
            "--snapshot-dir" => {
                if let Some(path) = args.next() {
                    snapshot_dir = Some(PathBuf::from(path));
                } else {
                    return Err(eyre!("expected path after --snapshot-dir"));
                }
            }
            "--" => {
                if let Some(path) = args.next() {
                    if db_path.is_some() {
                        return Err(eyre!("only one <db-path> argument is allowed"));
                    }
                    db_path = Some(PathBuf::from(path));
                } else {
                    return Err(eyre!("expected path after '--'"));
                }
            }
            value if value.starts_with("--") => {
                return Err(eyre!("unrecognized option '{}'", value));
            }
            value => {
                if db_path.is_some() {
                    return Err(eyre!("only one <db-path> argument is allowed"));
                }
                db_path = Some(PathBuf::from(value));
            }
        }
    }

    let Some(db_path) = db_path else {
        print_usage();
        return Err(eyre!("missing required <db-path> argument"));
    };

    Ok(CliOptions {
        db_path,
        prove,
        use_network,
        snapshot_dir,
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file
    dotenvy::dotenv().ok();

    monerochan::utils::setup_logger();

    let cli = match parse_args() {
        Ok(cli) => cli,
        Err(err) => {
            eprintln!("Error: {err}");
            process::exit(1);
        }
    };

    let mut conn = Connection::open(&cli.db_path)?;
    rusqlite::vtab::array::load_module(&conn)?;

    let mut snapshot = create_witness(&mut conn, None, cli.snapshot_dir.as_deref()).await?;

    // Set nonce based on current timestamp to avoid prover-gateway idempotency cache
    // when resubmitting after fixing bugs
    snapshot.nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    println!("witness nonce: {} (forces new job hash)", snapshot.nonce);

    // Diagnostic: Validate ufvk encoding before serialization
    for (i, account) in snapshot.accounts.iter().enumerate() {
        // Validate that the bytes can be decoded as UTF-8 (for UFVK decoding)
        match std::str::from_utf8(&account.ufvk) {
            Ok(ufvk_str) => {
                // Validate that it's a valid Zcash UFVK encoding
                match account.decode_ufvk() {
                    Ok(_) => {
                        // Valid UFVK encoding
                    }
                    Err(e) => {
                        eprintln!(
                            "⚠️  WARNING: Account {} ufvk is not a valid Zcash UFVK encoding: {:?}",
                            i, e
                        );
                        eprintln!("   ufvk value (as string): {}", ufvk_str);
                    }
                }
            }
            Err(e) => {
                eprintln!(
                    "⚠️  WARNING: Account {} ufvk bytes are not valid UTF-8: valid up to byte {}",
                    i,
                    e.valid_up_to()
                );
                eprintln!("   ufvk length: {} bytes", account.ufvk.len());
                eprintln!(
                    "   ufvk hex (first 32 bytes): {}",
                    hex::encode(&account.ufvk[..std::cmp::min(32, account.ufvk.len())])
                );
            }
        }

        // Check for null bytes (possible corruption)
        if account.ufvk.len() > 0 && account.ufvk[0] == 0 {
            eprintln!(
                "⚠️  WARNING: Account {} ufvk starts with null byte (possible corruption)",
                i
            );
        }
    }

    // Use actual roots from the witness (wallet/snapshot) instead of hardcoded SNAPSHOT
    let snapshot_meta = snapshot
        .actual_snapshot
        .as_ref()
        .ok_or_else(|| eyre!("No snapshot metadata available - wallet may be incomplete"))?;

    let processed = verify_holdings(&snapshot, snapshot_meta, PROOF_DOMAIN, false)?;

    println!(
        "total balance: {} zatoshis ({} shielded notes, {} transparent UTXOs)",
        processed.total_zatoshis,
        processed.alternate_nullifiers.len(),
        processed.transparent_utxo_commitments.len(),
    );
    for nf in &processed.alternate_nullifiers {
        println!("  alternate nullifier: {}", format_hex(nf));
    }
    for utxo_commitment in &processed.transparent_utxo_commitments {
        println!(
            "  transparent UTXO commitment: {}",
            format_hex(utxo_commitment)
        );
    }

    println!("📊 Snapshot info before serialization:");
    println!("   Accounts: {}", snapshot.accounts.len());
    println!("   Nonce: {}", snapshot.nonce);

    let mut input = MONEROCHANStdin::new();
    input.write(&snapshot);

    // Choose prover based on CLI flag
    if cli.use_network {
        println!("Using Monerochan Network Prover (Reserved Capacity mode)...");

        // Monerochan checks MONEROCHAN_NETWORK_PRIVATE_KEY first, then BASE_PRIVATE_KEY
        
        let network_key = env::var("MONEROCHAN_NETWORK_PRIVATE_KEY")
            .ok()
            .or_else(|| env::var("BASE_PRIVATE_KEY").ok())
            .unwrap_or_else(|| {
                eprintln!("Error: MONEROCHAN_NETWORK_PRIVATE_KEY or BASE_PRIVATE_KEY environment variable not set");
                eprintln!("Please set one in .env or export it:");
                eprintln!("  export MONEROCHAN_NETWORK_PRIVATE_KEY=your_api_key_here");
                eprintln!("  or");
                eprintln!("  export BASE_PRIVATE_KEY=your_api_key_here");
                process::exit(1);
            });

        println!(
            "Using account: {}...",
            &network_key[..10.min(network_key.len())]
        );

        // Set NETWORK_PRIVATE_KEY from BASE_PRIVATE_KEY for compatibility
        unsafe {
            env::set_var("NETWORK_PRIVATE_KEY", &network_key);
        }

        let client = ProverClient::builder()
            .network_for(NetworkMode::Reserved)
            .build();

        if cli.prove {
            println!("🚀 Submitting proof request to Monerochan Network (Reserved)...\n");

            let (pk, vk) = client.setup(ZFUN_PROGRAM_ELF);

            // Submit proof request and get request_id
            // request_id is the monerochan network request ID (hex string with 0x prefix, 66 chars)
            let request_id = client
                .prove(&pk, &input)
                .groth16()
                .strategy(FulfillmentStrategy::Reserved)
                .request_async()
                .await
                .map_err(|e| eyre!("failed to submit proof request: {}", e))?;

            println!("📋 Proof Request ID: {}", request_id);
            println!(
                "🔍 View on Explorer: https://explorer.monero-chan.org/proof/{}",
                request_id
            );
            println!("⏳ Waiting for proof to complete...\n");

            // Wait for proof to complete
            let proof = client
                .wait_proof(request_id, None, None)
                .await
                .expect("failed to retrieve proof");

            println!("\n✓ Proof generation completed!");
            println!(
                "public values: {}",
                hex::encode(proof.public_values.to_vec())
            );
            log_decoded_public_values(&proof.public_values.to_vec());

            proof.save("proof.bin").expect("failed to save proof");
            println!("✓ Proof saved to proof.bin");

            // Skip verification if Docker is not available (required for Groth16 verification)
            if std::process::Command::new("docker")
                .arg("info")
                .output()
                .is_ok()
            {
                match client.verify(&proof, &vk) {
                    Ok(_) => println!("✓ Proof verified successfully"),
                    Err(e) => println!("✗ Proof verification failed: {}", e),
                }
            } else {
                println!("⚠️  Skipping local proof verification (Docker not available)");
                println!("   The proof was already verified by the monerochan network prover.");
            }
        } else {
            // Only execute locally if not proving (to see the output)
            println!("Executing locally to get public values...");
            let (public_values, execution_report) =
                client.execute(ZFUN_PROGRAM_ELF, &input).run().unwrap();
            println!("execution report: {:?}", execution_report);
            println!("public values: {}", hex::encode(public_values.to_vec()));
            log_decoded_public_values(&public_values.to_vec());

            println!("\nSkipping proof generation (pass --prove to enable).");
            println!("Note: With --prove, execution + proving happens on Monerochan Network");
        }
    } else {
        println!("Using local EnvProver (set MONEROCHAN_PROVER env var to configure)...");

        let client = EnvProver::new();
        let (public_values, execution_report) =
            client.execute(ZFUN_PROGRAM_ELF, &input).run().unwrap();
        println!("execution report: {:?}", execution_report);
        println!("public values: {}", hex::encode(public_values.to_vec()));
        log_decoded_public_values(&public_values.to_vec());

        if cli.prove {
            let (pk, vk) = client.setup(ZFUN_PROGRAM_ELF);
            let proof = client
                .prove(&pk, &input)
                .groth16()
                .run()
                .expect("proof generation failed");

            proof.save("proof.bin").expect("failed to save proof");
            println!("proof saved to proof.bin");

            match client.verify(&proof, &vk) {
                Ok(_) => println!("proof verified"),
                Err(e) => println!("proof verification failed: {}", e),
            }
        } else {
            println!("Skipping proof generation (pass --prove to enable).");
        }
    }

    Ok(())
}
