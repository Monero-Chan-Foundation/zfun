# Z.FUN - Private Proof of Funds for Zcash

Built on top of the Monerochan.rs stack ([Monero-Chan Foundation / monerochan-rs](https://github.com/Monero-Chan-Foundation/monerochan-rs)) and powered by the Monerochan network ([explorer](https://explorer.monero-chan.org/)), Z.FUN routes every proof through Monerochan nodes. All fees paid while generating proofs flow back into the Monerochan ecosystem to sustain the network.

Prove your Zcash holdings without revealing your addresses or amounts.

## Overview

Z.FUN allows you to generate zero-knowledge proofs of your Zcash wallet balance, enabling you to prove solvency or creditworthiness without compromising privacy.

## Features

- **Privacy-Preserving**: Prove your balance without revealing addresses or transaction history
- **Browser-Based**: Generate proofs directly in your browser using WebAssembly
- **Decentralized**: Anyone can run their own Z.FUN server
- **Auditable**: All code is open source for security review

## Architecture

```
├── crates/
│   ├── lib/           # Core ZK proof verification logic
│   ├── wasm/          # Browser WebAssembly client
│   ├── server/        # API server for proof generation
│   ├── shard-builder/ # Merkle tree snapshot generation
│   ├── host/          # CLI proof generation tool
│   └── host-lib/      # Shared wallet integration logic
├── frontend/          # React web interface
└── vendor/            # Forked Zcash dependencies
```

## Documentation

The `docs/` directory hosts a Docusaurus site for long-form documentation. Common commands:

1. Install deps (first run): `cd docs && npm install`
2. Start local preview: `npm run start` (default `http://localhost:3000`, SSH-port-forward when developing remotely)
3. Build static site: `npm run build` (outputs to `docs/build/` for deployment on Cloudflare Pages/S3/etc.)

Keep all new guides/reference pages in Markdown/MDX under `docs/docs/` so they version with the codebase.

## How It Works

Z.FUN combines wallet data with a fixed-height snapshot so every proof is consistent and privacy-preserving.

### Data sources & why snapshots matter
- **Wallet DB (any height ≥ snapshot)**: Contains the note position, note data (value, rho, rseed, diversifier), and viewing key so we can derive commitments and nullifiers.
- **Snapshot (single block height)**: Frozen Orchard/Sapling trees, nullifier and UTXO sets, shard paths, and roots. Using snapshot data keeps Merkle paths and nullifier exclusion proofs aligned to the same height even if your wallet is synced later.

### Proof pipeline
1. **Build or download a snapshot**: `snapshot_build` writes `metadata.json` (roots, shard_paths) and `snapshot_metadata.json` (public roots/counts) plus shard binaries under `snapshots/<height>/`.
2. **Assemble witnesses on the host**: `zfun-host` reads your wallet export, selects unspent notes at the snapshot height, derives commitments/nullifiers from wallet secrets, and pulls 32-level Merkle paths from snapshot shards via `SnapshotWitnessSource`.
3. **Fetch server-side checks**: The server returns a nullifier exclusion proof showing each note was unspent at the snapshot height; it can also serve shards for browser-based proving.
4. **Prove inside the Monerochan.rs network prover**: The zk program recomputes commitments, walks the Merkle path using position bits to reach the snapshot root, checks nullifier exclusion, and commits the total value (optionally rounded) as the only public output.
5. **Verify anywhere**: Anyone can verify the proof against the published verification key map; no wallet data or addresses ever leave the prover.

### Snapshot layout (Merkle shards)
- Commitment trees are 32 levels deep; shards hold the bottom 16 levels for 65,536 positions each (for example, `shards/orchard/188.bin`).
- Each shard file is memory-mapped and walked to collect siblings for levels 0-15, with an LRU cache to avoid rereads.
- Shard root paths for levels 16-31 live in `metadata.json.shard_paths[pool][shard_id]` and are appended to complete the Merkle path that matches the snapshot root.
- `snapshot_metadata.json` mirrors the public roots/counts so the server and frontend can advertise the height being proven.

### Preconditions for a successful proof
- The note existed before the snapshot height and is unspent (validated by the nullifier exclusion proof).
- The wallet export includes commitment tree positions for each note.
- The prover has access to the snapshot directory (or remote shards) that matches the server's advertised height.

### End-to-end flow (host CLI path)
1. Export or point to a synced wallet: `wallet.db` (can be ahead of the snapshot height; positions are still valid).
2. Choose a snapshot directory: contains `metadata.json`, `snapshot_metadata.json`, and `shards/{orchard,sapling}/*.bin`.
3. Run `zfun-host`:
   - Reads snapshot metadata: roots, counts, shard_paths.
   - Opens shard binaries with mmap + LRU cache.
   - Queries wallet DB for unspent notes at the snapshot height.
   - Extracts 32-level Merkle paths for each note (16 from shard file, 16 from shard_paths).
   - Derives commitments/nullifiers from wallet secrets (no secrets leave the host).
   - Fetches nullifier exclusion proofs from the server.
   - Packages witnesses for the zkVM program.
4. Monerochan.rs network proof:
   - Recomputes commitments from note data inside the circuit.
   - Replays the Merkle path using position bits to reach the snapshot root.
   - Checks the nullifier exclusion proof against the nullifier set root.
   - Commits only the total proven value (optionally rounded) as public output.
5. Verification:
   - Public verifier uses the verification key map (`vk_map`) to verify the Monerochan.rs network proof.
   - Snapshot roots in the proof must match the published snapshot metadata for consistency.

### Browser flow (WASM path)
- The frontend uses `crates/wasm` bindings to:
  - Parse wallet export in the browser (positions + note data stay local).
  - Request shard slices and nullifier exclusion proofs from the server.
  - Build the witness and request proving; proving can be delegated to the server or run locally (if enabled).
- Privacy: the server only sees nullifiers and positions needed to serve Merkle paths and exclusion proofs; note secrets and amounts stay client-side.

### Snapshot anatomy and math
- Tree depth: 32 levels → ~4.29B leaves.
- Shard size: 2^16 leaves = 65,536 positions per shard → shard file ~4MB.
- Levels 0-15 live in shard binaries; levels 16-31 live in `metadata.json.shard_paths`.
- Position arithmetic:
  - `shard_id = position / 65536`
  - `position_in_shard = position % 65536`
  - Path direction at level `i` comes from bit `(position >> i) & 1`.
- Roots:
  - Orchard root, Sapling root, Nullifier set root, UTXO root all frozen at the snapshot height and stored in both `metadata.json` and `snapshot_metadata.json`.

### Proof inputs and outputs
- Private inputs (never leave prover):
  - Note data: value, rho, rseed, diversifier, recipient keys.
  - Full viewing key (to derive nullifier).
  - Commitment tree position per note.
  - Merkle path siblings (32 × 32-byte nodes).
  - Nullifier exclusion proof data.
- Public inputs:
  - Snapshot roots (orchard/sapling/nullifier/utxo).
  - Proven total value (may be rounded to a threshold bucket).
- Output artifact:
- `proof.bin` (Monerochan.rs network proof) plus public outputs that anyone can verify against the vk map.

### Operational notes and failure cases
- Mismatched heights: using wallet Merkle paths instead of snapshot shards will fail because roots differ; always source paths from the snapshot.
- Missing positions: wallets must include `commitment_tree_position`; if absent, the prover cannot locate the note in shards.
- New notes after snapshot: notes created after the snapshot height cannot be proven until a newer snapshot is used.
- Spent notes at snapshot height: nullifier exclusion proof will fail if the note was already spent.
- I/O performance: shard access is mmap + LRU cached; batching many notes in the same shard is fast, random shards incur extra page faults (~ms).
- GPU proving: set `MONEROCHAN_PROVER=cuda` to speed up proof generation if CUDA is available.

### Actors, trust boundaries, and data flow
- **Wallet / host**: Holds secrets (note data, viewing keys). Computes commitments/nullifiers. Never shares secrets beyond the proving environment.
- **Snapshot provider**: Publishes immutable tree data at a specific height. If snapshots are tampered, proofs will not verify because roots change.
- **Server**: Serves shard data and nullifier exclusion proofs. Learns nullifiers and positions requested but not note secrets or exact balances.
- **Verifier**: Consumes only the Monerochan.rs network proof, public outputs, and a verification key map. No access to wallet data.
- **Boundary**: Secrets never leave the host or browser proving context; all public data (roots, counts, total) are explicitly committed in the proof.

### Component map (where things live)
- `crates/shard-builder`: Builds snapshots, writes `metadata.json`, `snapshot_metadata.json`, and shard binaries.
- `crates/lib`: Shared primitives for reading snapshot metadata, roots, counts.
- `crates/host-lib`: Wallet extraction, witness assembly, shard reading (`SnapshotWitnessSource`), nullifier fetch.
- `crates/host`: CLI for local proving; wraps `host-lib`.
- `crates/wasm`: Browser bindings for witness assembly and proof requests.
- `crates/program`: Monerochan.rs zkVM program that validates commitments, Merkle paths, and nullifier exclusion, then commits the total.
- `crates/server`: Proof service: serves shards, provides nullifier exclusion proofs, exposes metadata.
- `crates/setup-script`: Snapshot tree builder binary (`snapshot_build`).

### Snapshot builder specifics
- Inputs: target height, optional Zebra cache path (when built with `--features zebra`), optional synthetic counts for testing.
- Outputs under `snapshots/<height>/`:
  - `metadata.json`: roots, counts, shard inventory, shard_paths for levels 16-31, per-pool shard counts.
  - `snapshot_metadata.json`: public roots and counts for server/frontend consumption.
  - `shards/sapling/*.bin`, `shards/orchard/*.bin`: level 0-15 data.
  - UTXO/nullifier data if configured.
- `metadata.json` fields (common):
  - `height`, `orchard_root`, `sapling_root`, `nullifier_root`, `utxo_root`
  - `orchard_count`, `sapling_count`, `nullifier_count`, `utxo_count`
  - `shards`: inventory per pool so clients know how many shard files to expect.
  - `shard_paths`: per-shard arrays of 16 siblings (hex) for levels 16-31.

### Shard binary format (per pool, per shard)
- Header (fixed-size):
  - Magic/id, version, pool id, shard_id, leaf_count.
  - Sanity checks ensure shard_id matches filename and leaf_count ≤ 65,536.
- Body (level-ordered nodes):
  - Level 0: 65,536 leaves × 32 bytes.
  - Level 1: 32,768 nodes × 32 bytes.
  - ...
  - Level 15: 2 nodes × 32 bytes (the shard roots).
- Path extraction:
  - Compute sibling index via XOR bit flip, read 32 bytes from mmap, descend up the tree for 16 levels.
  - Append shard_paths[pool][shard_id] from `metadata.json` for upper 16 levels.

### Cryptographic checks in the zkVM (high level)
- Recompute note commitments:
  - Orchard: Sinsemilla-style hash over Pallas curve (matches Zcash Orchard spec).
  - Sapling: Pedersen hash over Jubjub (matches Zcash Sapling spec).
- Merkle path replay:
  - Use position bits to order (left/right) hashing at every level.
  - Expect root to match the frozen snapshot root for that pool.
- Nullifier exclusion:
  - Validates provided exclusion proof against the snapshot nullifier set root (exact structure depends on server set encoding).
- Public commitment:
  - Commits the proven total (may be rounded/bucketed) plus snapshot identifiers so verifiers tie the proof to a specific snapshot height.

### Inside the Monerochan.rs zkVM
- Reconstruct Orchard (Sinsemilla) and Sapling (Pedersen) note commitments from wallet secrets.
- Walk the full 32-level Merkle path using the note position bits to ensure the commitment exists in the snapshot.
- Validate the nullifier exclusion proof to confirm the note is unspent and liquid at the snapshot height.
- Commit the total proven value as the only public output, ensuring verifiers learn nothing beyond the aggregate balance and snapshot metadata.

### Proving and verification artifacts
- **Prover input**: wallet export (`wallet.db`), snapshot directory, server URL for nullifier proof, optional GPU env (`MONEROCHAN_PROVER=cuda`).
- **Prover output**: `proof.bin` (Monerochan.rs network proof) plus public output JSON containing roots, counts, total proven value, and any rounding parameters.
- **Verifier input**: `proof.bin`, verification key map (for the Monerochan.rs prover stack), expected snapshot metadata (roots at the advertised height).
- **Verifier output**: Pass/fail; no access to note-level data.
- **TEE proving**: Trusted execution environment proving is supported via `tee.mainnet.monero-chan.org`. This path keeps all witness material inside the enclave for end-to-end privacy with zero information leakage, but each proof currently takes roughly 8–15 minutes. For this demo we keep TEE proving disabled by default to avoid long waits, though the endpoint remains online for full private runs.

### Latency and resource expectations
- Snapshot building: depends on machine and source data; reading from Zebra cache is IO-bound and can take hours at mainnet scale.
- Proving: CPU can be tens of minutes for large wallets; CUDA roughly halves this. Memory usage depends on circuit size (Orchard+Sapling combined).
- Merkle path fetch: local shard read is sub-millisecond (cache hit) to a few milliseconds (cache miss + page load); remote fetch adds network RTT.

### Operational safety checklist
- Ensure `metadata.json` and `snapshot_metadata.json` are from the same height and directory.
- Stop `zebrad` when building snapshots from a live cache to avoid RocksDB locks.
- Keep shard files immutable; any change invalidates roots and makes all proofs unverifiable.
- Version pin: honor `rust-toolchain` in this repo to avoid compiler drift.
- Log hygiene: server logs only operational metadata; avoid logging secrets or wallet exports when adding instrumentation.

### Concrete end-to-end example
```
Snapshot height: 3,140,000
Wallet sync height: 3,145,500
Note A: position 12,345,678 (Orchard), value 4.2 ZEC
Note B: position 98,765,432 (Sapling), value 7.8 ZEC
```
1. Snapshot builder produced `snapshots/3140000/` with shard `orchard/188.bin` and `sapling/1507.bin`.
2. `zfun-host --snapshot-dir snapshots/3140000 wallet.db`:
   - Loads metadata: sees orchard_root `0xd2bf...`, sapling_root `0x2910...`.
   - Queries wallet: both notes unspent at 3,140,000 even though wallet synced to 3,145,500.
   - Derives nullifiers using FVK (private).
   - Reads shard 188 (positions 12,288,000-12,353,535) and 1507 to extract bottom 16 siblings.
   - Appends upper siblings from metadata `shard_paths` arrays.
   - Requests `/nullifier_exclusion/<nullifier>` from server for each note.
   - Packages witness with both notes, including positions, note data, 32-level paths, exclusion proofs.
3. zkVM program:
   - Recomputes commitments for Note A/B (Orchard/Sapling logic).
   - Walks both Merkle paths to confirm commitments sit in roots `0xd2bf...` / `0x2910...`.
   - Checks nullifiers absent from spent set root `0xe36b...`.
   - Adds values (4.2 + 7.8) and rounds if configured (e.g., to nearest integer) before committing 12.0 ZEC.
4. Proof output includes `snapshot_height=3140000`, `orchard_root=...`, `sapling_root=...`, `total=12.0`.
5. Verifier checks:
   - `monerochan-verify --vk-map vk_map_v5.0.0 --proof proof.bin --public public.json`.
   - Confirms proof valid and snapshot roots match what the verifier expects for height 3,140,000.

### Nullifier exclusion proofs (conceptual)
- Purpose: prove the nullifier is NOT in the spent set up to snapshot height.
- The server maintains a commitment tree of nullifiers (e.g., binary tree or Merkle tree over sorted nullifiers).
- To prove absence, it returns:
  - The frontier that would contain the nullifier position (e.g., predecessor/successor hashes) and their Merkle paths.
  - Snapshot nullifier root, so zkVM ensures the exclusion proof matches the same root as the Merkle inclusion proof.
- zkVM logic:
  - Recompute expected leaf position for the nullifier.
  - Use provided path to confirm the slot is empty or contains a different nullifier.
  - Verify path root equals the nullifier root from snapshot metadata.

### Server API high-level overview
- `GET /metadata`: Returns `snapshot_metadata.json` (roots, counts, height) so clients can display the proving target.
- `GET /shards/<pool>/<shard_id>`: Streams shard binary; used by browsers or remote hosts lacking local snapshots.
- `GET /shard_paths/<pool>/<shard_id>`: Returns the 16 upper-level siblings if clients store shards but not metadata.
- `POST /nullifier_exclusion`: Body contains nullifier(s); response returns exclusion proofs + merkle paths.
- `POST /prove`: Optional endpoint allowing the server to run Monerochan.rs proving on behalf of a thin client; requires sending witness (without secrets if using TEE/secure channel).
- Rate limits/logging focus on abuse detection while avoiding storing user secrets.

### Browser witness builder details
- The WASM module mirrors `host-lib` but runs in-browser:
  - Parses wallet export (provided via file upload or wallet integration).
  - Requests shard slices lazily: only levels needed for requested positions.
  - Stores witnesses in IndexedDB/session memory if multiple attempts are needed.
- Proof options:
  - Local WASM proving (slow, experimental).
  - Remote proving via server `/prove` endpoint.
  - Hybrid: browser builds witness, sends to server's GPU prover.
- Browser privacy considerations:
  - Nullifiers + positions sent to server; amounts stay local.
  - CORS-limited endpoints ensure shards/exclusion proofs only served to authorized origins.

-### Monerochan verification flow
- Verification key map (`vk_map_v5.0.0`) maps program IDs to verification keys.
- Prover publishes program ID + proof.
- Verifier command example:
  ```bash
  monerochan-verify \
    --vk-map vk_map_v5.0.0 \
    --proof proof.bin \
    --public public.json
  ```
- `public.json` should contain:
  - `snapshot_height`
  - `orchard_root`, `sapling_root`, `nullifier_root`, `utxo_root`
  - `proven_total` (and rounding/granularity metadata)
- Verification fails if the vk map is outdated or roots mismatch expected values.

### Troubleshooting quick-reference
- **Error**: `Snapshot metadata missing shard_paths` → Ensure `metadata.json` exists and contains `shard_paths`; rebuild snapshot if necessary.
- **Error**: `Note missing commitment_tree_position` → Wallet DB not fully synced; rescan/reindex to populate positions.
- **Error**: `Nullifier exclusion proof invalid` → Note likely spent before snapshot height; confirm spend status or pick older note.
- **Error**: `Cannot mmap shard` → Check file permissions and that shard file is complete (size multiple of 32 bytes × node count).
- **Slow proving**: Enable CUDA (`MONEROCHAN_PROVER=cuda`) or reduce witness size (fewer notes).
- **Verification fails**: Ensure verifier uses the same `snapshot_metadata.json` values; mismatch indicates tampered metadata or wrong snapshot directory.

### Snapshot builder CLI reference
```
cargo run --release --bin snapshot_build -- [FLAGS] [OPTIONS]

Flags:
    --features zebra        # Enable Zebra integration for live data (requires zebrad stop)
    --orchard-only          # Only build Orchard tree
    --sapling-only          # Only build Sapling tree

Options:
    --height <u32>               # Target block height (required)
    --output-dir <path>          # Destination directory for metadata/shards
    --zebra-cache-dir <path>     # Path to Zebra rocksdb cache
    --network {mainnet,testnet}  # Network for Zebra parsing
    --orchard-count <u64>        # Synthetic leaf count (testing)
    --sapling-count <u64>        # Synthetic leaf count (testing)
    --chunk-size <u32>           # Notes per chunk when streaming from Zebra
```
- Output layout: `metadata.json`, `snapshot_metadata.json`, `shards/{pool}/{shard_id}.bin`, optional `nullifiers.bin`, `utxos.bin`, logs.
- Builder enforces deterministic ordering so identical inputs yield identical roots.

### Host CLI reference (`zfun-host`)
```
cargo run --release --bin zfun-host -- [FLAGS] [OPTIONS] <wallet-path>

Flags:
    --prove                  # Produce Monerochan network proof (default is witness-only)
    --verify                 # Verify proof after proving (requires proof + vk map)
    --sapling-only           # Ignore Orchard notes
    --orchard-only           # Ignore Sapling notes

Options:
    --snapshot-dir <path>            # Directory with metadata/shards
    --server-url <url>               # Proof service base URL
    --vk-map <path>                  # Monerochan verification key map (for --verify)
    --output <proof.bin>             # Output path for proof artifact
    --public <public.json>           # Output path for public inputs
    --rounding <u64>                 # Round total value to nearest multiple
    --account <index>                # Wallet account index (default 0)
```
- Default behavior without `--prove`: build witness to inspect note coverage.
- Supports environment overrides (see below) for prover backend selection.

### Environment variables
- `SNAPSHOT_DIR`: default snapshot path for CLI/server scripts.
- `MONEROCHAN_PROVER`: choose backend (`cpu`, `cuda`, `metal`).
- `RUST_LOG`: configure log verbosity (`info`, `debug`, etc.).
- `ZFUN_SERVER_CONFIG`: optional path to extended server config.
- `VK_MAP_SRC_PATH`: location of the Monerochan verification key map when building prover programs.

### Metadata schema reference
```json
{
  "height": 3140000,
  "timestamp": 1734550000,
  "roots": {
    "orchard": "d2bf...",
    "sapling": "2910...",
    "nullifier": "e36b...",
    "utxo": "e726..."
  },
  "counts": {
    "orchard": 49468062,
    "sapling": 73833641,
    "nullifier": 51869247,
    "utxo": 27937654
  },
  "shards": {
    "orchard": {
      "latest": 754,
      "total": 755
    },
    "sapling": {
      "latest": 1126,
      "total": 1127
    }
  },
  "shard_paths": {
    "orchard": {
      "188": ["a1b2...", "d4e5...", "..."],
      "189": ["..."]
    },
    "sapling": { "...": ["..."] }
  }
}
```
- `snapshot_metadata.json` is a subset: `snapshot_height`, `*_root`, `*_count`.
- Sign or hash metadata when distributing snapshots to prevent tampering.

### Wallet database expectations
- Format: SQLite DB exported by shielded wallet (Zcash reference schema).
- Required fields:
  - `received_notes.note_commitment_tree_position`
  - `received_notes.diversifier`, `amount`, `rseed`
  - Full viewing keys per account
- Host queries `select_unspent_notes(account, pools, target_height, tags)`.
- Missing positions → run wallet rescan/reindex to backfill tree positions.

### Nullifier data structure
- Snapshot stores sorted nullifier chunks and merkleizes them.
- Server maintains `nullifier_set` storage (RocksDB or flat files) and precomputes frontier nodes.
- Exclusion proofs typically depth 32; server responds in <10 ms per nullifier with caching.

### Server configuration knobs
- `snapshots_path`, `max_nullifier_batch`, `rate_limit`, `proof_timeout`, `log_level`, `metrics_port`, optional TLS settings.
- Multi-snapshot mode can expose `/metadata/<height>` for different heights.

### Server internal flow (nullifier request)
1. Receive JSON `{ "nullifiers": ["..."] }`.
2. For each nullifier:
   - Map to tree bucket.
   - Pull frontier chunk + siblings.
   - Build exclusion proof referencing current snapshot root.
3. Bundle results and include snapshot height/root for client verification.
4. Optionally sign response to authenticate server.

### Security assumptions and mitigations
- Snapshot integrity enforced by signed metadata/shard hashes.
- Server considered semi-trusted; zkVM re-derives everything so malicious responses fail verification.
- TLS/MTLS recommended for server endpoints; proving secrets kept local or inside TEE.
- Proofs embed snapshot height + nonce to avoid replay.
- Rate limits + CDN distribution defend against shard fetch DoS.

### Future enhancements
- Incremental snapshot streaming (delta shards).
- Multi-height server support with automatic fallback.
- Hardware wallet export helpers for note positions.
- Proof aggregation / batch verification.
- Configurable threshold proofs (prove ≥ X ZEC without exact sum).

### Monerochan.rs program internals (crates/program)
- Entry point reads witness:
  - Accounts array → orchard/sapling note records + optional transparent UTXOs.
  - Snapshot metadata (roots/counts) plus prover nonce.
  - Nullifier exclusion proofs per note.
- Circuit stages:
  1. **Note reconstruction**: derive note commitments using Orchard/Sapling gadget libraries (matches librustzcash spec).
  2. **Merkle verification**: reuse shared `MerklePath` gadget that iterates over 32 siblings, toggling left/right using position bits packed in little-endian order.
  3. **Nullifier exclusion**: custom gadget verifying absence proof structure (frontier nodes sorted, nullifier ordering preserved).
  4. **Value accumulation**: convert note values to common representation (zatoshi), apply rounding if configured, and enforce non-negative totals.
  5. **Output commitment**: commit to snapshot roots + rounded total; optionally include per-pool counts for auditability.
- Program emits digest consumed by verification key map; updates require regenerating vk map and distributing to verifiers.

### Host-lib key modules
- `snapshot_witness_source.rs`: handles shard loading, LRU cache, path extraction.
- `wallet.rs`: wrappers around `zcash_client_backend` to query SQLite wallets.
- `nullifier_fetch.rs`: async HTTP client hitting proof server for exclusion proofs (supports batching).
- `witness.rs`: structures for `HoldingsWitness`, `AccountWitness`, `MerkleWitness`.
- `proof.rs`: orchestrates Monerochan proving by invoking the prover SDK crate.
- Each module exposes async APIs so host binary remains responsive even when shards reside on network filesystems.

### Data formats
- **MerkleWitness JSON (debug tools)**:
  ```json
  {
    "position": 12345678,
    "path": ["hex32", "..."]  // 32 entries
  }
  ```
- **NullifierExclusionProof**:
  ```json
  {
    "nullifier": "hex32",
    "frontier": [
      {"direction": "left", "hash": "hex32"},
      ...
    ],
    "path": ["hex32", "..."]  // siblings to root
  }
  ```
- **Public outputs (`public.json`)**:
  ```json
  {
    "snapshot_height": 3140000,
    "orchard_root": "d2bf...",
    "sapling_root": "2910...",
    "nullifier_root": "e36b...",
    "utxo_root": "e726...",
    "total_zatoshi": "1200000000",
    "rounding_granularity": 100000000,  // optional
    "nonce": "hex32"
  }
  ```

### Testing strategy
- Unit tests:
  - `snapshot_witness_source` tests verifying shard path extraction for mock shards.
  - `nullifier_exclusion` tests to ensure malformed proofs are rejected.
  - zk program unit tests using small synthetic snapshots (few leaves).
- Integration tests:
  - End-to-end host test hitting mock server to fetch nullifiers and run a Monerochan proof.
  - Browser e2e using Playwright to upload wallet sample and request proof (in `frontend`).
- Benchmarks:
  - Shard loading benchmark to track path extraction latency per note.
  - Proving benchmark measuring CPU vs CUDA performance for various note counts.

### Logging and observability
- Host CLI logs via `tracing`:
  - Note counts per pool.
  - Shard cache hit/miss statistics (at debug level).
  - Nullifier exclusion response latency.
- Server logs:
  - Request metadata (IP, endpoint, duration).
  - Snapshot load success/failure.
  - Prover job lifecycle (queued/running/completed).
- Metrics:
  - Expose Prometheus metrics for shard cache hits, nullifier request latency, proving duration.
  - Health endpoints return snapshot height, shard counts, queue depth.

### Deployment notes
- Snapshot hosting: store `metadata.json` + shards in object storage (S3/GCS) with checksums; server can fetch and cache locally.
- Server scaling:
  - Stateless API nodes handle shard/metadata requests.
  - Dedicated prover workers with GPUs subscribe to job queue (Redis/NATS) to offload Monerochan proving.
  - Use systemd service (`zfun-server.service`) provided in repo for single-node deployments.
- Frontend build:
  - `frontend` uses Bun/Vite; environment variables configure API base URL, snapshot height display, and rounding policy.
  - Deploy static assets to CDN; ensure service worker caches snapshot metadata for offline reference.

### Data retention & privacy practices
- Server should avoid storing wallet dumps or nullifier histories beyond short-lived logs.
- Proof requests can include anonymized nonce to separate sessions without identifying data.
- Encourage users to self-host snapshots + server for maximum privacy; default server is convenience only.

### Compatibility considerations
- Supports both Orchard and Sapling pools; transparent UTXOs optional.
- Wallet exports from `zcashd` and lightwalletd-compatible clients work if they populate positions.
- Snapshot builder currently targets mainnet; testnet supported via `--network testnet`.
- The Monerochan zkVM stack requires the pinned Rust toolchain in this repo; ensure your toolchain matches `rust-toolchain` and dependencies in `Cargo.lock`.

## Getting Started

### Run the Frontend Locally

```bash
cd frontend
bun install
bun run dev
```

Visit `http://localhost:3000` and connect your Zcash wallet.

### Run Your Own Server

Requirements:
- Zebra full node (synced to mainnet)
- 100GB+ disk space for snapshots
- Rust toolchain

```bash
# Build snapshot
cargo run --release --bin snapshot_build -- --height <block_height>

# Start server
cargo run --release --bin zfun-server
```

## Security

**Warning**: This is experimental software. Use at your own risk.

- **Open Source**: Available for community audit
- **Security Audit**: Professional audit planned
- **Vulnerability Reports**: security@z.fun (DO NOT open public issues)

## Privacy Guarantees

### What the server learns:
- That someone generated a proof (IP address)
- Which shielded pools they use (Orchard/Sapling)
- Approximate proof generation time

### What the server NEVER learns:
- Your wallet addresses
- Which specific notes you own
- Your exact balance
- Your transaction history

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Submit a pull request

## License

MIT License - see [LICENSE](LICENSE) for details

## Acknowledgments

Built on top of:
- [Zcash Protocol](https://z.cash)
- [Zebra](https://github.com/ZcashFoundation/zebra) - Full node implementation
- [librustzcash](https://github.com/zcash/librustzcash) - Zcash cryptography
- [Monerochan.rs](https://github.com/Monero-Chan-Foundation/monerochan-rs) - zkVM + prover stack

---

**Disclaimer**: This software is provided "as is" without warranty. Users are responsible for verifying proofs and understanding the privacy implications.
