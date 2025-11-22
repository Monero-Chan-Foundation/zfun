use log::info;
use rusqlite::{Connection, Error as SqlError, MAIN_DB, Name, ffi};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_wasm_bindgen::{from_value, to_value};
use monerochan_stark::{MONEROCHANReduceProof, StarkVerifyingKey, baby_bear_poseidon2::BabyBearPoseidon2};
use std::ptr;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::console;
use web_sys::{Request, RequestInit, RequestMode, Response};
use zfun_host_lib::{ActiveShards, create_witness_with_active_shards};
use zfun_lib::{HoldingsWitness, PROOF_DOMAIN, ProcessedHoldings, SNAPSHOT, verify_holdings};

// CDN base URL for shards
const CDN_BASE_URL: &str = "https://wasm.z.fun";

async fn fetch_active_shard_from_cdn(pool: &str, snapshot_height: u64, shard_id: u64) -> Result<Vec<u8>, JsValue> {
    let url = format!(
        "{}/snapshots/{}/{}/{}.bin",
        CDN_BASE_URL, snapshot_height, pool, shard_id
    );

    console::log_1(&format!("Attempting CDN fetch for {}: {}", pool, url).into());

    let opts = {
        let mut opts = RequestInit::new();
        opts.set_method("GET");
        opts.set_mode(RequestMode::Cors);
        opts
    };

    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| {
        JsValue::from_str(&format!("Failed to create CDN request: {:?}", e))
    })?;

    let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window object"))?;

    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;

    if !resp.ok() {
        return Err(JsValue::from_str(&format!(
            "CDN fetch failed with status: {}",
            resp.status()
        )));
    }

    let array_buffer = JsFuture::from(resp.array_buffer()?).await?;
    let uint8_array = js_sys::Uint8Array::new(&array_buffer);
    let bytes = uint8_array.to_vec();

    console::log_1(&format!("✓ Fetched {} bytes from CDN for {} shard", bytes.len(), pool).into());

    Ok(bytes)
}

async fn fetch_active_shard_from_server(base_url: &str, pool: &str) -> Result<Vec<u8>, JsValue> {
    let url = format!("{}/api/snapshots/active-shard/{}", base_url, pool);

    console::log_1(&format!("Fetching active shard for {}: {}", pool, url).into());

    let opts = {
        let mut opts = RequestInit::new();
        opts.set_method("GET");
        opts.set_mode(RequestMode::Cors);
        opts
    };

    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| {
        JsValue::from_str(&format!(
            "Failed to create request for active shard {}: {:?}",
            pool, e
        ))
    })?;

    let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window object"))?;

    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| {
            let error_msg = format!(
                "Failed to fetch active shard for {} from {}: {:?}. This may be due to a network error, CORS issue, or the server being unavailable.",
                pool, url, e
            );
            console::error_1(&error_msg.clone().into());
            JsValue::from_str(&error_msg)
        })?;

    let resp: Response = resp_value.dyn_into().map_err(|_| {
        JsValue::from_str(&format!(
            "Failed to cast response to Response for active shard {}",
            pool
        ))
    })?;

    if !resp.ok() {
        let status = resp.status();
        let status_text = resp.status_text();
        let error_msg = format!(
            "HTTP error {} ({}) when fetching active shard for {} from {}",
            status, status_text, pool, url
        );
        console::error_1(&error_msg.clone().into());
        return Err(JsValue::from_str(&error_msg));
    }

    let array_buffer = JsFuture::from(resp.array_buffer().map_err(|_| {
        JsValue::from_str(&format!(
            "No array_buffer method for active shard {} response",
            pool
        ))
    })?)
    .await
    .map_err(|e| {
        JsValue::from_str(&format!(
            "Failed to get array buffer for active shard {}: {:?}",
            pool, e
        ))
    })?;

    let uint8_array = js_sys::Uint8Array::new(&array_buffer);
    let mut bytes = vec![0u8; uint8_array.length() as usize];
    uint8_array.copy_to(&mut bytes);

    console::log_1(&format!("✓ Fetched {} bytes from server for {} shard", bytes.len(), pool).into());

    Ok(bytes)
}

async fn fetch_active_shard(base_url: &str, pool: &str) -> Result<Vec<u8>, JsValue> {
    // Get metadata to determine snapshot height and shard ID
    let metadata = fetch_metadata(base_url).await?;
    let snapshot_height = metadata["snapshot_height"]
        .as_u64()
        .ok_or_else(|| JsValue::from_str("Missing snapshot_height in metadata"))?;

    let count_key = match pool {
        "orchard" => "orchard_count",
        "sapling" => "sapling_count",
        _ => return Err(JsValue::from_str("Invalid pool")),
    };

    let count = metadata[count_key]
        .as_u64()
        .ok_or_else(|| JsValue::from_str(&format!("Missing {} in metadata", count_key)))?;

    let shard_id = count / 65536;

    // Try CDN first
    match fetch_active_shard_from_cdn(pool, snapshot_height, shard_id).await {
        Ok(bytes) => {
            console::log_1(&format!("📦 Loaded {} shard from CDN ({}x faster!)", pool, "5-10").into());
            Ok(bytes)
        }
        Err(cdn_err) => {
            console::warn_1(&format!(
                "⚠ CDN unavailable for {} shard, using server fallback: {:?}",
                pool, cdn_err
            ).into());

            // Fallback to server
            fetch_active_shard_from_server(base_url, pool).await
        }
    }
}

async fn fetch_metadata(base_url: &str) -> Result<serde_json::Value, JsValue> {
    let url = format!("{}/api/snapshots/metadata", base_url);

    console::log_1(&format!("Fetching snapshot metadata: {}", url).into());

    let opts = {
        let mut opts = RequestInit::new();
        opts.set_method("GET");
        opts.set_mode(RequestMode::Cors);
        opts
    };

    let request = Request::new_with_str_and_init(&url, &opts)
        .map_err(|e| JsValue::from_str(&format!("Failed to create request: {:?}", e)))?;

    let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window object"))?;

    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| JsValue::from_str(&format!("Fetch failed: {:?}", e)))?;

    let resp: Response = resp_value
        .dyn_into()
        .map_err(|_| JsValue::from_str("Failed to cast to Response"))?;

    if !resp.ok() {
        return Err(JsValue::from_str(&format!("HTTP error: {}", resp.status())));
    }

    let json_value = JsFuture::from(
        resp.json()
            .map_err(|_| JsValue::from_str("No json method"))?,
    )
    .await
    .map_err(|e| JsValue::from_str(&format!("Failed to parse JSON: {:?}", e)))?;

    let metadata_json: serde_json::Value = from_value(json_value)
        .map_err(|e| JsValue::from_str(&format!("Failed to deserialize JSON: {:?}", e)))?;

    console::log_1(&"Successfully fetched snapshot metadata".into());

    Ok(metadata_json)
}

async fn fetch_metadata_pool(base_url: &str, pool: &str) -> Result<serde_json::Value, JsValue> {
    let url = format!("{}/api/snapshots/metadata/{}", base_url, pool);

    console::log_1(&format!("Fetching snapshot metadata for {}: {}", pool, url).into());

    let opts = {
        let mut opts = RequestInit::new();
        opts.set_method("GET");
        opts.set_mode(RequestMode::Cors);
        opts
    };

    let request = Request::new_with_str_and_init(&url, &opts)
        .map_err(|e| JsValue::from_str(&format!("Failed to create request: {:?}", e)))?;

    let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window object"))?;

    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| JsValue::from_str(&format!("Fetch failed: {:?}", e)))?;

    let resp: Response = resp_value
        .dyn_into()
        .map_err(|_| JsValue::from_str("Failed to cast to Response"))?;

    if !resp.ok() {
        let status = resp.status();
        let status_text = resp.status_text();
        let msg = format!(
            "HTTP error {} ({}) when fetching {} metadata",
            status, status_text, pool
        );
        console::warn_1(&msg.clone().into());
        return Err(JsValue::from_str(&msg));
    }

    let json_value = JsFuture::from(
        resp.json()
            .map_err(|_| JsValue::from_str("No json method"))?,
    )
    .await
    .map_err(|e| JsValue::from_str(&format!("Failed to parse JSON: {:?}", e)))?;

    let metadata_json: serde_json::Value = from_value(json_value)
        .map_err(|e| JsValue::from_str(&format!("Failed to deserialize JSON: {:?}", e)))?;

    console::log_1(&format!("Successfully fetched {} metadata", pool).into());

    Ok(metadata_json)
}

fn validate_note_position(position: u64, pool: &str) -> Result<(), JsValue> {
    let max_position = match pool {
        "orchard" => SNAPSHOT.orchard_count - 1,
        "sapling" => SNAPSHOT.sapling_count - 1,
        _ => return Err(JsValue::from_str("Invalid pool")),
    };

    if position > max_position {
        let msg = format!(
            "Note position {} exceeds snapshot {}_count (max: {}). \
             Your wallet DB has notes that don't exist in this snapshot.",
            position, pool, max_position
        );
        console::error_1(&msg.clone().into());
        return Err(JsValue::from_str(&msg));
    }

    Ok(())
}

fn extract_note_positions(conn: &Connection) -> Result<Vec<(String, u64)>, rusqlite::Error> {
    let mut positions = Vec::new();

    // Query Orchard notes - use commitment_tree_position column and exclude spent notes
    // Spent notes are in orchard_received_note_spends table
    let mut stmt = conn.prepare(
        "SELECT rn.commitment_tree_position 
         FROM orchard_received_notes rn
         LEFT JOIN orchard_received_note_spends sp ON rn.id = sp.orchard_received_note_id
         WHERE sp.orchard_received_note_id IS NULL AND rn.commitment_tree_position IS NOT NULL",
    )?;
    let orchard_positions = stmt.query_map([], |row| {
        let pos: Option<i64> = row.get(0)?;
        Ok(pos.map(|p| ("orchard".to_string(), p as u64)))
    })?;
    positions.extend(orchard_positions.filter_map(|r| r.ok().flatten()));

    // Query Sapling notes - use commitment_tree_position column and exclude spent notes
    // Spent notes are in sapling_received_note_spends table
    let mut stmt = conn.prepare(
        "SELECT rn.commitment_tree_position 
         FROM sapling_received_notes rn
         LEFT JOIN sapling_received_note_spends sp ON rn.id = sp.sapling_received_note_id
         WHERE sp.sapling_received_note_id IS NULL AND rn.commitment_tree_position IS NOT NULL",
    )?;
    let sapling_positions = stmt.query_map([], |row| {
        let pos: Option<i64> = row.get(0)?;
        Ok(pos.map(|p| ("sapling".to_string(), p as u64)))
    })?;
    positions.extend(sapling_positions.filter_map(|r| r.ok().flatten()));

    Ok(positions)
}

fn detect_pools(note_positions: &[(String, u64)]) -> (bool, bool) {
    let mut has_orchard = false;
    let mut has_sapling = false;
    for (pool, _) in note_positions {
        if pool == "orchard" {
            has_orchard = true;
        }
        if pool == "sapling" {
            has_sapling = true;
        }
    }
    (has_orchard, has_sapling)
}

fn extract_shard_paths(
    metadata_json: &serde_json::Value,
    pool: &str,
    shard_id: u64,
) -> Option<Vec<[u8; 32]>> {
    // Handle both full metadata shape and pool-scoped shape.
    // Full: shard_paths -> pool -> shard_id -> [hex...]
    // Pool-scoped: shard_paths -> shard_id -> [hex...] (already scoped)
    let maybe_array = metadata_json
        .get("shard_paths")
        .and_then(|paths| {
            if let Some(pool_paths) = paths.get(pool) {
                pool_paths.get(&shard_id.to_string())
            } else {
                // pool-scoped: shard_paths is already the map for this pool
                paths.get(&shard_id.to_string())
            }
        })
        .and_then(|path_array| path_array.as_array());

    maybe_array.map(|path_array| {
        let mut paths = Vec::new();
        for hex_str in path_array {
            if let Some(hex) = hex_str.as_str() {
                if let Ok(bytes) = hex::decode(hex) {
                    if bytes.len() == 32 {
                        let mut node = [0u8; 32];
                        node.copy_from_slice(&bytes);
                        paths.push(node);
                    }
                }
            }
        }
        paths
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MONEROCHANStdin {
    /// Input stored as a vec of vec of bytes. It's stored this way because the read syscall reads
    /// a vec of bytes at a time.
    pub buffer: Vec<Vec<u8>>,
    pub ptr: usize,
    pub proofs: Vec<(
        MONEROCHANReduceProof<BabyBearPoseidon2>,
        StarkVerifyingKey<BabyBearPoseidon2>,
    )>,
}

#[wasm_bindgen(start)]
pub fn init_wasm() {
    let _ = console_log::init_with_level(log::Level::Debug);
    log::info!("wasm initialized");
}

impl MONEROCHANStdin {
    /// Write a value to the buffer.
    pub fn write<T: Serialize>(&mut self, data: &T) {
        let mut tmp = Vec::new();
        bincode::serialize_into(&mut tmp, data).expect("serialization failed");
        self.buffer.push(tmp);
    }
}

#[wasm_bindgen]
pub struct ProofInput {
    stdin: MONEROCHANStdin,
    snapshot: HoldingsWitness,
    processed: ProcessedHoldings,
}

#[wasm_bindgen]
impl ProofInput {
    fn new(stdin: MONEROCHANStdin, snapshot: HoldingsWitness, processed: ProcessedHoldings) -> Self {
        Self {
            stdin,
            snapshot,
            processed,
        }
    }

    #[wasm_bindgen]
    pub fn snapshot(&self) -> Result<JsValue, JsValue> {
        Ok(to_value(&self.snapshot).map_err(to_js_error)?)
    }

    #[wasm_bindgen]
    pub fn processed(&self) -> Result<JsValue, JsValue> {
        Ok(to_value(&self.processed).map_err(to_js_error)?)
    }
}

/// Builds a proof input from a connection and a base URL.
async fn build_proof_input(conn: &mut Connection, base_url: String) -> Result<ProofInput, JsValue> {
    console::log_1(&"creating snapshot".into());
    info!("height: {}", SNAPSHOT.height);
    info!("next");

    // NEW: Validate notes before fetching shards
    console::log_1(&"validating note positions against snapshot".into());

    // Extract note positions from wallet DB first (before witness generation)
    let note_positions = extract_note_positions(conn).map_err(to_js_error)?;

    // Validate each note exists in snapshot
    for (pool, position) in &note_positions {
        validate_note_position(*position, pool)?;
    }
    console::log_1(&format!("Validated {} notes", note_positions.len()).into());

    // Detect which pools have unspent notes
    let (has_orchard, has_sapling) = detect_pools(&note_positions);
    console::log_1(
        &format!(
            "Pool detection -> orchard: {}, sapling: {}",
            has_orchard, has_sapling
        )
        .into(),
    );

    // Fetch active shards from server only for pools in use
    console::log_1(&"fetching active shards".into());
    let orchard_active = if has_orchard {
        Some(
            fetch_active_shard(&base_url, "orchard")
                .await
                .map_err(|e| {
                    JsValue::from_str(&format!("Failed to fetch Orchard active shard: {:?}", e))
                })?,
        )
    } else {
        console::log_1(&"Skipping Orchard shard fetch (no Orchard notes)".into());
        None
    };

    let sapling_active = if has_sapling {
        Some(
            fetch_active_shard(&base_url, "sapling")
                .await
                .map_err(|e| {
                    JsValue::from_str(&format!("Failed to fetch Sapling active shard: {:?}", e))
                })?,
        )
    } else {
        console::log_1(&"Skipping Sapling shard fetch (no Sapling notes)".into());
        None
    };

    console::log_1(
        &format!(
            "Fetched Orchard: {} bytes, Sapling: {} bytes",
            orchard_active.as_ref().map(|v| v.len()).unwrap_or(0),
            sapling_active.as_ref().map(|v| v.len()).unwrap_or(0)
        )
        .into(),
    );

    // Calculate active shard IDs
    let orchard_shard_id = SNAPSHOT.orchard_count / 65536;
    let sapling_shard_id = SNAPSHOT.sapling_count / 65536;

    // Fetch metadata to get shard_paths for active shards (pool-scoped first, full fallback)
    console::log_1(&"fetching snapshot metadata for shard_paths".into());
    let mut orchard_shard_paths = None;
    let mut sapling_shard_paths = None;
    let mut full_metadata: Option<serde_json::Value> = None;

    if has_orchard {
        match fetch_metadata_pool(&base_url, "orchard").await {
            Ok(meta) => {
                orchard_shard_paths = extract_shard_paths(&meta, "orchard", orchard_shard_id);
            }
            Err(_) => {
                console::warn_1(&"Falling back to full metadata for Orchard shard_paths".into());
                let meta = if let Some(ref m) = full_metadata {
                    m.clone()
                } else {
                    let m = fetch_metadata(&base_url).await?;
                    full_metadata = Some(m.clone());
                    m
                };
                orchard_shard_paths = extract_shard_paths(&meta, "orchard", orchard_shard_id);
            }
        }
        if let Some(paths) = orchard_shard_paths.as_ref() {
            console::log_1(
                &format!(
                    "Loaded {} shard_paths for Orchard shard {}",
                    paths.len(),
                    orchard_shard_id
                )
                .into(),
            );
        }
    }

    if has_sapling {
        match fetch_metadata_pool(&base_url, "sapling").await {
            Ok(meta) => {
                sapling_shard_paths = extract_shard_paths(&meta, "sapling", sapling_shard_id);
            }
            Err(_) => {
                console::warn_1(&"Falling back to full metadata for Sapling shard_paths".into());
                let meta = if let Some(ref m) = full_metadata {
                    m.clone()
                } else {
                    let m = fetch_metadata(&base_url).await?;
                    full_metadata = Some(m.clone());
                    m
                };
                sapling_shard_paths = extract_shard_paths(&meta, "sapling", sapling_shard_id);
            }
        }
        if let Some(paths) = sapling_shard_paths.as_ref() {
            console::log_1(
                &format!(
                    "Loaded {} shard_paths for Sapling shard {}",
                    paths.len(),
                    sapling_shard_id
                )
                .into(),
            );
        }
    }

    // Create ActiveShards struct to pass to witness generation
    let active_shards = ActiveShards {
        orchard: orchard_active,
        sapling: sapling_active,
        orchard_shard_paths,
        sapling_shard_paths,
    };

    // Use create_witness_with_active_shards to inject the fetched shards
    // Note: For WASM, we pass the base_url which will be used to fetch shard_paths if needed
    // The active shards provide the shard-level paths (16 levels), but we still need
    // the root paths (levels 16-31) from metadata.shard_paths for complete witnesses.
    // For now, this will work for notes in active shards when wallet DB roots match,
    // but may fail for notes requiring full 32-level paths.
    let snapshot = create_witness_with_active_shards(
        conn,
        Some(base_url.clone()),
        None, // No snapshot_dir in WASM - active shards will be used via InMemoryWitnessSource
        Some(active_shards),
    )
    .await
    .map_err(to_js_error)?;

    console::log_1(&"snapshot created".into());
    let processed =
        verify_holdings(&snapshot, &SNAPSHOT, PROOF_DOMAIN, false).map_err(to_js_error)?;
    console::log_1(&"processed created".into());
    let mut stdin = MONEROCHANStdin::default();
    stdin.write(&snapshot);
    Ok(ProofInput::new(stdin, snapshot, processed))
}

/// Loads a proof input from a database bytes and a base URL.
#[wasm_bindgen]
pub async fn load_processed_input(
    db_bytes: &[u8],
    base_url: String,
) -> Result<ProofInput, JsValue> {
    console::log_1(&"loading processed input".into());
    let mut conn = connection_from_bytes(db_bytes).map_err(to_js_error)?;
    console::log_1(&"connection from bytes".into());
    build_proof_input(&mut conn, base_url).await
}

/// Gets the snapshot metadata.
#[wasm_bindgen]
pub fn get_snapshot_metadata() -> Result<JsValue, JsValue> {
    Ok(to_value(&SNAPSHOT).map_err(to_js_error)?)
}

/// Requests a proof from the server.
#[wasm_bindgen]
pub async fn request_proof(
    server_base_url: String,
    tee_base_url: String,
    proof_input: ProofInput,
    address: String,
    signature: String,
    message: String,
    timestamp: String,
    is_tee: bool,
) -> Result<JsValue, JsValue> {
    console::log_1(&format!("request_proof: {:?}", proof_input.stdin.buffer.len()).into());
    if is_tee {
        // Check API for queue status
        let queue_full = reqwest::Client::builder()
            .build()
            .map_err(to_js_error)?
            .get(format!("{}/queue", server_base_url))
            .fetch_credentials_omit()
            .send()
            .await
            .map_err(to_js_error)?
            .json::<bool>()
            .await
            .map_err(to_js_error)?;
        if queue_full {
            return Err(JsValue::from_str(
                "TEE Prover capacity is at maximum. Please try again later.",
            ));
        }
    }
    // Check API for used hashes
    let hashes = proof_input
        .processed
        .alternate_nullifiers
        .iter()
        .chain(proof_input.processed.transparent_utxo_commitments.iter())
        .map(|h| hex::encode(h))
        .collect::<Vec<_>>();
    let body = json!({ "hashes": hashes });
    let response = reqwest::Client::builder()
        .build()
        .map_err(to_js_error)?
        .post(format!("{}/hashes", server_base_url))
        .json(&body)
        .fetch_credentials_omit()
        .send()
        .await
        .map_err(to_js_error)?
        .json::<bool>()
        .await
        .map_err(to_js_error)?;
    if response {
        // Demo mode: allow re-verification
        // In production, this would prevent double-counting
        web_sys::console::warn_1(&JsValue::from_str(
            "Note: Some nullifiers from this wallet have already been verified. In production this would be blocked to prevent double-counting, but this is a demo so you can verify as many times as you want."
        ));
    }
    // Get URL to upload stdin to
    let response = reqwest::Client::builder()
        .build()
        .map_err(to_js_error)?
        .post(format!("{}/artifact/{}", server_base_url, is_tee))
        .fetch_credentials_omit()
        .send()
        .await
        .map_err(to_js_error)?
        .json::<serde_json::Value>()
        .await
        .map_err(to_js_error)?;
    // Validate artifact presigned URL and URI for TEE
    if is_tee {
        if !response["artifact_presigned_url"]
            .as_str()
            .unwrap_or_default()
            .starts_with(&tee_base_url)
        {
            return Err(JsValue::from_str("invalid artifact presigned URL"));
        }
        if !response["artifact_uri"]
            .as_str()
            .unwrap_or_default()
            .starts_with(&tee_base_url)
        {
            return Err(JsValue::from_str("invalid artifact URI"));
        }
    }
    
    // Serialize stdin
    let encoded = bincode::serialize(&proof_input.stdin).map_err(to_js_error)?;
    
    // For non-TEE mode, send stdin inline as base64
    // For TEE mode, upload to presigned URL and send URI
    let (stdin_base64, stdin_uri_opt) = if is_tee {
        // Upload stdin for TEE mode
        let presigned_url = response["artifact_presigned_url"]
            .as_str()
            .ok_or_else(|| JsValue::from_str("missing artifact presigned URL"))?;
        reqwest::Client::builder()
            .build()
            .map_err(to_js_error)?
            .put(presigned_url)
            .body(encoded.clone())
            .fetch_credentials_omit()
            .send()
            .await
            .map_err(to_js_error)?
            .text()
            .await
            .map_err(to_js_error)?;
        
        let artifact_uri = response["artifact_uri"]
            .as_str()
            .ok_or_else(|| JsValue::from_str("missing artifact uri"))?;
        (None, Some(artifact_uri.to_string()))
    } else {
        // For non-TEE mode, send stdin inline as base64
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        (Some(STANDARD.encode(&encoded)), None)
    };
    
    let timestamp_u64: u64 = timestamp
        .parse()
        .map_err(|_| JsValue::from_str("Invalid timestamp format"))?;

    let post_url = format!("{}/proof", server_base_url);
    let mut request_body = json!({
        "address": address,
        "signature": signature,
        "message": message,
        "timestamp": timestamp_u64,
        "is_tee": is_tee,
        "amount": proof_input.processed.total_zatoshis
    });
    
    // Add either stdin or stdin_uri depending on mode
    if let Some(stdin) = stdin_base64 {
        request_body["stdin"] = json!(stdin);
    }
    if let Some(uri) = stdin_uri_opt {
        request_body["stdin_uri"] = json!(uri);
    }

    let response2 = reqwest::Client::builder()
        .build()
        .map_err(to_js_error)?
        .post(&post_url)
        .json(&request_body)
        .fetch_credentials_omit()
        .send()
        .await
        .map_err(to_js_error)?
        .json::<serde_json::Value>()
        .await
        .map_err(to_js_error)?;
    Ok(to_value(&response2).map_err(to_js_error)?)
}

trait SqliteDeserialize {
    fn deserialize_owned_bytes<N: Name>(&mut self, schema: N, data: &[u8]) -> rusqlite::Result<()>;
    fn deserialize_<N: Name>(
        &mut self,
        schema: N,
        data: *mut u8,
        sz: ffi::sqlite_int64,
        flags: std::ffi::c_uint,
    ) -> rusqlite::Result<()>;
}

impl SqliteDeserialize for Connection {
    fn deserialize_owned_bytes<N: Name>(&mut self, schema: N, data: &[u8]) -> rusqlite::Result<()> {
        let sz_i64: ffi::sqlite_int64 = data
            .len()
            .try_into()
            .expect("database size exceeds supported range");
        let raw = unsafe { ffi::sqlite3_malloc(sz_i64.try_into().unwrap()) } as *mut u8;
        if raw.is_null() {
            return Err(SqlError::SqliteFailure(
                ffi::Error {
                    code: ffi::ErrorCode::CannotOpen,
                    extended_code: ffi::SQLITE_CANTOPEN,
                },
                Some("sqlite3_malloc failed".into()),
            ));
        }

        unsafe { ptr::copy_nonoverlapping(data.as_ptr(), raw, sz_i64 as usize) };

        let result = self.deserialize_(
            schema,
            raw,
            sz_i64,
            ffi::SQLITE_DESERIALIZE_FREEONCLOSE | ffi::SQLITE_DESERIALIZE_READONLY,
        );

        if result.is_err() {
            unsafe {
                ffi::sqlite3_free(raw.cast());
            }
        }

        result
    }

    fn deserialize_<N: Name>(
        &mut self,
        schema: N,
        data: *mut u8,
        sz: ffi::sqlite_int64,
        flags: std::ffi::c_uint,
    ) -> rusqlite::Result<()> {
        let schema = schema.as_cstr()?;
        let rc = unsafe {
            ffi::sqlite3_deserialize(self.handle(), schema.as_ptr(), data, sz, sz, flags)
        };
        if rc != ffi::SQLITE_OK {
            return Err(rusqlite::Error::InvalidParameterName(format!(
                "unknown error: {}",
                rc
            )));
        }
        Ok(())
    }
}

fn connection_from_bytes(db_bytes: &[u8]) -> Result<Connection, rusqlite::Error> {
    console::log_1(&"opening connection".into());
    let mut conn = Connection::open_in_memory()?;
    console::log_1(&"loading array module".into());
    rusqlite::vtab::array::load_module(&conn)?;
    console::log_1(&"deserializing bytes".into());
    conn.deserialize_owned_bytes(MAIN_DB, db_bytes)?;
    console::log_1(&"bytes deserialized".into());
    Ok(conn)
}

fn to_js_error<E: std::fmt::Display>(err: E) -> JsValue {
    JsValue::from_str(&err.to_string())
}
