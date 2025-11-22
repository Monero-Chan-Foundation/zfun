/**
 * Dynamic WASM loader for Z.FUN
 * Fetches WASM from R2 CDN instead of bundling it
 */

const WASM_CDN_BASE =
  process.env.NEXT_PUBLIC_WASM_CDN_BASE || "https://wasm.z.fun/wasm/latest"
const SERVER_BASE_URL =
  process.env.NEXT_PUBLIC_SERVER_BASE_URL || "http://localhost:3000"

export interface SnapshotMetadata {
  snapshot_height: number
  orchard_root: string
  sapling_root: string
  orchard_count: number
  sapling_count: number
  [key: string]: unknown
}

/**
 * Fetch snapshot metadata from R2
 */
export async function fetchSnapshotMetadata(): Promise<SnapshotMetadata> {
  // Prefer server metadata so WASM version matches live snapshot; fallback to CDN
  const sources = [
    `${SERVER_BASE_URL}/api/snapshots/metadata`,
    `${WASM_CDN_BASE}/metadata.json`,
  ]

  let lastError: Error | null = null
  for (const url of sources) {
    try {
      const response = await fetch(url, { cache: "no-store" })
      if (!response.ok) {
        throw new Error(`HTTP ${response.status} (${response.statusText})`)
      }
      return response.json()
    } catch (err) {
      lastError =
        err instanceof Error
          ? err
          : new Error(typeof err === "string" ? err : "unknown error")
      // Try next source
    }
  }

  throw lastError || new Error("Failed to fetch snapshot metadata")
}

/**
 * Load WASM module from R2 CDN
 */
export async function loadWasmFromCDN(): Promise<any> {
  try {
    // Fetch metadata first to check version
    const metadata = await fetchSnapshotMetadata()
    console.log(`Loading WASM for snapshot height: ${metadata.snapshot_height}`)

    // Add cache buster using snapshot height to ensure JS and WASM are in sync
    const cacheBuster = `?v=${metadata.snapshot_height}`

    // Dynamically import the WASM module from R2
    // The JS glue code will fetch the .wasm file from the same directory
    const wasmModule = await import(/* webpackIgnore: true */ `${WASM_CDN_BASE}/zfun_wasm.js${cacheBuster}`)

    // Check if default export exists and is a function
    if (!wasmModule.default || typeof wasmModule.default !== 'function') {
      throw new Error(
        'WASM module default export is not a function. ' +
        'The JS and WASM files may be mismatched. ' +
        'Please rebuild and re-upload both files together.'
      )
    }

    // Initialize the WASM module
    // The init function will fetch the .wasm file
    // wasm-pack generates a default export that accepts the WASM file path/URL
    const wasmUrl = `${WASM_CDN_BASE}/zfun_wasm_bg.wasm${cacheBuster}`
    
    try {
      await wasmModule.default(wasmUrl)
    } catch (initError: any) {
      // Check for the specific wasm-bindgen import error
      if (initError?.message?.includes('Import') && initError?.message?.includes('function import requires a callable')) {
        throw new Error(
          'WASM JS glue code and WASM binary are mismatched. ' +
          'This happens when the JS and WASM files are from different builds. ' +
          'Please rebuild the WASM and re-upload both files together:\n' +
          '1. cd zfun-private/crates/wasm && wasm-pack build --target web\n' +
          '2. cd ../../frontend && ./scripts/upload-wasm.sh\n' +
          '3. Hard refresh your browser (Cmd+Shift+R)'
        )
      }
      throw initError
    }

    console.log("WASM loaded successfully from R2")

    return wasmModule
  } catch (error) {
    console.error("Failed to load WASM from CDN:", error)
    throw error
  }
}

/**
 * Get cached metadata from localStorage
 */
export function getCachedMetadata(): SnapshotMetadata | null {
  try {
    const cached = localStorage.getItem("zfun_snapshot_metadata")
    if (!cached) return null

    const data = JSON.parse(cached)
    // Check if cache is less than 5 minutes old
    const cacheAge = Date.now() - (data.timestamp || 0)
    if (cacheAge > 5 * 60 * 1000) {
      return null
    }

    return data.metadata
  } catch {
    return null
  }
}

/**
 * Cache metadata to localStorage
 */
export function cacheMetadata(metadata: SnapshotMetadata): void {
  try {
    localStorage.setItem(
      "zfun_snapshot_metadata",
      JSON.stringify({
        metadata,
        timestamp: Date.now(),
      })
    )
  } catch {
    // Ignore localStorage errors
  }
}
