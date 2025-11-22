mod proof_service;
mod proof_store;
mod snapshot_updater;

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use clap::Parser;
use ed25519_dalek::{Signature, SignatureError, VerifyingKey};
use eyre::{Result, eyre};
use proof_service::ProofService;
use proof_store::{PendingRequest, ProofStore};
use prost::Message;
use rusqlite::ErrorCode;
use serde::{Deserialize, Serialize};

use monerochan::{
    CpuProver, MONEROCHAN_CIRCUIT_VERSION,
    MONEROCHANVerifyingKey, NetworkSigner, Prover, ProverClient,
    network::{
        FulfillmentStrategy, NetworkMode,
        proto::{
            artifact::{
                ArtifactType, CreateArtifactRequest, CreateArtifactResponse,
                artifact_store_client::ArtifactStoreClient,
            },
            // Use MonerochanNetwork client instead of SP1's ProverNetwork client
            api::{
                monerochan_network_client::MonerochanNetworkClient,
                FulfillmentStrategy as ApiStrategy, ProofMode as ApiProofMode,
                RequestProofRequest,
                GetNonceRequest as ApiGetNonceRequest,
                GetProgramRequest as ApiGetProgramRequest,
                CreateProgramRequest as ApiCreateProgramRequest,
                GetProofStatusRequest as ApiGetProofStatusRequest,
            },
            base_types::{
                FulfillmentStatus, GetProofRequestStatusResponse,
            },
        },
    },
    proof::{MONEROCHANProofWithPublicValues, ProofFromNetwork},
};
use std::{
    net::SocketAddr,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    signal,
    time::{Duration, interval},
};
use tonic::{
    Code,
    transport::{Channel, ClientTlsConfig, Endpoint},
};
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};
use tracing::{debug, info, warn};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use zfun_elf::ZFUN_PROGRAM_ELF;
use zfun_lib::ProcessedHoldings;

const MAX_QUEUE_SIZE: u64 = 1000;

/// Z.FUN Server - Privacy-preserving proof generation server
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Snapshot directory path (e.g., ./snapshots/snapshot-3140000)
    #[arg(long, env = "SNAPSHOT_DIR")]
    snapshot_dir: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command-line arguments
    let args = Args::parse();

    // Load .env file if it exists (ignore errors if file doesn't exist)
    let _ = dotenv::dotenv();

    let _guard = init_tracing();

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls default crypto provider - TLS will not work");

    tracing::info!("initializing zfun server");
    let config = ProverConfig::from_env()?;
    tracing::info!(
        bind_address = %config.bind_address,
        tee_enabled = config.tee_private_key.is_some(),
        tee_rpc_url = %config.tee_rpc_url,
        base_rpc_url = %config.base_rpc_url,
        "configuration loaded"
    );
    tracing::info!("setting up prover and verifying key");
    let prover = Arc::new(ProverClient::builder().cpu().build());
    let (_, vk) = prover.setup(ZFUN_PROGRAM_ELF);
    let vk = Arc::new(vk);
    tracing::info!("prover setup complete");

    tracing::info!("initializing prover service");
    let service = Arc::new(ProverService::new(vk.clone(), &config).await?);
    if config.tee_private_key.is_some() {
        tracing::info!("setting up TEE program");
        if let Err(e) = service.setup_program(true).await {
            warn!("Failed to setup TEE program: {}", e);
        } else {
            if let Ok(signer) = service.signer(true) {
                info!("tee signer: {:?}", signer.address());
            }
        }
    } else {
        info!("TEE mode disabled (TEE_PRIVATE_KEY not set)");
    }
    tracing::info!("setting up base program");
    if let Err(e) = service.setup_program(false).await {
        warn!(
            "Failed to setup base program (this may be expected if program management is not available): {}",
            e
        );
    } else {
        info!("base signer: {:?}", service.signer(false)?.address());
    }

    // Create proof service
    let data_dir: std::path::PathBuf = std::env::var("DATA_DIR")
        .unwrap_or_else(|_| "./data".to_string())
        .into();

    // Get snapshot directory from CLI args (preferred) or fallback to default
    let snapshot_dir = args
        .snapshot_dir
        .unwrap_or_else(|| "./snapshots/snapshot-3140000".into());

    tracing::info!(
        data_dir = %data_dir.display(),
        snapshot_dir = %snapshot_dir.display(),
        "initializing proof service"
    );
    let proof_service = Arc::new(ProofService::new(data_dir, snapshot_dir)?);
    tracing::info!("proof service initialized");

    let db_path: std::path::PathBuf = std::env::var("PROOF_DB_PATH")
        .unwrap_or_else(|_| "proof_requests.db".to_string())
        .into();
    tracing::info!(db_path = %db_path.display(), "initializing proof store database");
    let proof_store = Arc::new(ProofStore::new(db_path)?);
    tracing::info!("proof store initialized");

    let app_state = AppState {
        service,
        proof_service,
        proof_store,
    };
    spawn_status_worker(prover, vk, app_state.clone())?;
    let app = Router::new()
        .route("/healthz", get(health_handler))
        .route("/artifact/{is_tee}", post(create_artifact))
        .route("/proof", post(request_proof))
        .route("/proof/{request_id}", get(get_proof))
        .route("/utxo-proof", post(get_utxo_proof))
        .route("/nullifier-exclusion", post(get_nullifier_exclusion))
        .route("/hashes", post(get_hashes))
        .route("/queue", get(get_queue_health))
        .route("/stats", get(get_stats))
        .route("/api/snapshots/active-shard/{pool}", get(get_active_shard))
        .route("/api/snapshots/metadata", get(get_snapshot_metadata))
        .route("/api/snapshots/metadata/{pool}", get(get_snapshot_metadata_pool))
        .route("/api/snapshots/update-status", get(snapshot_update_status))
        .route("/api/snapshots/trigger-update", post(trigger_snapshot_update))
        .layer(CompressionLayer::new())
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::http::Request<_>| {
                    let uri_path = request.uri().path();
                    let is_noisy = uri_path.starts_with("/api/snapshots") 
                        || uri_path == "/hashes" 
                        || uri_path == "/nullifier-exclusion"
                        || uri_path.starts_with("/artifact/");
                    
                    if is_noisy {
                        // Create a TRACE level span for noisy endpoints (won't be logged)
                        tracing::span!(tracing::Level::TRACE, "http_request")
                    } else {
                        tracing::info_span!(
                            "http_request",
                            method = %request.method(),
                            uri = %request.uri(),
                            version = ?request.version(),
                        )
                    }
                })
                .on_request(|request: &axum::http::Request<_>, _span: &tracing::Span| {
                    // Skip logging for noisy endpoints
                    let uri = request.uri().path();
                    let is_noisy = uri.starts_with("/api/snapshots") 
                        || uri == "/hashes" 
                        || uri == "/nullifier-exclusion"
                        || uri.starts_with("/artifact/");
                    
                    if !is_noisy {
                        tracing::info!(method = %request.method(), uri = %request.uri(), "incoming request");
                    }
                })
                .on_response(|response: &axum::http::Response<_>, latency: std::time::Duration, span: &tracing::Span| {
                    // Check if this is a noisy endpoint by checking span level
                    // Noisy endpoints use TRACE level spans
                    if span.metadata().map(|m| m.level()) == Some(&tracing::Level::TRACE) {
                        return; // Skip logging for noisy endpoints
                    }
                    tracing::info!(status = %response.status(), latency_ms = latency.as_millis(), "request completed");
                })
                .on_failure(|error: tower_http::classify::ServerErrorsFailureClass, _latency: std::time::Duration, _span: &tracing::Span| {
                    tracing::warn!(error = ?error, "request failed");
                }),
        )
        .layer({
            // Configure CORS from environment variable or use permissive mode
            match std::env::var("CORS_ALLOWED_ORIGINS") {
                Ok(origins_str) if !origins_str.is_empty() => {
                    let origins: Vec<HeaderValue> = origins_str
                        .split(',')
                        .filter_map(|s| s.trim().parse().ok())
                        .collect();
                    
                    if origins.is_empty() {
                        tracing::warn!("CORS_ALLOWED_ORIGINS is set but no valid origins found, using permissive CORS");
                        CorsLayer::permissive()
                    } else {
                        tracing::info!("Using CORS origins from environment: {} origins configured", origins.len());
                        CorsLayer::new()
                            .allow_origin(origins)
                            .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                            .allow_headers([
                                axum::http::header::CONTENT_TYPE,
                                axum::http::header::AUTHORIZATION,
                                axum::http::header::ACCEPT,
                            ])
                            .allow_credentials(false)
                    }
                }
                _ => {
                    tracing::info!("CORS_ALLOWED_ORIGINS not set, using permissive CORS (allows all origins)");
                    CorsLayer::permissive()
                }
            }
        })
        .with_state(app_state);

    // Start snapshot updater if enabled
    let updater_config = snapshot_updater::SnapshotUpdaterConfig::from_env();
    if updater_config.enabled {
        tracing::info!("Starting automatic snapshot updater");
        let updater = snapshot_updater::SnapshotUpdater::new(updater_config);
        updater.spawn();
    } else {
        tracing::info!(
            "Automatic snapshot updates disabled (set SNAPSHOT_AUTO_UPDATE=true to enable)"
        );
    }

    let listener = tokio::net::TcpListener::bind(config.bind_address).await?;
    tracing::info!(address = %config.bind_address, "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

fn init_tracing() -> WorkerGuard {
    // Get log directory from environment or use default
    let log_dir = std::env::var("LOG_DIR").unwrap_or_else(|_| "./logs".to_string());

    // Check if console logging is enabled (default: true)
    let enable_console = std::env::var("LOG_CONSOLE")
        .unwrap_or_else(|_| "true".to_string())
        .parse::<bool>()
        .unwrap_or(true);

    // Create log directory if it doesn't exist
    std::fs::create_dir_all(&log_dir).expect("failed to create log directory");

    // Log rotation
    let file_appender = tracing_appender::rolling::hourly(log_dir, "server.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // Build filter from environment variable (e.g., RUST_LOG=info,server=debug)
    let filter = tracing_subscriber::EnvFilter::from_default_env();

    // Build subscriber with file output
    let file_subscriber = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false); // Disable ANSI colors in file logs

    // Build subscriber with console output (optional)
    if enable_console {
        tracing_subscriber::registry()
            .with(filter)
            .with(file_subscriber)
            .with(tracing_subscriber::fmt::layer().with_writer(std::io::stdout))
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(file_subscriber)
            .init();
    }

    guard
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm = signal(SignalKind::terminate()).expect("create signal handler");
        sigterm.recv().await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received");
}

#[derive(Clone)]
struct AppState {
    service: Arc<ProverService>,
    proof_service: Arc<ProofService>,
    proof_store: Arc<ProofStore>,
}

async fn health_handler() -> &'static str {
    "OK"
}

/// Get snapshot update status
async fn snapshot_update_status() -> Result<Json<serde_json::Value>, AppError> {
    let config = snapshot_updater::SnapshotUpdaterConfig::from_env();
    let status = snapshot_updater::get_update_status(&config)
        .await
        .map_err(AppError::internal)?;
    Ok(Json(status))
}

/// Trigger manual snapshot update
#[derive(Deserialize)]
struct TriggerUpdateRequest {
    height: Option<u32>,
}

async fn trigger_snapshot_update(
    Json(req): Json<TriggerUpdateRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let result = snapshot_updater::trigger_manual_update(req.height)
        .await
        .map_err(AppError::internal)?;
    Ok(Json(serde_json::json!({
        "status": "started",
        "message": result
    })))
}

async fn create_artifact(
    State(state): State<AppState>,
    Path(is_tee): Path<bool>,
) -> Result<Json<CreateArtifactResponse>, AppError> {
    let result = state
        .service
        .create_artifact(ArtifactType::Stdin, is_tee)
        .await
        .map_err(|e| {
            let error_msg = format!("Failed to create artifact (is_tee={}): {}", is_tee, e);
            tracing::error!(?e, is_tee = is_tee, "create_artifact failed");
            AppError::internal(eyre!(error_msg))
        })?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct GetHashesRequest {
    hashes: Vec<String>, // hex-encoded
}

async fn get_hashes(
    State(state): State<AppState>,
    Json(req): Json<GetHashesRequest>,
) -> Result<Json<bool>, AppError> {
    let hashes = req
        .hashes
        .iter()
        .map(|h| {
            hex::decode(h).map_err(|e| AppError::bad_request(format!("Invalid hash hex: {}", e)))
        })
        .collect::<Result<Vec<Vec<u8>>, AppError>>()?;
    let used = state
        .proof_store
        .get_used_hashes(hashes)
        .map_err(AppError::internal)?;
    Ok(Json(used))
}

async fn get_queue_health(State(state): State<AppState>) -> Result<Json<bool>, AppError> {
    let queue_status = state
        .service
        .is_queue_full()
        .await
        .map_err(AppError::internal)?;
    tracing::info!(response = ?queue_status, "GET /queue response");
    Ok(Json(queue_status))
}

#[derive(Debug, Serialize)]
struct CumulativeProofDataPoint {
    date: String,
    full_date: String,
    total_proofs: i64,
    daily_increase: i64,
}

#[derive(Debug, Serialize)]
struct CumulativeAmountsDataPoint {
    date: String,
    full_date: String,
    amount: f64,
}

#[derive(Debug, Serialize)]
struct RecentTransaction {
    id: String,
    address: String,
    amount: Option<f64>,
    timestamp: i64,
}

#[derive(Debug, Serialize)]
struct StatsResponse {
    cumulative_proofs: Vec<CumulativeProofDataPoint>,
    cumulative_amounts: Vec<CumulativeAmountsDataPoint>,
    recent_transactions: Vec<RecentTransaction>,
}

fn load_snapshot_metadata_value() -> Result<serde_json::Value, StatusCode> {
    let metadata_path = "snapshot_metadata.json";
    let metadata_str =
        std::fs::read_to_string(&metadata_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    serde_json::from_str(&metadata_str).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn cache_control_one_hour() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    );
    headers
}

async fn get_stats(State(state): State<AppState>) -> Result<Json<StatsResponse>, AppError> {
    const HOURS: i64 = 24; // Past 24 hours
    const LIMIT: i64 = 10; // Return 10 most recent transactions

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let cutoff = now - (HOURS * 3600);

    // Get all data in parallel using tokio::join! for better performance
    // Note: Since ProofStore uses Mutex internally, we can safely share the Arc across threads
    let (hourly_counts, hourly_amounts, total_before, amounts_before, recent_proofs) = tokio::join!(
        tokio::task::spawn_blocking({
            let store = state.proof_store.clone();
            move || store.get_hourly_proof_counts(HOURS)
        }),
        tokio::task::spawn_blocking({
            let store = state.proof_store.clone();
            move || store.get_hourly_amounts(HOURS)
        }),
        tokio::task::spawn_blocking({
            let store = state.proof_store.clone();
            move || store.get_total_before_timestamp(cutoff)
        }),
        tokio::task::spawn_blocking({
            let store = state.proof_store.clone();
            move || store.get_total_amount_before_timestamp(cutoff)
        }),
        tokio::task::spawn_blocking({
            let store = state.proof_store.clone();
            move || store.get_recent_proofs_with_addresses(LIMIT)
        }),
    );

    let hourly_counts = hourly_counts
        .map_err(|e| AppError::internal(eyre!("Task join error: {}", e)))?
        .map_err(AppError::internal)?;
    let hourly_amounts = hourly_amounts
        .map_err(|e| AppError::internal(eyre!("Task join error: {}", e)))?
        .map_err(AppError::internal)?;
    let total_before = total_before
        .map_err(|e| AppError::internal(eyre!("Task join error: {}", e)))?
        .map_err(AppError::internal)?;
    let amounts_before = amounts_before
        .map_err(|e| AppError::internal(eyre!("Task join error: {}", e)))?
        .map_err(AppError::internal)?;
    let recent_proofs = recent_proofs
        .map_err(|e| AppError::internal(eyre!("Task join error: {}", e)))?
        .map_err(AppError::internal)?;

    // Process cumulative proofs
    let mut cumulative_total = total_before;
    let mut cumulative_proofs = Vec::new();
    let mut current_hour = cutoff - (cutoff % 3600);
    let end_hour = now - (now % 3600);

    let mut hourly_map = std::collections::HashMap::new();
    for count in hourly_counts {
        hourly_map.insert(count.hour_timestamp, count.count);
    }

    while current_hour <= end_hour {
        let count = hourly_map.get(&current_hour).copied().unwrap_or(0);
        cumulative_total += count;

        let date =
            chrono::DateTime::from_timestamp(current_hour, 0).unwrap_or_else(chrono::Utc::now);

        cumulative_proofs.push(CumulativeProofDataPoint {
            date: date.format("%b %d %H:00").to_string(),
            full_date: date.format("%B %d, %Y at %H:00").to_string(),
            total_proofs: cumulative_total,
            daily_increase: count,
        });

        current_hour += 3600;
    }

    // Process cumulative amounts
    let mut hourly_amounts_map = std::collections::HashMap::new();
    for (hour, amount) in hourly_amounts {
        hourly_amounts_map.insert(hour, amount);
    }

    let mut cumulative_amounts_total = amounts_before as f64;
    let mut cumulative_amounts = Vec::new();
    let mut current_hour = cutoff - (cutoff % 3600);

    while current_hour <= end_hour {
        let hour_amount = hourly_amounts_map.get(&current_hour).copied().unwrap_or(0) as f64;
        cumulative_amounts_total += hour_amount;

        let date =
            chrono::DateTime::from_timestamp(current_hour, 0).unwrap_or_else(chrono::Utc::now);

        // Convert from zatoshis to ZEC (divide by 100_000_000)
        let amount_zec = cumulative_amounts_total / 100_000_000.0;

        cumulative_amounts.push(CumulativeAmountsDataPoint {
            date: date.format("%b %d %H:00").to_string(),
            full_date: date.format("%B %d, %Y at %H:00").to_string(),
            // Round to 3 decimal places to preserve small amounts (e.g., 0.001 ZEC)
            amount: (amount_zec * 1000.0).round() / 1000.0,
        });

        current_hour += 3600;
    }

    // Process recent transactions
    let mut recent_transactions = Vec::new();
    for (proof, address_bytes) in recent_proofs {
        let address = if let Some(bytes) = address_bytes {
            if bytes.len() == 32 {
                bs58::encode(&bytes).into_string()
            } else {
                format!("0x{}", hex::encode(&bytes))
            }
        } else {
            format!("req_{}", &proof.request_id[..8.min(proof.request_id.len())])
        };

        let amount = proof.amount.map(|a| a as f64 / 100_000_000.0);

        recent_transactions.push(RecentTransaction {
            id: proof.request_id,
            address,
            amount,
            timestamp: proof.created_at,
        });
    }

    let response = StatsResponse {
        cumulative_proofs,
        cumulative_amounts,
        recent_transactions,
    };
    Ok(Json(response))
}

async fn get_snapshot_metadata() -> Result<impl IntoResponse, StatusCode> {
    let metadata = load_snapshot_metadata_value()?;
    Ok((cache_control_one_hour(), Json(metadata)))
}

async fn get_snapshot_metadata_pool(
    Path(pool): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let metadata = load_snapshot_metadata_value()?;
    let (count_key, pool_name) = match pool.as_str() {
        "orchard" => ("orchard_count", "orchard"),
        "sapling" => ("sapling_count", "sapling"),
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let count = metadata[count_key]
        .as_u64()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let shard_id = count / 65_536;

    tracing::info!(
        pool = pool_name,
        shard_id,
        snapshot_height = metadata
            .get("snapshot_height")
            .and_then(|v| v.as_u64())
            .unwrap_or_default(),
        "serving pool-scoped metadata"
    );

    let shard_paths = metadata
        .get("shard_paths")
        .and_then(|paths| paths.get(pool_name))
        .and_then(|pool_paths| pool_paths.get(&shard_id.to_string()))
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));

    // Build shard_paths object in the same shape as full metadata:
    // { "shard_paths": { "<pool>": { "<shard_id>": [...] } } }
    let mut shard_paths_by_id = serde_json::Map::new();
    shard_paths_by_id.insert(shard_id.to_string(), shard_paths);

    let mut shard_paths_by_pool = serde_json::Map::new();
    shard_paths_by_pool.insert(pool_name.to_string(), serde_json::Value::Object(shard_paths_by_id));

    let mut body = serde_json::Map::new();
    body.insert(
        "snapshot_height".to_string(),
        metadata
            .get("snapshot_height")
            .cloned()
            .unwrap_or_else(|| serde_json::Value::Null),
    );
    body.insert(format!("{}_count", pool_name), serde_json::json!(count));
    body.insert("active_shard_id".to_string(), serde_json::json!(shard_id));
    body.insert(
        "shard_paths".to_string(),
        serde_json::Value::Object(shard_paths_by_pool),
    );

    Ok((
        cache_control_one_hour(),
        Json(serde_json::Value::Object(body)),
    ))
}

async fn get_active_shard(Path(pool): Path<String>) -> Result<impl IntoResponse, StatusCode> {
    let metadata = load_snapshot_metadata_value()?;

    // Get snapshot height from metadata
    let snapshot_height = metadata["snapshot_height"]
        .as_u64()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Derive snapshot directory from height
    let snapshot_dir = format!("snapshots/snapshot-{}", snapshot_height);

    // Calculate active shard ID based on count
    let (count_key, pool_name) = match pool.as_str() {
        "orchard" => ("orchard_count", "orchard"),
        "sapling" => ("sapling_count", "sapling"),
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let count = metadata[count_key]
        .as_u64()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let shard_id = count / 65536;

    tracing::info!(
        pool = pool_name,
        shard_id,
        snapshot_height,
        "serving active shard"
    );

    // Read the active shard file
    let shard_path = format!("{}/shards/{}/{}.bin", snapshot_dir, pool_name, shard_id);

    let shard_data = std::fs::read(&shard_path).map_err(|_| StatusCode::NOT_FOUND)?;

    // Return shard data with proper headers
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/octet-stream")
        .header("Access-Control-Allow-Origin", "*")
        .header("Access-Control-Allow-Headers", "Content-Type")
        .header("Cache-Control", "public, max-age=300")
        .body(Body::from(shard_data))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?)
}

async fn request_proof(
    State(state): State<AppState>,
    Json(mut body): Json<ProofRequest>,
) -> Result<Json<ProofResponse>, AppError> {
    tracing::info!(
        address = %body.address,
        is_tee = body.is_tee,
        amount = body.amount,
        timestamp = body.timestamp,
        has_stdin = body.stdin.is_some(),
        has_stdin_uri = body.stdin_uri.is_some(),
        "POST /proof request received"
    );
    // Decode signature once
    use base64::Engine;
    tracing::debug!("decoding base64 signature");
    let signature_bytes = base64::engine::general_purpose::STANDARD
        .decode(&body.signature)
        .map_err(|e| {
            tracing::warn!(error = %e, "failed to decode base64 signature");
            AppError::bad_request(format!("Invalid base64 signature: {}", e))
        })?;

    // Verify signature (before normalizing address in body)
    tracing::debug!("verifying Solana signature");
    verify_solana_signature(
        &signature_bytes,
        &body.message,
        &body.address,
        body.timestamp,
    )
    .map_err(|e| {
        tracing::error!(
            "Signature verification failed: address={}, timestamp={}, error={:?}",
            body.address,
            body.timestamp,
            e
        );
        AppError::bad_request(format!("Invalid signature: {}", e))
    })?;
    tracing::debug!("signature verified successfully");

    tracing::debug!("parsing and normalizing Solana address");
    let (normalized_address, address_bytes) = parse_solana_address(&body.address)?;
    tracing::debug!(normalized_address = %normalized_address, "address normalized");
    body.address = normalized_address;

    // Extract timestamp before body is moved
    let signature_timestamp = body.timestamp;

    // Check for replay attacks - ensure signature is unique
    tracing::debug!("checking for duplicate signature (replay attack prevention)");
    let signature_exists = state
        .proof_store
        .signature_exists(&signature_bytes)
        .map_err(|e| {
            tracing::error!(error = ?e, "failed to check signature existence");
            AppError::internal(eyre!("Database error: {}", e))
        })?;
    if signature_exists {
        tracing::warn!(
            address = %body.address,
            timestamp = signature_timestamp,
            "duplicate signature detected - replay attack prevented"
        );
        return Err(AppError::bad_request("Duplicate request (replay detected)"));
    }

    // Check queue status (only for TEE mode)
    let queue_full = if body.is_tee {
        tracing::debug!("checking TEE queue status");
        let status = state
            .service
            .is_queue_full()
            .await
            .map_err(AppError::internal)?;
        tracing::debug!(queue_full = status, "TEE queue status checked");
        status
    } else {
        false // Base mode doesn't have queue limits
    };
    if queue_full {
        tracing::warn!("TEE queue is full, rejecting request");
        return Err(AppError::bad_request(
            "TEE Prover capacity is at maximum. Please try again later.",
        ));
    }

    let is_tee = body.is_tee;

    // Check if TEE mode is requested but not enabled
    if is_tee {
        tracing::debug!("checking TEE mode availability");
        if state.service.signer(true).is_err() {
            tracing::warn!("TEE mode requested but not enabled");
            return Err(AppError::bad_request(
                "TEE mode is not enabled. Set TEE_PRIVATE_KEY environment variable to enable TEE mode.",
            ));
        }
        tracing::debug!("TEE mode is available");
    }

    let amount = body.amount;
    tracing::info!(
        is_tee = is_tee,
        amount = amount,
        has_stdin = body.stdin.is_some(),
        has_stdin_uri = body.stdin_uri.is_some(),
        "submitting proof request to monerochan network"
    );
    let result = state.service.request_proof(body).await.map_err(|e| {
        tracing::error!(error = %e, "failed to request proof from monerochan network");
        AppError::internal(e)
    })?;
    tracing::info!(
        request_id = %result.request_id,
        "proof request submitted successfully, recording in database"
    );
    state
        .proof_store
        .record_request(
            &result.request_id,
            "Assigned",
            address_bytes.as_slice(),
            is_tee,
            amount,
            &signature_bytes,
            signature_timestamp,
        )
        .map_err(|e| {
            tracing::error!(error = ?e, request_id = %result.request_id, "failed to record proof request in database");
            AppError::internal(eyre!("Database error: {}", e))
        })?;
    tracing::debug!(request_id = %result.request_id, "proof request recorded in database");
    tracing::info!(request_id = %result.request_id, response = ?result, "POST /proof response");
    Ok(Json(result))
}

async fn get_proof(
    State(state): State<AppState>,
    Path(request_id): Path<String>,
) -> Result<Json<GetProofResponse>, AppError> {
    tracing::info!(request_id = %request_id, "GET /proof/{request_id} request received");
    tracing::debug!(request_id = %request_id, "querying database for proof request");
    let result = state.proof_store.get_request(&request_id).map_err(|e| {
        tracing::error!(error = ?e, request_id = %request_id, "database query failed");
        AppError::internal(eyre!("Database error: {}", e))
    })?;
    let Some(mut result) = result else {
        tracing::warn!(request_id = %request_id, "proof request not found in database");
        return Err(AppError::not_found(format!(
            "Proof request not found: {}",
            request_id
        )));
    };
    
    // Query prover-gateway for real-time status
    // Get the appropriate network client based on is_tee
    let network_client = state.service.network_client(result.is_tee)
        .map_err(|e| AppError::internal(eyre!("Failed to get network client: {}", e)))?;
    
    // Add 0x prefix if not present
    let request_id_with_prefix = if request_id.starts_with("0x") {
        request_id.clone()
    } else {
        format!("0x{}", request_id)
    };
    
    match network_client.clone().get_proof_status(ApiGetProofStatusRequest {
        request_id: request_id_with_prefix,
    }).await {
        Ok(response) => {
            let response = response.into_inner();
            tracing::debug!(request_id = %request_id, status = ?response.status, "fetched live status from prover-gateway");
            
            // Map JobStatus to string
            use monerochan::network::proto::api::JobStatus;
            let status_str = match JobStatus::try_from(response.status) {
                Ok(JobStatus::Succeeded) => "Fulfilled",
                Ok(JobStatus::Failed) => "Failed",
                Ok(JobStatus::Running) => "Processing",
                Ok(JobStatus::Pending) => "Pending",
                Ok(JobStatus::Unspecified) | _ => "Unknown",
            };
            
            result.status = status_str.to_string();
            
            // TODO: Handle proof_url and fulfilled_at from response if available
        }
        Err(e) => {
            tracing::warn!(error = %e, request_id = %request_id, "failed to fetch live status from prover-gateway, using cached status");
            // Fall back to cached database status
        }
    }
    
    tracing::info!(request_id = %request_id, response = ?result, "GET /proof/{request_id} response");
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct UtxoProofRequest {
    txid: String, // hex-encoded
    vout: u32,
    value: u64,
    script_pubkey: String, // hex-encoded
}

async fn get_utxo_proof(
    State(state): State<AppState>,
    Json(req): Json<UtxoProofRequest>,
) -> Result<Json<zfun_lib::InclusionProof>, AppError> {
    tracing::info!(
        txid = %req.txid,
        vout = req.vout,
        value = req.value,
        "POST /utxo-proof request received"
    );
    tracing::debug!("decoding txid hex");
    let txid_bytes = hex::decode(&req.txid).map_err(|e| {
        tracing::warn!(error = %e, txid = %req.txid, "invalid txid hex");
        AppError::bad_request(format!("Invalid txid hex: {}", e))
    })?;
    let mut txid = [0u8; 32];
    if txid_bytes.len() != 32 {
        tracing::warn!(txid_len = txid_bytes.len(), "txid length mismatch");
        return Err(AppError::bad_request("txid must be 32 bytes"));
    }
    txid.copy_from_slice(&txid_bytes);

    tracing::debug!("decoding script_pubkey hex");
    let script_pubkey = hex::decode(&req.script_pubkey).map_err(|e| {
        tracing::warn!(error = %e, "invalid script_pubkey hex");
        AppError::bad_request(format!("Invalid script_pubkey hex: {}", e))
    })?;

    tracing::debug!("generating UTXO inclusion proof");
    let proof =
        state
            .proof_service
            .generate_utxo_proof(&txid, req.vout, req.value, &script_pubkey)
            .map_err(|e| {
                tracing::error!(error = %e, txid = %req.txid, vout = req.vout, "failed to generate UTXO proof");
                AppError::internal(e)
            })?;

    tracing::info!(txid = %req.txid, vout = req.vout, response = ?proof, "POST /utxo-proof response");
    Ok(Json(proof))
}

#[derive(Debug, Deserialize)]
struct NullifierExclusionRequest {
    nullifier: String, // hex-encoded
}

async fn get_nullifier_exclusion(
    State(state): State<AppState>,
    Json(req): Json<NullifierExclusionRequest>,
) -> Result<Json<zfun_lib::ExclusionProof>, AppError> {
    let nullifier_bytes = hex::decode(&req.nullifier).map_err(|e| {
        tracing::warn!(error = %e, nullifier = %req.nullifier, "invalid nullifier hex");
        AppError::bad_request(format!("Invalid nullifier hex: {}", e))
    })?;
    let mut nullifier = [0u8; 32];
    if nullifier_bytes.len() != 32 {
        tracing::warn!(
            nullifier_len = nullifier_bytes.len(),
            "nullifier length mismatch"
        );
        return Err(AppError::bad_request("nullifier must be 32 bytes"));
    }
    nullifier.copy_from_slice(&nullifier_bytes);

    let proof = state
        .proof_service
        .generate_nullifier_exclusion(&nullifier)
        .map_err(|e| {
            tracing::error!(error = %e, nullifier = %req.nullifier, "failed to generate nullifier exclusion proof");
            AppError::internal(e)
        })?;

    Ok(Json(proof))
}

struct ProverService {
    config: ProverConfig,
    tee_signer: Option<NetworkSigner>,
    tee_artifact_client: Option<ArtifactStoreClient<Channel>>,
    tee_network_client: Option<MonerochanNetworkClient<Channel>>,

    tee_network_client_monerochan: Option<monerochan::network::NetworkClient>,
    base_signer: NetworkSigner,
    base_artifact_client: ArtifactStoreClient<Channel>,
    base_network_client: MonerochanNetworkClient<Channel>,

    base_network_client_monerochan: Option<monerochan::network::NetworkClient>,
    vk: Arc<MONEROCHANVerifyingKey>,
    http_client: reqwest::Client,
}

impl ProverService {
    async fn new(vk: Arc<MONEROCHANVerifyingKey>, config: &ProverConfig) -> Result<Self> {
        let (tee_signer, tee_artifact_client, tee_network_client) =
            if let Some(ref tee_key) = config.tee_private_key {
                let mut tee_endpoint = Endpoint::from_shared(config.tee_rpc_url.clone())?;
                if config.tee_rpc_url.starts_with("https://") {
                    tee_endpoint =
                        tee_endpoint.tls_config(ClientTlsConfig::new().with_enabled_roots())?;
                }
                // HTTP URLs will use plaintext HTTP/2 automatically
                let tee_channel = tee_endpoint.connect().await?;
                let tee_artifact = ArtifactStoreClient::new(tee_channel.clone());
                let tee_network = MonerochanNetworkClient::new(tee_channel.clone());
                // Try Solana first, then fall back to Ethereum
                let signer = NetworkSigner::solana(tee_key)
                    .or_else(|_| NetworkSigner::local(tee_key))?;
                (Some(signer), Some(tee_artifact), Some(tee_network))
            } else {
                (None, None, None)
            };


        let tee_network_client_monerochan = if let Some(ref tee_key) = config.tee_private_key {
            use monerochan::network::{NetworkClient, NetworkMode};
            // Try Solana first, then fall back to Ethereum
            let signer = NetworkSigner::solana(tee_key)
                .or_else(|_| NetworkSigner::local(tee_key))?;
            Some(NetworkClient::new(
                signer,
                config.tee_rpc_url.clone(),
                NetworkMode::Reserved,
            ))
        } else {
            None
        };

        let mut base_endpoint = Endpoint::from_shared(config.base_rpc_url.clone())?;
        if config.base_rpc_url.starts_with("https://") {
            base_endpoint =
                base_endpoint.tls_config(ClientTlsConfig::new().with_enabled_roots())?;
        }
        // HTTP URLs will use plaintext HTTP/2 automatically
        // Retry connection with exponential backoff (up to 5 attempts)
        let base_channel = {
            let mut retries = 0;
            let max_retries = 5;
            loop {
                match base_endpoint.connect().await {
                    Ok(channel) => break channel,
                    Err(e) => {
                        retries += 1;
                        if retries >= max_retries {
                            return Err(eyre!("Failed to connect to base RPC after {} retries: {}", max_retries, e));
                        }
                        let delay = Duration::from_millis(100 * (1 << retries)); // Exponential backoff: 200ms, 400ms, 800ms, 1600ms
                        eprintln!("Failed to connect to base RPC (attempt {}/{}): {}. Retrying in {:?}...", retries, max_retries, e, delay);
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        };
        let base_artifact_client = ArtifactStoreClient::new(base_channel.clone());
        let base_network_client = MonerochanNetworkClient::new(base_channel);

        // Try to parse as Solana key first (base58, 32 bytes), then fall back to Ethereum
        let base_signer = NetworkSigner::solana(&config.base_private_key)
            .map(|signer| {
                if let Some(addr) = signer.solana_address() {
                    info!("Using Solana key with address: {}", addr);
                }
                signer
            })
            .or_else(|_| {
                info!("Failed to parse as Solana key, trying Ethereum format");
                NetworkSigner::local(&config.base_private_key)
            })?;

        
        let base_network_client_monerochan = {
            use monerochan::network::{NetworkClient, NetworkMode};
            Some(NetworkClient::new(
                base_signer.clone(),
                config.base_rpc_url.clone(),
                NetworkMode::Reserved,
            ))
        };

        // Create reusable HTTP client
        let http_client = reqwest::Client::builder()
            .build()
            .map_err(|e| eyre!("Failed to create HTTP client: {}", e))?;

        Ok(Self {
            config: config.clone(),
            tee_signer,
            tee_artifact_client,
            tee_network_client,
            
            tee_network_client_monerochan,
            base_signer,
            base_artifact_client,
            base_network_client,
            
            base_network_client_monerochan,
            vk,
            http_client,
        })
    }

    fn network_client(&self, is_tee: bool) -> Result<&MonerochanNetworkClient<Channel>> {
        if is_tee {
            self.tee_network_client.as_ref().ok_or_else(|| {
                eyre!("TEE mode is not enabled. Set TEE_PRIVATE_KEY to enable TEE mode.")
            })
        } else {
            Ok(&self.base_network_client)
        }
    }

    fn artifact_client(&self, is_tee: bool) -> Result<&ArtifactStoreClient<Channel>> {
        if is_tee {
            self.tee_artifact_client.as_ref().ok_or_else(|| {
                eyre!("TEE mode is not enabled. Set TEE_PRIVATE_KEY to enable TEE mode.")
            })
        } else {
            Ok(&self.base_artifact_client)
        }
    }

    fn signer(&self, is_tee: bool) -> Result<&NetworkSigner> {
        if is_tee {
            self.tee_signer.as_ref().ok_or_else(|| {
                eyre!("TEE mode is not enabled. Set TEE_PRIVATE_KEY to enable TEE mode.")
            })
        } else {
            Ok(&self.base_signer)
        }
    }

    /// Fetch stdin bytes from an HTTPS URL with basic validation
    async fn fetch_stdin_via_https(&self, url: &str) -> Result<Vec<u8>> {
        let response = self
            .http_client
            .get(url)
            .send()
            .await
            .map_err(|e| eyre::eyre!("failed to fetch stdin from URL {}: {}", url, e))?;

        if !response.status().is_success() {
            return Err(eyre::eyre!(
                "failed to fetch stdin from URL {}: HTTP {}",
                url,
                response.status()
            ));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| eyre::eyre!("failed to read stdin bytes from URL {}: {}", url, e))?
            .to_vec();

        if bytes.is_empty() {
            return Err(eyre::eyre!(
                "stdin payload is empty at URL {}; proof requests require non-empty input",
                url
            ));
        }

        Ok(bytes)
    }

    async fn get_nonce(&self, is_tee: bool) -> Result<u64> {
        let signer_address = format!("0x{}", hex::encode(&**self.signer(is_tee)?.address()));

        {
            if let Some(ref client) = if is_tee {
                self.tee_network_client_monerochan.as_ref()
            } else {
                self.base_network_client_monerochan.as_ref()
            } {
                let nonce = client.get_nonce().await.map_err(|e| eyre::eyre!("{}", e))?;
                tracing::info!(
                    signer_address = %signer_address,
                    nonce = nonce,
                    is_tee = is_tee,
                    source = "NetworkClient",
                    "retrieved nonce from network"
                );
                return Ok(nonce);
            }
        }

        // Fallback to low-level API when NetworkClient not available
        tracing::debug!(
            signer_address = %signer_address,
            is_tee = is_tee,
            "using fallback gRPC client to get nonce"
        );
        let nonce = self
            .network_client(is_tee)?
            .clone()
            .get_nonce(ApiGetNonceRequest {
                address: (**self.signer(is_tee)?.address()).into(),
            })
            .await?
            .into_inner()
            .nonce;
        tracing::info!(
            signer_address = %signer_address,
            nonce = nonce,
            is_tee = is_tee,
            source = "gRPC",
            "retrieved nonce from network"
        );
        Ok(nonce)
    }

    fn vk_hash(&self) -> Vec<u8> {
        // Use the SAME vk_hash computation as NetworkClient to ensure consistency
        use monerochan::network::NetworkClient;
        NetworkClient::get_vk_hash(&*self.vk)
            .expect("failed to compute vk_hash")
            .to_vec()
    }

    async fn setup_program(&self, is_tee: bool) -> Result<()> {
        // Try using Monerochan's NetworkClient API if available
        
        {
            if let Some(ref client) = if is_tee {
                self.tee_network_client_monerochan.as_ref()
            } else {
                self.base_network_client_monerochan.as_ref()
            } {
                use monerochan::network::NetworkClient;
                let vk_hash_b256 =
                    NetworkClient::get_vk_hash(&*self.vk).map_err(|e| eyre::eyre!("{}", e))?;
                let vk_hash_bytes: &[u8] = vk_hash_b256.as_ref();
                let vk_hash_hex = format!("0x{}", hex::encode(vk_hash_bytes));
                info!(vk_hash = %vk_hash_hex, "checking if program exists on monerochan network");
                match client.get_program(vk_hash_b256).await {
                    Ok(Some(program)) => {
                        info!(vk_hash = %vk_hash_hex, program_uri = ?program.program_uri(), "program already exists on monerochan network");
                        return Ok(());
                    }
                    Ok(None) => {
                        info!(vk_hash = %vk_hash_hex, "program NOT found on monerochan network, registering...");
                        // Program doesn't exist, register it using Monerochan's high-level API
                        info!(vk_hash = %vk_hash_hex, "Registering program using Monerochan NetworkClient...");
                        match client.register_program(&*self.vk, ZFUN_PROGRAM_ELF).await {
                            Ok(_) => {
                                info!(vk_hash = %vk_hash_hex, "program registered successfully on monerochan network");
                                return Ok(());
                            }
                            Err(e) => {
                                // Check if it's Unimplemented - that's OK, program management might not be available
                                if let Some(tonic_err) = e.downcast_ref::<tonic::Status>() {
                                    if tonic_err.code() == Code::Unimplemented {
                                        warn!(
                                            "Program management not available (Unimplemented), skipping program registration"
                                        );
                                        return Ok(());
                                    }
                                }
                                // Also check for anyhow errors that might wrap tonic errors
                                let err_str = format!("{}", e);
                                if err_str.contains("Unimplemented")
                                    || err_str.contains("unimplemented")
                                {
                                    warn!(
                                        "Program management not available (Unimplemented), skipping program registration"
                                    );
                                    return Ok(());
                                }
                                warn!(
                                    "Failed to register program with Monerochan NetworkClient: {}, falling back to low-level API",
                                    e
                                );
                                // Fall through to low-level API
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Failed to get program with Monerochan NetworkClient: {}, falling back to low-level API",
                            e
                        );
                        // Fall through to low-level API
                    }
                }
            }
        }

        // Fallback to low-level API when NetworkClient not available
        let program = self
            .network_client(is_tee)?
            .clone()
            .get_program(ApiGetProgramRequest {
                vk_hash: self.vk_hash(),
            })
            .await;
        if let Err(e) = program {
            // Try to create artifact, but handle Unimplemented gracefully (some backends may not support artifact store)
            let artifact_result = self.create_artifact(ArtifactType::Program, is_tee).await;
            let artifact = match artifact_result {
                Ok(art) => Some(art),
                Err(err) => {
                    if let Some(tonic_err) = err.downcast_ref::<tonic::Status>() {
                        if tonic_err.code() == Code::Unimplemented {
                            warn!(
                                "Artifact store not available (Unimplemented), skipping artifact creation"
                            );
                            None
                        } else {
                            return Err(err);
                        }
                    } else {
                        return Err(err);
                    }
                }
            };

            if let Some(ref artifact) = artifact {
                let elf_bytes = bincode::serialize(&(ZFUN_PROGRAM_ELF.to_vec()))?;
                match self
                    .http_client
                    .put(artifact.artifact_presigned_url.clone())
                    .body(elf_bytes)
                    .send()
                    .await
                {
                    Ok(res) => {
                        info!("uploaded program to {}", artifact.artifact_presigned_url);
                        debug!("response: {:?}", res.text().await?);
                    }
                    Err(e) => {
                        warn!("error uploading program: {:?}", e);
                    }
                }
                info!("uploaded program to {}", artifact.artifact_uri);
            }

            if e.code() == Code::NotFound {
                // Try to create program, but handle Unimplemented gracefully (some backends may not support program management)
                let nonce_result = self.get_nonce(is_tee).await;
                match nonce_result {
                    Ok(nonce) => {
                        let vk_bytes = bincode::serialize(&self.vk)?;
                        let program_uri = match artifact {
                            Some(ref a) => a.artifact_uri.clone(),
                            None => {
                                // If we need to register the program but couldn't upload the artifact, we can't proceed.
                                tracing::error!(
                                    "Cannot register program: Artifact upload failed or not supported"
                                );
                                return Err(eyre!(
                                    "Cannot register program without artifact URI. Artifact store may not be available."
                                ));
                            }
                        };
                        // Use Monerochan CreateProgramRequest format (vk_hash + ELF bytes)
                        let create_program_result = self
                            .network_client(is_tee)?
                            .clone()
                            .create_program(ApiCreateProgramRequest {
                                vk_hash: self.vk_hash(),
                                elf: ZFUN_PROGRAM_ELF.to_vec(),
                            })
                            .await;

                        match create_program_result {
                            Ok(_) => {
                                info!("program created successfully");
                            }
                            Err(create_err) => {
                                if create_err.code() == Code::Unimplemented {
                                    warn!(
                                        "Program management not available (Unimplemented), skipping program creation. The program may need to be registered separately."
                                    );
                                } else {
                                    return Err(create_err.into());
                                }
                            }
                        }
                    }
                    Err(nonce_err) => {
                        if let Some(tonic_err) = nonce_err.downcast_ref::<tonic::Status>() {
                            if tonic_err.code() == Code::Unimplemented {
                                warn!(
                                    "Program management not available (Unimplemented), skipping program registration"
                                );
                            } else {
                                return Err(nonce_err);
                            }
                        } else {
                            return Err(nonce_err);
                        }
                    }
                }
            } else {
                return Err(e.into());
            }
        } else {
            info!("program already exists");
        }
        Ok(())
    }

    async fn create_artifact(
        &self,
        artifact_type: ArtifactType,
        is_tee: bool,
    ) -> Result<CreateArtifactResponse> {
        // Check queue status (only for TEE mode)
        if is_tee {
            let queue_full = self.is_queue_full().await?;
            if queue_full {
                return Err(eyre!(
                    "TEE Prover capacity is at maximum. Please try again later."
                ));
            }
        }

        let response = self
            .artifact_client(is_tee)?
            .clone()
            .create_artifact(CreateArtifactRequest {
                artifact_type: artifact_type.into(),
                signature: self
                    .signer(is_tee)?
                    .sign_bytes(b"create_artifact")?,
            })
            .await?
            .into_inner();
        Ok(response)
    }

    async fn request_proof(&self, request: ProofRequest) -> Result<ProofResponse> {
        tracing::debug!(is_tee = request.is_tee, "request_proof: getting nonce");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let nonce = self.get_nonce(request.is_tee).await.map_err(|e| {
            tracing::error!(error = %e, is_tee = request.is_tee, "failed to get nonce");
            e
        })?;
        tracing::debug!(nonce = nonce, is_tee = request.is_tee, "nonce retrieved");

        // Use the SAME vk_hash computation method as setup_program to ensure consistency
        use monerochan::network::NetworkClient;
        let vk_hash_bytes = NetworkClient::get_vk_hash(&*self.vk)
            .map_err(|e| eyre::eyre!("failed to compute vk_hash: {}", e))?
            .to_vec();

        let vk_hash_hex = format!("0x{}", hex::encode(&vk_hash_bytes));
        let version_string = format!("monerochan-{}", MONEROCHAN_CIRCUIT_VERSION.trim());

        // Resolve stdin: prefer inline stdin, fall back to stdin_uri
        let stdin_bytes = if let Some(ref stdin_base64) = request.stdin {
            // Use inline stdin
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .decode(stdin_base64)
                .map_err(|e| eyre::eyre!("failed to decode inline stdin from base64: {}", e))?
        } else if let Some(ref stdin_uri) = request.stdin_uri {
            // Fetch stdin from URI (handles S3 URIs via monerochan network client)
            tracing::info!(stdin_uri = %stdin_uri, "fetching stdin from URI");
            
            if stdin_uri.starts_with("s3://") {
                // For S3 URIs, convert to HTTPS URL format
                // Note: This works for public S3 buckets. For private buckets, presigned URLs are needed
                // but the monerochan crate doesn't expose GetArtifact yet, so we use direct HTTPS conversion
                let https_url = {
                    let parts: Vec<&str> = stdin_uri.trim_start_matches("s3://").splitn(2, '/').collect();
                    if parts.len() == 2 {
                        format!("https://{}.s3.amazonaws.com/{}", parts[0], parts[1])
                    } else {
                        return Err(eyre::eyre!("invalid S3 URI format: {}", stdin_uri));
                    }
                };
                
                tracing::info!(stdin_uri = %stdin_uri, https_url = %https_url, "converted S3 URI to HTTPS URL");
                
                let response = self
                    .http_client
                    .get(&https_url)
                    .send()
                    .await
                    .map_err(|e| eyre::eyre!("failed to fetch stdin from converted HTTPS URL {}: {}", https_url, e))?;
                
                if !response.status().is_success() {
                    return Err(eyre::eyre!(
                        "failed to fetch stdin from converted HTTPS URL {}: HTTP {} (Note: S3 bucket may be private and require presigned URLs)",
                        https_url,
                        response.status()
                    ));
                }
                
                response
                    .bytes()
                    .await
                    .map_err(|e| eyre::eyre!("failed to read stdin bytes from converted HTTPS URL {}: {}", https_url, e))?
                    .to_vec()
            } else {
                // For HTTPS URLs, download directly
                self.fetch_stdin_via_https(stdin_uri).await?
            }
        } else {
            return Err(eyre::eyre!(
                "either 'stdin' (base64-encoded) or 'stdin_uri' must be provided"
            ));
        };

        if stdin_bytes.is_empty() {
            // Fail fast before hitting the gateway to avoid confusing EOF errors downstream
            return Err(eyre::eyre!(
                "stdin payload is empty; proof requests require non-empty input"
            ));
        }

        let stdin_sha256 = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&stdin_bytes);
            hex::encode(hasher.finalize())
        };

        info!(
            vk_hash = %vk_hash_hex,
            nonce = nonce,
            is_tee = request.is_tee,
            stdin_size = stdin_bytes.len(),
            stdin_sha256 = %stdin_sha256,
            version = %version_string,
            "submitting proof request to monerochan network"
        );
        tracing::debug!("building monerochan network request");
        let cycle_limit = std::env::var("PROOF_CYCLE_LIMIT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1_000_000_000);
        let gas_limit = std::env::var("PROOF_GAS_LIMIT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1_000_000_000);
        let timeout_secs = 60 * 15; // 15 minutes

        let response = self
            .network_client(request.is_tee)?
            .clone()
            .request_proof(RequestProofRequest {
                program_id: vk_hash_hex.clone(), // Pass vk_hash as program_id
                elf: ZFUN_PROGRAM_ELF.to_vec(), // Include ELF so gateway can run setup if not cached
                stdin: stdin_bytes, // Pass stdin inline
                proof_mode: ApiProofMode::Groth16.into(),
                strategy: ApiStrategy::Hosted.into(),
                timeout_secs: Some(timeout_secs),
                skip_simulation: false,
                cycle_limit: Some(cycle_limit),
                gas_limit: Some(gas_limit),
                client_address: Some({
                    let signer = self.signer(request.is_tee)?;
                    // If Solana signer, use the proper base58 public key; otherwise encode Ethereum address
                    signer.solana_address().unwrap_or_else(|| {
                        let addr = signer.address();
                        let addr_bytes: &[u8] = addr.as_ref();
                        bs58::encode(addr_bytes).into_string()
                    })
                }), // Solana address in base58 format
                client_auth: None, // Auth handled by gateway
                stdin_uri: None, // Stdin provided inline
            })
            .await
            .map_err(|e| {
                let err_msg = format!("{}", e);
                if err_msg.contains("foreign key constraint") || err_msg.contains("vk_hash_fkey") {
                    tracing::error!(
                        vk_hash = %vk_hash_hex,
                        error = %err_msg,
                        "program not registered on monerochan network"
                    );
                    eyre!(
                        "Program with VK hash {} is not registered on the monerochan network prover. \
                        The program must be registered before submitting proof requests. \
                        Please ensure setup_program completed successfully at server startup. \
                        Original error: {}",
                        vk_hash_hex,
                        err_msg
                    )
                } else {
                    tracing::error!(
                        error = %e,
                        vk_hash = %vk_hash_hex,
                        is_tee = request.is_tee,
                        "proof request failed"
                    );
                    e.into()
                }
            })?;
        tracing::debug!("received response from monerochan network");
        let response_inner = response.into_inner();
        // Monerochan response has request_id as a hex string (with 0x prefix)
        let request_id = response_inner.request_id.trim_start_matches("0x").to_string();
        tracing::info!(
            request_id = %request_id,
            job_id = %response_inner.job_id,
            "proof request submitted successfully"
        );
        Ok(ProofResponse { request_id })
    }

    async fn is_queue_full(&self) -> Result<bool> {
        // Only check TEE queue if TEE is enabled
        if self.tee_network_client.is_none() {
            return Ok(false); // TEE not enabled, so queue is not full
        }

        let status = reqwest::Client::builder()
            .build()
            .map_err(|e| eyre!("Failed to create HTTP client: {}", e))?
            .get(format!("{}/health", self.config.tee_rpc_url))
            .send()
            .await
            .map_err(|e| eyre!("Failed to check TEE queue status: {}", e))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| eyre!("Failed to parse TEE queue status response: {}", e))?;
        Ok(status["queued_proof_request_count"].as_u64().unwrap_or(0) >= MAX_QUEUE_SIZE)
    }

    async fn get_proof(
        &self,
        request_id: String,
        is_tee: bool,
    ) -> Result<GetProofRequestStatusResponse> {
        tracing::debug!(request_id = %request_id, is_tee = is_tee, "getting proof status from monerochan network");
        let request_id_bytes = hex::decode(&request_id)
            .map_err(|e| {
                tracing::error!(error = %e, request_id = %request_id, "failed to decode request_id hex");
                e
            })?;
        let response = self
            .network_client(is_tee)?
            .clone()
            .get_proof_status(ApiGetProofStatusRequest {
                request_id: format!("0x{}", request_id), // Monerochan uses hex string with 0x prefix
            })
            .await
            .map_err(|e| {
                tracing::error!(error = %e, request_id = %request_id, is_tee = is_tee, "failed to get proof status from monerochan network");
                e
            })?
            .into_inner();
        tracing::debug!(
            request_id = %request_id,
            status = ?response.status,
            "received proof status from monerochan network"
        );
        // Convert Monerochan response to base type for compatibility
        // Map JobStatus to FulfillmentStatus
        use monerochan::network::proto::api::JobStatus;
        let fulfillment_status = match JobStatus::try_from(response.status) {
            Ok(JobStatus::Succeeded) => FulfillmentStatus::Fulfilled,
            Ok(JobStatus::Failed) => FulfillmentStatus::Unfulfillable,
            Ok(JobStatus::Running) => FulfillmentStatus::Assigned,
            Ok(JobStatus::Pending) | Ok(JobStatus::Unspecified) | _ => FulfillmentStatus::Requested,
        };

        // Note: Monerochan response includes proof bytes directly, but base type uses proof_uri
        // For now, we don't populate proof_uri since we have the proof bytes
        
        // Set a reasonable deadline: current time + 1 hour (3600 seconds)
        let deadline_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() + 3600;
        
        Ok(GetProofRequestStatusResponse {
            fulfillment_status: fulfillment_status as i32,
            execution_status: 0, // Not used
            request_tx_hash: vec![],
            deadline: deadline_secs,
            fulfill_tx_hash: None,
            proof_uri: response.proof_uri, // Pass through the proof_uri from Monerochan network
            proof_public_uri: None,
            public_values_hash: None,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ProofRequest {
    /// Base64-encoded stdin payload (inline, preferred over stdin_uri)
    #[serde(default)]
    stdin: Option<String>,
    /// URI to fetch stdin from (used if stdin is not provided, deprecated)
    #[serde(default)]
    stdin_uri: Option<String>,
    /// Address of the user requesting the proof.
    address: String,
    /// Base64-encoded signature of the message.
    signature: String,
    /// The message that was signed (for verification).
    message: String,
    /// Timestamp when the message was signed (milliseconds since epoch).
    timestamp: u64,
    /// Whether the request is for a TEE prover.
    is_tee: bool,
    /// Optimistic amount of the user requesting the proof.
    amount: u64,
}

#[derive(Debug, Serialize)]
struct ProofResponse {
    /// Request identifier
    request_id: String,
}

#[derive(Debug, Serialize)]
struct GetProofResponse {
    status: String,
    proof_url: Option<String>,
    is_tee: bool,
    created_at: u64,
    fulfilled_at: Option<u64>,
    balance: Option<u64>,
    had_duplicate_nullifiers: bool,
}

fn parse_solana_address(raw: &str) -> Result<(String, Vec<u8>), AppError> {
    let trimmed = raw.trim();
    let decoded = bs58::decode(trimmed)
        .into_vec()
        .map_err(|_| AppError::bad_request("invalid base58 Solana address"))?;
    if decoded.len() != 32 {
        return Err(AppError::bad_request(
            "Solana address must decode to 32 bytes",
        ));
    }
    let normalized = bs58::encode(&decoded).into_string();
    Ok((normalized, decoded))
}

/// Convert s3://bucket/key into https://bucket.s3.amazonaws.com/key for direct HTTP GET
fn s3_to_https(uri: &str) -> Result<String> {
    if !uri.starts_with("s3://") {
        return Err(eyre::eyre!("expected s3 URI, got {}", uri));
    }
    let parts: Vec<&str> = uri.trim_start_matches("s3://").splitn(2, '/').collect();
    if parts.len() != 2 {
        return Err(eyre::eyre!("invalid s3 URI format: {}", uri));
    }
    Ok(format!("https://{}.s3.amazonaws.com/{}", parts[0], parts[1]))
}

fn verify_solana_signature(
    signature_bytes: &[u8],
    message: &str,
    address: &str,
    timestamp: u64,
) -> Result<(), SignatureError> {
    // Solana signatures are 64 bytes
    if signature_bytes.len() != 64 {
        return Err(SignatureError::new());
    }

    // Verify timestamp is recent (within 1 hour)
    // timestamp is in milliseconds (from frontend)
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let timestamp_seconds = timestamp / 1000; // Convert milliseconds to seconds
    let max_age_seconds = 3600u64; // 1 hour
    if timestamp_seconds < now.saturating_sub(max_age_seconds) || timestamp_seconds > now + 60 {
        // Allow 60 seconds clock skew for future timestamps
        return Err(SignatureError::new());
    }

    // Reconstruct the expected message (using milliseconds like the original message)
    let expected_message = format!(
        "Z.fun Verification & Legal Acknowledgment\n\nBy signing this message, I confirm:\n\n- I have read and agree to the Terms & Legal Documentation at app.z.fun/terms\n- I acknowledge this is experimental software with inherent risks and no warranties\n\nAddress: {}\nTimestamp: {}",
        address, timestamp
    );

    // Verify the provided message matches the expected message exactly
    if message != expected_message {
        tracing::warn!(
            "Message mismatch: expected len={}, got len={}",
            expected_message.len(),
            message.len()
        );
        // Log first difference
        let min_len = expected_message.len().min(message.len());
        for (i, (e, g)) in expected_message.chars().zip(message.chars()).enumerate() {
            if e != g {
                tracing::debug!(
                    "First difference at position {}: expected '{:?}' ({:?}), got '{:?}' ({:?})",
                    i,
                    e,
                    e as u32,
                    g,
                    g as u32
                );
                break;
            }
        }
        if expected_message.len() != message.len() {
            tracing::debug!(
                "Length mismatch: expected ends with '{}', got ends with '{}'",
                &expected_message[min_len.saturating_sub(20)..],
                &message[min_len.saturating_sub(20)..]
            );
        }
        return Err(SignatureError::new());
    }

    // Parse signature
    let signature = Signature::from_bytes(signature_bytes[..64].try_into().unwrap());

    // Parse public key from address
    let pubkey_bytes = bs58::decode(address)
        .into_vec()
        .map_err(|_| SignatureError::new())?;
    if pubkey_bytes.len() != 32 {
        return Err(SignatureError::new());
    }

    let verifying_key = VerifyingKey::from_bytes(pubkey_bytes[..32].try_into().unwrap())
        .map_err(|_| SignatureError::new())?;

    // Verify the signature against the message bytes
    // Solana wallet adapters sign the raw message bytes
    let message_bytes = message.as_bytes();
    verifying_key.verify_strict(message_bytes, &signature)
}

#[derive(Clone)]
struct StatusWorker {
    state: AppState,
    http: reqwest::Client,
}

impl StatusWorker {
    fn new(state: AppState) -> Result<Self> {
        let http = reqwest::Client::builder().build()?;
        Ok(Self { state, http })
    }

    async fn run(self) {
        let mut ticker = interval(Duration::from_secs(10));
        loop {
            ticker.tick().await;
            if let Err(err) = self.tick().await {
                tracing::warn!(?err, "status worker tick failed");
            }
        }
    }

    async fn tick(&self) -> Result<()> {
        tracing::debug!("status worker tick started");
        let pending = self
            .state
            .proof_store
            .list_pending_requests()
            .map_err(|e| {
                tracing::error!(error = ?e, "failed to list pending requests");
                eyre!(e)
            })?;
        tracing::debug!(
            pending_count = pending.len(),
            "found pending proof requests"
        );
        if pending.is_empty() {
            return Ok(());
        }
        // Process requests in parallel to avoid blocking on slow proof verification
        let tasks: Vec<_> = pending
            .into_iter()
            .map(|request| {
                let worker = self.clone();
                tokio::spawn(async move {
                    if let Err(err) = worker.process_request(request).await {
                        tracing::warn!(?err, "failed to process proof status");
                    }
                })
            })
            .collect();

        // Wait for all tasks to complete
        for task in tasks {
            let _ = task.await;
        }
        tracing::debug!("status worker tick completed");
        Ok(())
    }

    async fn process_request(&self, record: PendingRequest) -> Result<()> {
        debug!(
            request_id = record.request_id,
            is_tee = record.is_tee,
            "processing proof status"
        );
        tracing::debug!(
            request_id = record.request_id,
            "fetching proof status from monerochan network"
        );
        let response = self
            .state
            .service
            .get_proof(record.request_id.clone(), record.is_tee)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, request_id = record.request_id, "failed to get proof status from monerochan network");
                e
            })?;
        let mut status = format!("{:?}", response.fulfillment_status());
        tracing::debug!(
            request_id = record.request_id,
            fulfillment_status = %status,
            deadline = response.deadline,
            "received proof status from monerochan network"
        );
        if response.fulfillment_status() == FulfillmentStatus::Assigned
            && response.deadline
                < SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
        {
            tracing::warn!(
                request_id = record.request_id,
                deadline = response.deadline,
                "proof request deadline exceeded, marking as Unknown"
            );
            status = "Unknown".to_string();
        }
        let proof_url = response.proof_uri.clone();

        if status == "Fulfilled" {
            // Download proof and process public values (monerochan network already verified the proof)
            tracing::info!(
                request_id = record.request_id,
                "proof fulfilled, downloading and processing"
            );
            let Some(proof_url) = proof_url else {
                tracing::error!(
                    request_id = record.request_id,
                    "proof fulfilled but proof_url is missing"
                );
                return Err(eyre!("proof url is missing"));
            };
            tracing::debug!(request_id = record.request_id, proof_url = %proof_url, "downloading proof");
            let proof_bytes = self.download_proof(&proof_url).await
                .map_err(|e| {
                    tracing::error!(error = %e, request_id = record.request_id, proof_url = %proof_url, "failed to download proof");
                    e
                })?;
            tracing::debug!(
                request_id = record.request_id,
                proof_size_bytes = proof_bytes.len(),
                "proof downloaded, deserializing"
            );
            let proof: ProofFromNetwork = bincode::deserialize(&proof_bytes)
                .map_err(|e| {
                    tracing::error!(error = %e, request_id = record.request_id, "failed to deserialize proof");
                    e
                })?;
            let proof: MONEROCHANProofWithPublicValues = proof.into();
            let public_values = proof.public_values.to_vec();
            tracing::debug!(
                request_id = record.request_id,
                public_values_size = public_values.len(),
                "processing public values"
            );

            // Parse public values and record hashes (skip verification, trust monerochan network)
            let (status, amount, had_duplicate_nullifiers) = match bincode::deserialize::<ProcessedHoldings>(&public_values) {
                Ok(parsed) => {
                    info!(
                        request_id = record.request_id,
                        total_zatoshis = parsed.total_zatoshis,
                        alternate_nullifiers_count = parsed.alternate_nullifiers.len(),
                        transparent_utxo_commitments_count =
                            parsed.transparent_utxo_commitments.len(),
                        "parsed public values successfully"
                    );
                    let used_hashes = parsed
                        .alternate_nullifiers
                        .iter()
                        .chain(parsed.transparent_utxo_commitments.iter())
                        .map(|h| h.as_slice())
                        .collect::<Vec<_>>();
                    tracing::debug!(
                        request_id = record.request_id,
                        hash_count = used_hashes.len(),
                        "recording used hashes in database"
                    );

                    // Demo mode: allow duplicate nullifiers
                    let demo_mode = std::env::var("DEMO_MODE")
                        .unwrap_or_else(|_| "true".to_string())
                        .parse::<bool>()
                        .unwrap_or(true);

                    match self.state.proof_store.record_used_hashes_demo(&used_hashes, demo_mode) {
                        Ok((had_duplicates, _duplicate_hashes)) => {
                            if had_duplicates {
                                info!(
                                    request_id = record.request_id,
                                    hash_count = used_hashes.len(),
                                    "Some nullifiers were already used (demo mode - allowing re-verification)"
                                );
                            }
                            tracing::debug!(
                                request_id = record.request_id,
                                had_duplicates = had_duplicates,
                                "used hashes recorded successfully"
                            );
                            ("Fulfilled", Some(parsed.total_zatoshis), had_duplicates)
                        }
                        Err(err) => {
                            tracing::error!(
                                error = ?err,
                                request_id = record.request_id,
                                "failed to record used hashes"
                            );
                            ("Unknown", None, false)
                        }
                    }
                }
                Err(err) => {
                    tracing::error!(
                        ?err,
                        request_id = record.request_id,
                        public_values_size = public_values.len(),
                        "failed to parse public values"
                    );
                    ("Unknown", None, false)
                }
            };

            tracing::info!(
                request_id = record.request_id,
                status = %status,
                amount = ?amount,
                had_duplicates = had_duplicate_nullifiers,
                "updating proof status in database"
            );
            self.state.proof_store.update_status_with_duplicates(
                &record.request_id,
                status,
                Some(&proof_url),
                Some(proof_bytes),
                Some(public_values),
                amount,
                had_duplicate_nullifiers,
            )
            .map_err(|e| {
                tracing::error!(error = ?e, request_id = record.request_id, "failed to update proof status in database");
                e
            })?;
            tracing::debug!(
                request_id = record.request_id,
                "proof status updated successfully"
            );
        } else {
            tracing::debug!(
                request_id = record.request_id,
                status = %status,
                "updating proof status (not fulfilled)"
            );
            self.state.proof_store.update_status(
                &record.request_id,
                &status,
                proof_url.as_deref(),
                None,
                None,
                None,
            )
            .map_err(|e| {
                tracing::error!(error = ?e, request_id = record.request_id, "failed to update proof status in database");
                e
            })?;
        }

        Ok(())
    }

    async fn download_proof(&self, url: &str) -> Result<Vec<u8>> {
        tracing::debug!(url = %url, "downloading proof from URL");
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, url = %url, "HTTP request failed");
                e
            })?
            .error_for_status()
            .map_err(|e| {
                tracing::error!(error = %e, url = %url, "HTTP response error");
                e
            })?;
        let bytes = response.bytes().await.map_err(|e| {
            tracing::error!(error = %e, url = %url, "failed to read response bytes");
            e
        })?;
        tracing::debug!(url = %url, size_bytes = bytes.len(), "proof downloaded successfully");
        Ok(bytes.to_vec())
    }
}

fn spawn_status_worker(
    _prover: Arc<CpuProver>,
    _vk: Arc<MONEROCHANVerifyingKey>,
    state: AppState,
) -> Result<()> {
    let worker = StatusWorker::new(state)?;
    tokio::spawn(async move {
        worker.run().await;
    });
    Ok(())
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
struct AppError {
    status: StatusCode,
    message: String,
}

impl AppError {
    fn internal(error: impl Into<eyre::Report>) -> Self {
        let error = error.into();
        let error_msg = error.to_string();
        tracing::error!(?error, "internal server error");
        // Include error details in response for debugging
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error_msg,
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let body = Json(ErrorBody {
            message: self.message,
        });
        (self.status, body).into_response()
    }
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    message: String,
}

#[derive(Clone)]
struct ProverConfig {
    bind_address: SocketAddr,
    tee_private_key: Option<String>,
    base_private_key: String,
    tee_rpc_url: String,
    base_rpc_url: String,
}

impl ProverConfig {
    fn from_env() -> Result<Self> {
        let bind_address: SocketAddr = std::env::var("SERVER_BIND_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:3000".to_string())
            .parse()?;

        // TEE_PRIVATE_KEY is optional if TEE mode is not being used
        let tee_private_key = std::env::var("TEE_PRIVATE_KEY").ok();
        if tee_private_key.is_none() {
            warn!("TEE_PRIVATE_KEY not set. TEE mode will be disabled.");
        }
        let base_private_key = std::env::var("MONEROCHAN_NETWORK_PRIVATE_KEY")
            .or_else(|_| std::env::var("BASE_PRIVATE_KEY"))
            .expect("MONEROCHAN_NETWORK_PRIVATE_KEY or BASE_PRIVATE_KEY must be set in the environment");

        let tee_rpc_url = std::env::var("TEE_RPC_URL").unwrap_or_else(|_| {
            monerochan::network::utils::get_default_rpc_url_for_mode(NetworkMode::Reserved)
        });

        let base_rpc_url = std::env::var("BASE_RPC_URL").unwrap_or_else(|_| {
            monerochan::network::utils::get_default_rpc_url_for_mode(NetworkMode::Reserved)
        });

        Ok(Self {
            bind_address,
            tee_private_key,
            base_private_key,
            tee_rpc_url,
            base_rpc_url,
        })
    }
}
