"use client"

import type { DragEvent } from "react"
import { useCallback, useEffect, useMemo, useRef, useState } from "react"

import type { WalletName } from "@solana/wallet-adapter-base"
import type { Analysis, StepDefinition } from "@/lib/types"
import { findShieldedBalances } from "@/lib/utils"
import { Navbar } from "../../components/navbar"
import { useWallet } from "@solana/wallet-adapter-react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { FAQ } from "./components/FAQ"
import { Footer } from "../../components/Footer"
import { FileUploadStep } from "./components/FileUploadStep"
import { StepIndicator } from "./components/StepIndicator"
import { InspectStep } from "./components/InspectStep"
import { WalletConnectionStep } from "./components/WalletConnectionStep"
import { SubmitStep } from "./components/SubmitStep"

// Dynamic WASM loading from R2 CDN
import { loadWasmFromCDN, fetchSnapshotMetadata } from "@/lib/wasm-loader"

// WASM functions - will be loaded dynamically from R2
let loadProcessedInput: any
let requestProof: any
let getSnapshotMetadata: any

// Type for ProofInput (imported for type checking only)
type ProofInput = any

const ZATOSHIS_PER_ZEC = 100_000_000

function formatError(error: unknown): string {
  if (error instanceof Error) {
    return error.message
  }
  if (typeof error === "string") {
    return error
  }
  try {
    return JSON.stringify(error)
  } catch {
    return "Unknown error"
  }
}

type WasmShieldedSnapshot = {
  height: number
  accounts: unknown[]
}

type WasmProcessedSnapshot = {
  total_zatoshis: number
  alternate_nullifiers: unknown[]
}

type ProofStatusResponse = {
  status?: string
  proof_url?: string
  [key: string]: unknown
}

const PROOF_API_BASE =
  process.env.NEXT_PUBLIC_SERVER_BASE_URL?.replace(/\/+$/, "") ?? "http://localhost:3000"

const STEPS: StepDefinition[] = [
  { id: 1, label: "UPLOAD" },
  { id: 2, label: "INSPECT" },
  { id: 3, label: "CONNECT WALLET" },
  { id: 4, label: "SUBMIT" },
]


function loadFileContents(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onerror = () => reject(reader.error ?? new Error("Unable to read file"))
    reader.onabort = () => reject(new Error("File reading aborted"))
    reader.onload = () => resolve(String(reader.result ?? ""))
    reader.readAsText(file)
  })
}

export default function AppPage() {
  const { connected, publicKey, disconnect, signMessage, wallet, select } = useWallet()
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()
  const requestIdParam = searchParams.get("id")
  const inputRef = useRef<HTMLInputElement>(null)
  const proofInputRef = useRef<ProofInput | null>(null)
  const skipParamSyncRef = useRef(false)
  const lastStatusFetchRef = useRef(0)
  const [mode] = useState<"shielded">("shielded")
  const [analysis, setAnalysis] = useState<Analysis | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [isProcessing, setIsProcessing] = useState(false)
  const [step, setStep] = useState<number>(1)
  const [uploadedFileName, setUploadedFileName] = useState<string | null>(null)
  // const isDevMode = process.env.NODE_ENV !== "production"
  const isDevMode = false
  const placeholderShieldedAddress = "zs1xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
  const placeholderShieldedBalance = "12.34567890"
  const [isRequestingProof, setIsRequestingProof] = useState(false)
  const [proofQueued, setProofQueued] = useState(false)
  const [proofQueueId, setProofQueueId] = useState<string | null>(null)
  const [submittedAt, setSubmittedAt] = useState<number | null>(null)
  const [submittedProofMode, setSubmittedProofMode] = useState<"zk-tee" | "zk-only" | null>(null)
  const [isWasmReady, setIsWasmReady] = useState(false)
  const [wasmInitError, setWasmInitError] = useState<string | null>(null)
  const [snapshot, setSnapshot] = useState<WasmShieldedSnapshot | null>(null)
  const [processedInput, setProcessedInput] = useState<WasmProcessedSnapshot | null>(null)
  const [statusData, setStatusData] = useState<ProofStatusResponse | null>(null)
  const [statusError, setStatusError] = useState<string | null>(null)
  const [isStatusLoading, setIsStatusLoading] = useState(false)
  const [addressCopied, setAddressCopied] = useState(false)
  const [isRequestingSignature, setIsRequestingSignature] = useState(false)
  const [signatureReceived, setSignatureReceived] = useState(false)
  const [signatureError, setSignatureError] = useState<string | null>(null)
  const [walletSignature, setWalletSignature] = useState<string | null>(null)
  const [signedMessage, setSignedMessage] = useState<string | null>(null)
  const [signatureTimestamp, setSignatureTimestamp] = useState<number | null>(null)
  const submittedAtDisplay = useMemo(() => {
    if (!submittedAt) return null
    try {
      return new Date(submittedAt).toLocaleString()
    } catch {
      return String(submittedAt)
    }
  }, [submittedAt])

  const statusUpdatedDisplay = useMemo(() => {
    const timestamp =
      (typeof statusData?.updated_at === "string" && statusData.updated_at) ||
      (typeof statusData?.created_at === "string" && statusData.created_at) ||
      null
    if (!timestamp) return null
    const date = new Date(timestamp)
    return Number.isNaN(date.getTime()) ? timestamp : date.toLocaleString()
  }, [statusData])

  const derivedStatus = useMemo(() => {
    if (statusData) {
      const rawStatus =
        (typeof statusData.status === "string" && statusData.status) ||
        (typeof statusData.state === "string" && statusData.state) ||
        null
      if (rawStatus && rawStatus.length > 0) {
        const normalized = rawStatus.toLowerCase()
        if (normalized === "assigned") {
          return "Processing"
        }
        if (normalized === "fulfilled") {
          return "Processed"
        }
        if (normalized === "requested") {
          return "Pending"
        }
        if (normalized === "unfulfillable") {
          return "Failed"
        }
        if (normalized === "unknown") {
          return "Unknown"
        }
        // For any other status, return the raw value
        return rawStatus
      }
    }
    return proofQueued ? "Processing" : "Pending"
  }, [proofQueued, statusData])

  const normalizedStatus = useMemo(() => derivedStatus.toLowerCase(), [derivedStatus])

  const formattedStatus = useMemo(() => {
    if (!derivedStatus) return "Unknown"
    const cleaned = derivedStatus.replace(/[_-]+/g, " ").trim()
    if (!cleaned) return "Unknown"
    return cleaned
      .split(" ")
      .map((word) => (word ? word[0].toUpperCase() + word.slice(1) : ""))
      .join(" ")
  }, [derivedStatus])

  const statusMessage = useMemo(() => {
    if (statusData?.message && typeof statusData.message === "string" && statusData.message.length > 0) {
      return statusData.message
    }
    switch (normalizedStatus) {
      case "processing":
      case "queued":
      case "pending":
        return "You can safely close this page and check back later. Your request is in queue to be processed by the TEE prover. This may take a while depending on compute availability."
      case "proving":
      case "running":
        return "The prover is generating your proof. This usually takes a few minutes."
      case "processed":
      case "completed":
      case "success":
        return "Proof generation finished successfully. Holdings have been successfully verified. The proof does not reveal any shielded information."
      case "failed":
      case "error":
      case "unknown":
        return "The prover encountered an error. Please try again or contact support."
      default:
        return "Status updated. You can come back to this page any time to keep tracking progress."
    }
  }, [normalizedStatus, statusData])

  const statusIndicatorColor = useMemo(() => {
    switch (normalizedStatus) {
      case "processed":
      case "completed":
      case "success":
        return "bg-emerald-500"
      case "processing":
      case "queued":
      case "pending":
      case "proving":
      case "running":
        return "bg-[#f3b724] animate-pulse"
      case "failed":
      case "error":
      case "unknown":
        return "bg-red-500"
      default:
        return "bg-slate-500"
    }
  }, [normalizedStatus])

  const isTerminalStatus = useMemo(
    () => ["processed", "completed", "success"].includes(normalizedStatus),
    [normalizedStatus],
  )

  const formatRequestId = useCallback((value: string) => {
    if (value.length <= 8) {
      return value
    }
    return `${value.slice(0, 4)}...${value.slice(-4)}`
  }, [])

  const fetchStatus = useCallback(
    async (id: string) => {
      if (!id) return
      setIsStatusLoading(true)
      setStatusError(null)
      try {
        const response = await fetch(`${PROOF_API_BASE}/proof/${encodeURIComponent(id)}`, {
          method: "GET",
          headers: { "Content-Type": "application/json" },
          cache: "no-store",
        })

        if (!response.ok) {
          throw new Error(`Proof status fetch failed (${response.status})`)
        }

        const body = (await response.json()) as ProofStatusResponse
        if (proofQueueId && proofQueueId !== id) {
          return
        }
        setStatusData(body)
        const created =
          typeof body.created_at === "string" ? Date.parse(body.created_at) : Number.NaN
        const updated =
          typeof body.updated_at === "string" ? Date.parse(body.updated_at) : Number.NaN
        const timestamp = !Number.isNaN(created) ? created : !Number.isNaN(updated) ? updated : null
        if (timestamp !== null) {
          setSubmittedAt(timestamp)
        }
      } catch (err) {
        if (proofQueueId && proofQueueId !== id) {
          return
        }
        setStatusError(err instanceof Error ? err.message : "Failed to fetch proof status")
      } finally {
        if (proofQueueId && proofQueueId !== id) {
          return
        }
        setIsStatusLoading(false)
      }
    },
    [proofQueueId],
  )

  useEffect(() => {
    if (skipParamSyncRef.current) {
      if (!requestIdParam) {
        skipParamSyncRef.current = false
      }
      return
    }
    if (!requestIdParam) {
      return
    }
    // Redirect to the dedicated proof page
    router.replace(`/proof/${encodeURIComponent(requestIdParam)}`, { scroll: false })
  }, [requestIdParam, router])

  useEffect(() => {
    if (!proofQueueId) {
      setStatusData(null)
      setStatusError(null)
      setIsStatusLoading(false)
      lastStatusFetchRef.current = 0
      return
    }

    const now = Date.now()
    if ((!isTerminalStatus || !statusData) && now - lastStatusFetchRef.current >= 10_000) {
      lastStatusFetchRef.current = now
      setIsStatusLoading(true)
      void fetchStatus(proofQueueId)
    }

    if (isTerminalStatus) {
      return
    }

    const interval = setInterval(() => {
      lastStatusFetchRef.current = Date.now()
      setIsStatusLoading(true)
      void fetchStatus(proofQueueId)
    }, 10_000)
    return () => clearInterval(interval)
  }, [fetchStatus, isTerminalStatus, proofQueueId, statusData])

  const handleRefreshStatus = useCallback(() => {
    if (proofQueueId) {
      lastStatusFetchRef.current = Date.now()
      setIsStatusLoading(true)
      void fetchStatus(proofQueueId)
    }
  }, [fetchStatus, proofQueueId])

  useEffect(() => {
    let cancelled = false

    async function initialiseWasm() {
      try {
        // Load WASM dynamically from R2 CDN
        const wasmModule = await loadWasmFromCDN()

        if (!cancelled) {
          // Assign the loaded functions
          loadProcessedInput = wasmModule.load_processed_input
          requestProof = wasmModule.request_proof
          getSnapshotMetadata = wasmModule.get_snapshot_metadata

          setIsWasmReady(true)
          setWasmInitError(null)

          console.log("WASM initialized successfully from R2")
        }
      } catch (err) {
        console.error("Failed to initialise wasm from R2:", err)
        if (!cancelled) {
          setWasmInitError(formatError(err))
        }
      }
    }

    void initialiseWasm()

    return () => {
      cancelled = true
    }
  }, [])

  useEffect(() => {
    return () => {
      if (proofInputRef.current) {
        try {
          proofInputRef.current.free()
        } catch {
          // ignore
        }
        proofInputRef.current = null
      }
    }
  }, [])

  const snapshotMetadata = useMemo(() => {
    if (!isWasmReady) return null
    const metadata = getSnapshotMetadata()
    if (metadata) {
      console.log("Snapshot Metadata:")
      console.log("  orchard_root:", metadata?.orchard_root)
      console.log("  sapling_root:", metadata?.sapling_root)
      console.log("  orchard_count:", metadata?.orchard_count)
      console.log("  sapling_count:", metadata?.sapling_count)
    }
    return metadata
  }, [isWasmReady])

  const handleFile = useCallback(async (file: File | null) => {
    if (!file) return

    if (proofInputRef.current) {
      try {
        proofInputRef.current.free()
      } catch {
        // ignore double free
      }
      proofInputRef.current = null
    }

    setStep(1)
    setIsProcessing(true)
    setError(null)
    setAnalysis(null)
    setUploadedFileName(file.name)
    setSnapshot(null)
    setProcessedInput(null)
    setProofQueued(false)
    setProofQueueId(null)
    setIsRequestingProof(false)
    setStatusData(null)
    setStatusError(null)
    setIsStatusLoading(false)
    setSubmittedAt(null)
    skipParamSyncRef.current = true
    lastStatusFetchRef.current = 0
    router.replace(pathname, { scroll: false })

    try {
      const name = file.name.toLowerCase()
      const isDb = name.endsWith(".db") || name.endsWith(".sqlite") || name.endsWith(".sqlite3")
      const isJson = name.endsWith(".json") || file.type === "application/json"

      if (isDb) {
        if (!isWasmReady) {
          const message = wasmInitError ?? "WebAssembly runtime is still initialising. Try again in a moment."
          setError(message)
          return
        }

        const buffer = await file.arrayBuffer()
        const bytes = new Uint8Array(buffer)

        try {
          const baseUrl = process.env.NEXT_PUBLIC_SERVER_BASE_URL;
          if (!baseUrl) {
            throw new Error("Server base URL is not set")
          }
          const result = await loadProcessedInput(bytes, baseUrl)
          const snapshotData = result.snapshot()
          const processedData = result.processed()

          proofInputRef.current = result

          console.log("processedData", processedData)
          console.log("snapshotData", snapshotData)

          setSnapshot(snapshotData)
          setProcessedInput(processedData)
          setStep(2)
        } catch (err) {
          console.error("Failed to load processed input", err)
          // Format error message with more context
          let errorMessage = "Failed to load processed input"
          if (err instanceof Error) {
            errorMessage = err.message || errorMessage
            // Add more context if available
            if (err.message.includes("database") || err.message.includes("sqlite")) {
              errorMessage = `Database error: ${err.message}. Please ensure you uploaded a valid Zashi wallet export file.`
            } else if (err.message.includes("snapshot") || err.message.includes("height")) {
              errorMessage = `Snapshot error: ${err.message}. The database may be from an incompatible Zcash network version.`
            } else if (err.message.includes("memory") || err.message.includes("allocation")) {
              errorMessage = `Memory error: ${err.message}. The database file may be too large or corrupted.`
            }
          } else if (typeof err === "string") {
            errorMessage = err
          }
          setError(errorMessage)
          setIsProcessing(false)
          // Stay on step 1 so user can try again without needing to reset
          return
        }
        return
      }

      if (!isJson) {
        setError("Please upload a Zashi-exported .db database file.")
        return
      }

      const raw = await loadFileContents(file)
      const parsed = JSON.parse(raw)
      const matches = findShieldedBalances(parsed)

      if (!matches.length) {
        setAnalysis({ fileName: file.name, matches: [], total: 0 })
        setStep(2)
        return
      }

      const sanitizedMatches = matches.filter((match) => Number.isFinite(match.value))
      const total = sanitizedMatches.reduce((sum, match) => sum + match.value, 0)

      setAnalysis({ fileName: file.name, matches: sanitizedMatches, total })
      setStep(2)
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to read file")
    } finally {
      setIsProcessing(false)
    }
  }, [isWasmReady, pathname, router, wasmInitError])

  const handleDrop = useCallback(
    (event: DragEvent<HTMLDivElement>) => {
      event.preventDefault()
      if (event.dataTransfer.files && event.dataTransfer.files[0]) {
        void handleFile(event.dataTransfer.files[0])
      }
    },
    [handleFile],
  )

  const eligible = analysis ? analysis.total > 0 : false
  const resetWorkflow = useCallback(() => {
    if (proofInputRef.current) {
      try {
        proofInputRef.current.free()
      } catch {
        // ignore
      }
      proofInputRef.current = null
    }
    setStep(1)
    setAnalysis(null)
    setError(null)
    setSnapshot(null)
    setProcessedInput(null)
    setIsProcessing(false)
    setIsRequestingProof(false)
    setProofQueued(false)
    setProofQueueId(null)
    setStatusData(null)
    setStatusError(null)
    setIsStatusLoading(false)
    setSubmittedAt(null)
    setUploadedFileName(null)
    // Reset signature state
    setWalletSignature(null)
    setSignedMessage(null)
    setSignatureTimestamp(null)
    setSignatureReceived(false)
    setSignatureError(null)
    setIsRequestingSignature(false)
    skipParamSyncRef.current = true
    lastStatusFetchRef.current = 0
    router.replace(pathname, { scroll: false })
    if (inputRef.current) {
      inputRef.current.value = ""
    }
  }, [pathname, router])
  const hasWasmResult = processedInput !== null

  const totalShieldedZec = useMemo(() => {
    if (!processedInput) return null
    const raw = processedInput.total_zatoshis
    if (typeof raw !== "number" || Number.isNaN(raw)) return null
    return raw / ZATOSHIS_PER_ZEC
  }, [processedInput])

  const shieldedBalanceDisplay = useMemo(() => {
    if (totalShieldedZec === null) return null
    return totalShieldedZec.toLocaleString(undefined, {
      minimumFractionDigits: 0,
      maximumFractionDigits: 8,
    })
  }, [totalShieldedZec])

  const snapshotHeightLabel = useMemo(() => {
    if (!snapshot || typeof snapshot.height !== "number") return null
    return snapshot.height.toLocaleString()
  }, [snapshot])

  const targetHeightLabel = useMemo(() => {
    if (!isWasmReady) return null
    if (!snapshotMetadata) return null
    return snapshotMetadata.height.toLocaleString()
  }, [isWasmReady, snapshotMetadata])

  const balanceDisplay = useMemo(() => {
    if (!analysis) return null
    const formatter = new Intl.NumberFormat("en-US", {
      minimumFractionDigits: 0,
      maximumFractionDigits: 8,
    })
    return formatter.format(analysis.total)
  }, [analysis])

  const progressPercent = useMemo(() => {
    const total = STEPS.length
    if (total <= 1) return 100
    const clampedStep = Math.max(1, Math.min(step, total))
    const percent = ((clampedStep - 1) / (total - 1)) * 100
    return Math.max(0, Math.min(100, percent))
  }, [step])

  // Step advancement is controlled by proof request; no manual finalize step.

  function setCookie(name: string, value: string, days = 30) {
    const expires = new Date(Date.now() + days * 864e5).toUTCString()
    document.cookie = `${name}=${encodeURIComponent(value)}; path=/; expires=${expires}; samesite=lax`
  }

  const handleRequestProof = useCallback(async (proofMode: "zk-tee" | "zk-only" | null) => {
    if (isRequestingProof || proofQueued || !proofMode || error) return
    const isTee = proofMode === "zk-tee"

    const proofInput = proofInputRef.current
    if (!proofInput) {
      setError("Upload a wallet DB export before requesting a proof.")
      return
    }

    setIsRequestingProof(true)
    setError(null)
    setStatusData(null)
    setStatusError(null)

    try {
      if (!proofInput) {
        throw new Error("Proof input is required")
      }

      proofInputRef.current = null
      const serverBaseUrl = process.env.NEXT_PUBLIC_SERVER_BASE_URL;
      if (!serverBaseUrl) {
        throw new Error("Server base URL is not set")
      }
      const teeBaseUrl = process.env.NEXT_PUBLIC_TEE_BASE_URL;
      if (!teeBaseUrl) {
        throw new Error("TEE base URL is not set")
      }

      // Choose the appropriate endpoint based on proof mode
      const baseUrl = proofMode === "zk-tee" ? teeBaseUrl : serverBaseUrl

      if (!walletSignature || !signedMessage || signatureTimestamp === null) {
        throw new Error("Signature is required. Please sign the message in the wallet connection step.")
      }

      const queueResp = (await requestProof(serverBaseUrl, baseUrl, proofInput, publicKey?.toBase58() ?? "", walletSignature, signedMessage, String(signatureTimestamp), isTee)) as Map<string, string>
      let queueId =
        queueResp.get("request_id") ?? queueResp.get("id") ?? queueResp.get("job_id") ?? null
      if (!queueId) {
        const stdinUri = queueResp.get("stdin_uri") ?? null
        if (stdinUri) {
          queueId = stdinUri.split("/").pop() ?? null
        }
      }

      if (!queueId) {
        if (queueResp.get("message")) {
          throw new Error(queueResp.get("message"))
        }
        throw new Error("Proof service did not return a request id")
      }

      setProofQueueId(queueId)
      try {
        const record = {
          id: queueId,
          mode,
          proofMode,
          address: placeholderShieldedAddress,
          file: uploadedFileName ?? analysis?.fileName ?? null,
          ts: Date.now(),
        }
        setCookie("zproof_request", JSON.stringify(record), 14)
      } catch {
        // ignore cookie errors
      }

      // Redirect immediately without showing processing state
      router.replace(`/proof/${encodeURIComponent(queueId)}`, { scroll: false })
    } catch (err) {
      console.error("Failed to request proof", err)
      setError(err instanceof Error ? err.message : typeof err === "string" ? err : "Failed to request proof")
    } finally {
      setIsRequestingProof(false)
    }
  }, [analysis, error, fetchStatus, isRequestingProof, mode, pathname, placeholderShieldedAddress, proofQueued, publicKey, router, uploadedFileName, walletSignature, signedMessage, signatureTimestamp])

  const handleCopyAddress = useCallback(async () => {
    if (!publicKey) return
    try {
      await navigator.clipboard.writeText(publicKey.toBase58())
      setAddressCopied(true)
      setTimeout(() => setAddressCopied(false), 2000)
    } catch {
      // ignore
    }
  }, [publicKey])

  const handleDisconnect = useCallback(async () => {
    // Immediately reset local state
    setSignatureReceived(false)
    setWalletSignature(null)
    setSignedMessage(null)
    setSignatureTimestamp(null)
    setSignatureError(null)
    setIsRequestingSignature(false)

    try {
      // Disconnect and clear the selected wallet
      await disconnect()
      // Deselect wallet after disconnect completes so next connect shows picker
      select(null)
    } catch (err) {
      console.error("Disconnect error:", err)
      // Even if disconnect fails, try to deselect anyway
      try {
        select(null)
      } catch {
        // ignore
      }
    }
  }, [disconnect, select])

  // Request signature from wallet
  const requestWalletSignature = useCallback(async () => {
    if (!publicKey || !signMessage) return

    setIsRequestingSignature(true)
    setSignatureError(null)

    try {
      // Create a message to sign with legal acknowledgments
      const timestamp = Date.now()
      const message = `Z.fun Verification & Legal Acknowledgment

By signing this message, I confirm:

- I have read and agree to the Terms & Legal Documentation at app.z.fun/terms
- I acknowledge this is experimental software with inherent risks and no warranties

Address: ${publicKey.toBase58()}
Timestamp: ${timestamp}`
      const messageBytes = new TextEncoder().encode(message)

      // Request signature - signMessage returns Uint8Array
      const signature = await signMessage(messageBytes)

      // Convert signature to base64 for storage
      // signature is Uint8Array from wallet adapter
      const signatureBase64 = Buffer.from(signature).toString('base64')

      setWalletSignature(signatureBase64)
      setSignedMessage(message)
      setSignatureTimestamp(timestamp)
      setSignatureReceived(true)
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : "Failed to get signature"
      setSignatureError(errorMessage)
      setSignatureReceived(false)
    } finally {
      setIsRequestingSignature(false)
    }
  }, [publicKey, signMessage])

  // Reset validation states when wallet disconnects (for any disconnect not triggered by our button)
  useEffect(() => {
    if (!connected || !publicKey) {
      setSignatureReceived(false)
      setWalletSignature(null)
      setSignedMessage(null)
      setSignatureTimestamp(null)
      setSignatureError(null)
      setIsRequestingSignature(false)
    }
  }, [connected, publicKey])

  // Step components for progress bar
  // Step 0 (Method) removed; shielded-only flow

  return (
    <>
      <Navbar />
      <div
        className="min-h-screen text-slate-100 relative overflow-hidden"
        style={{
          background: `linear-gradient(180deg,rgb(18,18,18),rgba(18,18,18,.99) 20%,rgba(18,18,18,.98) 30%,rgba(18,18,18,.93)),radial-gradient(ellipse at 50% 20%,rgba(255,255,255,0.15),rgba(255,255,255,0.05),rgba(10,12,20,0.95))`
        }}
      >
        {/* Ambient glow effects */}
        <div className="absolute top-0 left-1/2 transform -translate-x-1/2 w-full max-w-4xl h-64 bg-gradient-to-b from-white/8 via-white/4 to-transparent rounded-full blur-3xl pointer-events-none"></div>
        <div className="absolute bottom-0 right-0 w-96 h-64 bg-gradient-to-tl from-white/5 via-white/2 to-transparent rounded-full blur-3xl pointer-events-none"></div>

        <div className="mx-auto flex w-full max-w-7xl flex-col gap-8 px-4 sm:px-6 lg:px-8 pt-8 pb-16 relative z-10">
          {/* Page Header */}
          <div className="space-y-6">
            <div className="space-y-4">
              <h1 className="text-4xl sm:text-5xl md:text-6xl font-bold text-white uppercase tracking-tight">
                Verify Holdings
              </h1>
              <p className="text-base sm:text-lg text-white/60">
                Privately verify your Zcash shielded balance using zero-knowledge proofs
              </p>
            </div>

            {/* Progress Steps */}
            <div className="space-y-4">
              <div className="flex items-center justify-between text-[0.65rem] uppercase tracking-[0.3em]">
                {STEPS.map((stepDef) => (
                  <StepIndicator
                    key={stepDef.id}
                    stepNumber={stepDef.id}
                    currentStep={step}
                    label={stepDef.label}
                    onStepClick={undefined}
                    isDevMode={false}
                  />
                ))}
              </div>
              <div className="relative h-2 bg-white/5 border border-white/10 overflow-hidden">
                <div
                  className="absolute left-0 top-0 h-full bg-gradient-to-r from-[#f3b724] to-[#f3b724]/80 transition-all duration-500 ease-out"
                  style={{ width: `${progressPercent}%` }}
                />
              </div>
            </div>
          </div>

          {/* Step Content */}
          <div className="flex flex-col gap-10">
            {step === 1 && (
              <FileUploadStep
                inputRef={inputRef}
                onDrop={handleDrop}
                onFileChange={(file) => void handleFile(file)}
                isWasmReady={isWasmReady}
                wasmInitError={wasmInitError}
                mode={mode}
                error={error}
                isProcessing={isProcessing}
              />
            )}

            {step === 2 && (
              <InspectStep
                mode={mode}
                isProcessing={isProcessing}
                error={error}
                analysis={analysis}
                hasWasmResult={hasWasmResult}
                proofQueued={proofQueued}
                isRequestingProof={isRequestingProof}
                uploadedFileName={uploadedFileName}
                placeholderShieldedAddress={placeholderShieldedAddress}
                placeholderShieldedBalance={placeholderShieldedBalance}
                snapshotHeightLabel={snapshotHeightLabel}
                targetHeightLabel={targetHeightLabel}
                balanceDisplay={balanceDisplay}
                eligible={eligible}
                shieldedBalanceDisplay={shieldedBalanceDisplay}
                processedInput={processedInput}
                snapshot={snapshot}
                onContinue={() => setStep(3)}
                onReset={resetWorkflow}
                onBack={() => setStep(1)}
              />
            )}

            {mode !== null && step === 3 && (
              <WalletConnectionStep
                connected={connected}
                publicKey={publicKey}
                addressCopied={addressCopied}
                isRequestingSignature={isRequestingSignature}
                signatureError={signatureError}
                signatureReceived={signatureReceived}
                onCopyAddress={handleCopyAddress}
                onDisconnect={handleDisconnect}
                onBack={() => setStep(2)}
                onContinue={() => setStep(4)}
                onRequestSignature={requestWalletSignature}
              />
            )}

            {mode !== null && step === 4 && (
              <SubmitStep
                proofQueued={proofQueued}
                isRequestingProof={isRequestingProof}
                connected={connected}
                snapshotHeightLabel={snapshotHeightLabel}
                targetHeightLabel={targetHeightLabel}
                onRequestProof={(proofMode) => { void handleRequestProof(proofMode) }}
                onBack={() => setStep(3)}
                formattedStatus={formattedStatus}
                statusIndicatorColor={statusIndicatorColor}
                statusMessage={statusMessage}
                statusError={statusError}
                proofQueueId={proofQueueId}
                submittedAtDisplay={submittedAtDisplay}
                statusUpdatedDisplay={statusUpdatedDisplay}
                statusData={statusData}
                isStatusLoading={isStatusLoading}
                onRefreshStatus={handleRefreshStatus}
                submittedProofMode={submittedProofMode}
                error={error}
                onReset={resetWorkflow}
              />
            )}
          </div>

          {/* FAQ is shown regardless of step/mode */}
          <FAQ />
        </div>
      </div>

      <Footer />
    </>
  )
}
