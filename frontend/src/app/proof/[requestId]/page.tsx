"use client"

export const runtime = 'edge'

import { Usable, use, useCallback, useEffect, useMemo, useRef, useState } from "react"
import { Navbar } from "../../../components/navbar"
import { Footer } from "../../../components/Footer"
import { InfoCard } from "../../../components/InfoCard"
import { cn } from "@/lib/utils"
import { domToPng } from 'modern-screenshot'

type ProofStatusResponse = {
  id?: string
  request_id?: string
  status?: string
  updated_at?: string
  created_at?: string
  message?: string
  balance?: number
  [key: string]: unknown
}

const PROOF_API_BASE =
  process.env.NEXT_PUBLIC_SERVER_BASE_URL?.replace(/\/+$/, "") ?? "http://localhost:3000"

interface ProofStatusPageProps {
  params: Promise<{
    requestId: string
  }>
}

export default function ProofStatusPage({ params }: ProofStatusPageProps) {
  const [status, setStatus] = useState<string>("Processing")
  const [details, setDetails] = useState<ProofStatusResponse | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [isDownloading, setIsDownloading] = useState(false)
  const [isSharing, setIsSharing] = useState(false)
  const [isCopying, setIsCopying] = useState(false)
  const [imageCopied, setImageCopied] = useState(false)
  const [sharePercentage, setSharePercentage] = useState<number>(100)
  const [showAmount, setShowAmount] = useState(true)
  const [verifiedBalance, setVerifiedBalance] = useState<number | null>(null)
  const [isCardHovered, setIsCardHovered] = useState(false)
  const [isCardFlashing, setIsCardFlashing] = useState(false)
  const cardRef = useRef<HTMLDivElement>(null)

  const requestId = use(params).requestId

  // Mock ZEC price - in production, fetch from an API
  const zecPriceUSD = 45.32

  // Calculate share amount based on verified balance and percentage (rounded to 1 sig fig)
  const shareAmount1 = verifiedBalance ? verifiedBalance * sharePercentage / 100 : 0;
  function toOneSigFig(n: number): number {
    if (n === 0) return 0;
    const power = Math.floor(Math.log10(Math.abs(n)));
    const factor = Math.pow(10, power);
    return Math.round(n / factor) * factor;
  }
  const shareAmount = toOneSigFig(shareAmount1);

  // Calculate appropriate number of decimals for display based on the share amount
  const getDecimalsForAmount = (amount: number): number => {
    if (amount === 0) return 0;
    if (amount >= 10) return 0;
    if (amount >= 1) return 1;
    // For amounts < 1, show enough decimals to see the value
    const power = Math.floor(Math.log10(Math.abs(amount)));
    return Math.max(0, -power + 1); // Show 1-2 significant figures
  };

  const fetchStatus = useCallback(async () => {
    setError(null)
    try {
      const response = await fetch(`${PROOF_API_BASE}/proof/${requestId}`, {
        method: "GET",
        headers: {
          "Content-Type": "application/json",
        },
        cache: "no-store",
      })

      if (!response.ok) {
        throw new Error(`Proof status fetch failed: ${response.status} ${response.statusText}`)
      }

      const body = (await response.json()) as ProofStatusResponse
      setDetails(body)
      
      // Normalize status from API response
      // Handle all possible FulfillmentStatus values: Requested, Assigned, Fulfilled, Unfulfillable, UnspecifiedFulfillmentStatus
      const rawStatus = typeof body.status === "string" 
        ? body.status 
        : typeof body["state"] === "string" 
          ? body["state"] as string
          : null
      
      console.log("Proof status API response:", { rawStatus, body })
      
      let derivedStatus = "Unknown"
      if (rawStatus) {
        const normalized = rawStatus.toLowerCase()
        if (normalized === "assigned") {
          derivedStatus = "Processing"
        } else if (normalized === "fulfilled") {
          derivedStatus = "Processed"
        } else if (normalized === "requested") {
          derivedStatus = "Pending"
        } else if (normalized === "unfulfillable") {
          derivedStatus = "Failed"
        } else if (normalized === "unknown") {
          derivedStatus = "Unknown"
        } else {
          // For any other status, use the raw value (capitalize first letter of each word)
          derivedStatus = rawStatus
            .replace(/[_-]+/g, " ")
            .trim()
            .split(" ")
            .map((word) => word ? word[0].toUpperCase() + word.slice(1).toLowerCase() : "")
            .join(" ")
        }
      }
      
      console.log("Derived status:", derivedStatus)
      setStatus(derivedStatus)

      // Extract verified balance from response - use balance field as primary source
      // Only update if balance is present in response (preserve existing value if not)
      const balance = body["balance"]
      if (typeof balance === "number") {
        setVerifiedBalance(balance / 100000000)
      } else if (typeof balance === "string") {
        const parsed = parseFloat(balance)
        if (!isNaN(parsed)) {
          setVerifiedBalance(parsed / 100000000)
        }
      }
      // Don't set to null if balance is missing - preserve existing value
      // Balance might not be available during processing but can appear later
    } catch (err) {
      console.error("Failed to fetch proof status", err)
      setError(err instanceof Error ? err.message : "Failed to fetch proof status")
    }
  }, [requestId])

  useEffect(() => {
    void fetchStatus()

    // Only poll if status is not in a terminal state
    const isTerminalState =
      status === "Processed" ||
      status === "Completed" ||
      status === "Success" ||
      status === "Unknown"

    if (isTerminalState) {
      return // Don't set up polling for terminal states
    }

    const interval = setInterval(() => {
      void fetchStatus()
    }, 10_000)
    return () => clearInterval(interval)
  }, [fetchStatus, status])

  const updatedDisplay = useMemo(() => {
    const timestamp = details?.updated_at ?? details?.created_at
    if (timestamp === undefined || timestamp === null) return null
    
    let date: Date
    if (typeof timestamp === "number") {
      // If it's a number, check if it's in seconds (less than year 2000) or milliseconds
      // Unix timestamps in seconds are typically < 1e12, milliseconds are >= 1e12
      date = timestamp < 1e12 
        ? new Date(timestamp * 1000) // Convert seconds to milliseconds
        : new Date(timestamp) // Already in milliseconds
    } else if (typeof timestamp === "string") {
      date = new Date(timestamp)
    } else {
      return String(timestamp)
    }
    
    // Check if date is valid and reasonable (after year 2000)
    if (Number.isNaN(date.getTime()) || date.getFullYear() < 2000) {
      return null // Return null for invalid or unreasonably old dates
    }
    
    return date.toLocaleString()
  }, [details])

  const message = useMemo(() => {
    if (details?.message && typeof details.message === "string") {
      return details.message
    }
    switch (status.toLowerCase()) {
      case "processing":
        return "Your request is in queue to be processed by the TEE Prover. This may take some time depending on compute availability. You can safely close this page and check back later."
      case "processed":
        return "Proof generation finished successfully. Holdings have been successfully verified. The proof does not reveal any shielded information."
      case "unknown":
        return "The prover encountered an error. Please try again or contact support."
      default:
        return null
    }
  }, [details, status])

  function formatRequestId(requestId: string) {
    return `${requestId.slice(0, 4)}...${requestId.slice(-4)}`
  }

  const handleDownloadCard = useCallback(async () => {
    if (!cardRef.current) return

    setIsDownloading(true)
    try {
      const dataUrl = await domToPng(cardRef.current, {
        scale: 2,
        quality: 1,
        backgroundColor: 'transparent',
      })

      // Download the image
      const link = document.createElement('a')
      link.href = dataUrl
      link.download = `zfun-verification-${requestId.slice(0, 8)}.png`
      link.click()
    } catch (err) {
      console.error('Failed to download card:', err)
    } finally {
      setIsDownloading(false)
    }
  }, [requestId])

  const handleShareToTwitter = useCallback(async () => {
    if (!cardRef.current) return

    setIsSharing(true)
    setImageCopied(true)
    setIsCardFlashing(true)

    try {
      // Generate the image
      const dataUrl = await domToPng(cardRef.current, {
        scale: 2,
        quality: 1,
        backgroundColor: 'transparent',
      })

      // Convert data URL to blob
      const response = await fetch(dataUrl)
      const blob = await response.blob()

      // Copy image to clipboard
      await navigator.clipboard.write([
        new ClipboardItem({
          'image/png': blob
        })
      ])

      // Reset flash immediately
      setTimeout(() => setIsCardFlashing(false), 20)

      // Open Twitter intent
      const tweetText = encodeURIComponent(`I just verified my Zcash balance using @zdotfun 🔐\n\nProof: z.fun/verify`)
      window.open(`https://twitter.com/intent/tweet?text=${tweetText}`, '_blank')

      // Reset the copied state after 3 seconds
      setTimeout(() => setImageCopied(false), 3000)
    } catch (err) {
      console.error('Failed to share:', err)
      setImageCopied(false)
      setIsCardFlashing(false)
    } finally {
      setIsSharing(false)
    }
  }, [])

  const handleCopyCard = useCallback(async () => {
    if (!cardRef.current) return

    setIsCopying(true)
    setImageCopied(true)
    setIsCardFlashing(true)

    try {
      // Generate the image
      const dataUrl = await domToPng(cardRef.current, {
        scale: 2,
        quality: 1,
        backgroundColor: 'transparent',
      })

      // Convert data URL to blob
      const response = await fetch(dataUrl)
      const blob = await response.blob()

      // Copy image to clipboard
      await navigator.clipboard.write([
        new ClipboardItem({
          'image/png': blob
        })
      ])

      // Reset flash immediately
      setTimeout(() => setIsCardFlashing(false), 30)

      // Reset the copied state after 3 seconds
      setTimeout(() => setImageCopied(false), 3000)
    } catch (err) {
      console.error('Failed to copy:', err)
      setImageCopied(false)
      setIsCardFlashing(false)
    } finally {
      setIsCopying(false)
    }
  }, [])

  const isSuccess = status === "Processed" || status === "Completed" || status === "Success"
  // const isDevMode = process.env.NODE_ENV !== "production"
  const isDevMode = false

  console.log('status', status)

  // Calculate decimals based on shareAmount (what's actually displayed), not verifiedBalance
  const maxDecimals = getDecimalsForAmount(shareAmount)

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

        <div className="mx-auto flex min-h-screen w-full max-w-7xl flex-col gap-4 px-4 sm:px-6 lg:px-8 pt-8 pb-16 relative z-10">
          {/* Page Header */}
          <div className="space-y-4">
            <div>
              <h1 className="text-4xl sm:text-5xl md:text-6xl font-bold text-white uppercase tracking-tight mb-4">
                Proof Status
              </h1>
              <p className="text-base sm:text-lg text-white/60">
                Track your proof generation progress in real-time
              </p>
            </div>
            <div className="flex flex-col sm:flex-row items-start sm:items-center gap-2 text-sm">
              <div className="flex items-center gap-2 max-w-full">
                <span className="text-white/40 whitespace-nowrap">Request ID:</span>
                <a
                  target="_blank"
                  rel="noreferrer"
                  href={`https://explorer.monero-chan.org/proof/${requestId.startsWith('0x') ? requestId : '0x' + requestId}`}
                  className="font-mono text-[#f3b724] hover:text-[#ffd700] underline underline-offset-4 transition-colors break-all"
                >
                  {requestId}
                </a>
              </div>
              <button
                type="button"
                onClick={() => navigator.clipboard?.writeText(requestId)}
                className="group relative inline-flex items-center justify-center p-2 text-white/60 hover:text-white transition-colors duration-200"
                aria-label="Copy Request ID"
              >
                <svg
                  className="w-4 h-4 transition-transform group-hover:scale-110"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                    d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"
                  />
                </svg>
              </button>
            </div>
          </div>

          {/* Card Content */}

          {/* Status Section */}
          <div className="space-y-8">
            <InfoCard
              headerContent={
                <>
                  <div>
                    <h3 className="text-xs text-white/40 uppercase tracking-wider mb-2">Proof Status</h3>
                    <p className={cn(
                      "text-xl font-medium uppercase tracking-wide",
                      status === "Processed" ? "text-green-400" : "text-white"
                    )}>{status}</p>
                  </div>
                  {message && (
                    <p className="text-xs text-white/40 mt-2">
                      {message}
                    </p>
                  )}
                </>
              }
            >
              {updatedDisplay && (
                <div className="flex items-center gap-2 text-xs text-white/40 mb-6">
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                  </svg>
                  <span>Last updated: <span className="text-white/60">{updatedDisplay}</span></span>
                </div>
              )}

              {error && (
                <div className="border border-red-500/30 bg-red-500/5 p-4 mb-6">
                  <div className="flex items-start gap-3">
                    <svg className="w-5 h-5 text-red-400 flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                    </svg>
                    <p className="text-sm text-red-300">{error}</p>
                  </div>
                </div>
              )}

              {details && (
                <div className="space-y-3">
                  <p className="text-xs uppercase tracking-[0.2em] text-white/40 font-medium mb-4">Additional Details</p>
                  {typeof details.proof_url === "string" && details.proof_url && (
                    <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between">
                      <span className="text-xs uppercase tracking-[0.2em] text-white/30">Proof URL</span>
                      <a
                        target="_blank"
                        rel="noreferrer"
                        href={details.proof_url}
                        className="inline-flex items-center gap-2 text-[#f3b724] hover:text-[#ffd700] underline underline-offset-4 transition-colors font-mono text-sm break-all"
                      >
                        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                        </svg>
                        Download Proof
                      </a>
                    </div>
                  )}
                  <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between">
                    <span className="text-xs uppercase tracking-[0.2em] text-white/30">Explorer</span>
                    <a
                      href={`https://explorer.monero-chan.org/proof/${requestId.startsWith('0x') ? requestId : '0x' + requestId}`}
                      target="_blank"
                      rel="noreferrer"
                      className="text-[#f3b724] hover:text-[#ffd700] underline underline-offset-4 break-all transition-colors text-sm"
                    >
                      View in Explorer →
                    </a>
                  </div>
                  {details.is_tee !== undefined && (
                    <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between">
                      <span className="text-xs uppercase tracking-[0.2em] text-white/30">PROVER MODE</span>
                      <span className="font-mono text-sm text-white/70">
                        {details.is_tee === true || details.is_tee === "true" ? "ZK + TEE" : "ZK"}
                      </span>
                    </div>
                  )}
                  {details.created_at !== undefined && (
                    <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between">
                      <span className="text-xs uppercase tracking-[0.2em] text-white/30">Created At</span>
                      <span className="font-mono text-sm text-white/70">
                        {typeof details.created_at === "number"
                          ? new Date(details.created_at * 1000).toLocaleString()
                          : typeof details.created_at === "string"
                            ? new Date(details.created_at).toLocaleString()
                            : String(details.created_at)}
                      </span>
                    </div>
                  )}
                  {details.fulfilled_at !== undefined && details.fulfilled_at !== null && (
                    <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between">
                      <span className="text-xs uppercase tracking-[0.2em] text-white/30">Fulfilled At</span>
                      <span className="font-mono text-sm text-white/70">
                        {typeof details.fulfilled_at === "number"
                          ? new Date(details.fulfilled_at * 1000).toLocaleString()
                          : typeof details.fulfilled_at === "string"
                            ? new Date(details.fulfilled_at).toLocaleString()
                            : String(details.fulfilled_at)}
                      </span>
                    </div>
                  )}
                  {details.balance !== undefined && details.balance !== null && (
                    <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between">
                      <span className="text-xs uppercase tracking-[0.2em] text-white/30">Balance</span>
                      <span className="font-mono text-sm text-white/70">
                        {typeof details.balance === "number"
                          ? (details.balance / 100000000).toLocaleString(undefined, { minimumFractionDigits: 0, maximumFractionDigits: 6 })
                          : String((details.balance ?? 0) / 100000000)} ZEC
                      </span>
                    </div>
                  )}
                </div>
              )}
            </InfoCard>
          </div>

          {/* Shareable Card - Shown when balance is available */}
          {verifiedBalance !== null && (
            <div className="space-y-8">
              <section className="space-y-6">
                {/* Card Preview Section */}
                <InfoCard
                  title="Share Verification Card"
                  description="Click the card to copy it as an image, or use the buttons below to share."
                >
                  <div className="flex flex-col lg:flex-row gap-6 items-start">
                    {/* Shareable Card Preview */}
                    <div className="relative group flex-shrink-0">
                      <div
                        ref={cardRef}
                        className="w-[500px] max-w-full aspect-square relative overflow-hidden cursor-pointer transition-all duration-200"
                        style={{
                          background: 'linear-gradient(135deg, #121212 0%, #1a1a1a 100%)',
                        }}
                        onMouseEnter={() => setIsCardHovered(true)}
                        onMouseLeave={() => setIsCardHovered(false)}
                        onClick={handleCopyCard}
                      >
                        {/* Darken overlay on hover/click */}
                        <div
                          className={cn(
                            "absolute inset-0 pointer-events-none transition-all duration-200 z-10",
                            isCardFlashing ? "bg-black/10" : isCardHovered ? "bg-black/30" : "bg-black/0"
                          )}
                          style={{
                            transitionDuration: isCardFlashing ? "5ms" : "200ms"
                          }}
                        />
                        {/* Card Background */}
                        <div className="absolute inset-0">
                          {/* Large blurred Z logo in right center */}
                          <div className="absolute -right-30 top-1/2 -translate-y-1/2 w-[500px] h-[450px] opacity-75">
                            {/* Top half - less blur */}
                            <img
                              src="/zlogo-circ.svg"
                              alt=""
                              className="absolute inset-0 w-full h-full"
                              style={{
                                filter: 'blur(3px)',
                                maskImage: 'linear-gradient(to bottom, rgba(0,0,0,0.2) 0%, rgba(0,0,0,0.2) 40%, rgba(0,0,0,0) 60%)',
                                WebkitMaskImage: 'linear-gradient(to bottom, rgba(0,0,0,0.2) 0%, rgba(0,0,0,0.2) 40%, rgba(0,0,0,0) 60%)'
                              }}
                            />
                            {/* Bottom half - more blur */}
                            <img
                              src="/zlogo-circ.svg"
                              alt=""
                              className="absolute inset-0 w-full h-full"
                              style={{
                                filter: 'blur(4px)',
                                maskImage: 'linear-gradient(to bottom, rgba(0,0,0,0) 40%, rgba(0,0,0,0.1) 60%, rgba(0,0,0,0.02) 100%)',
                                WebkitMaskImage: 'linear-gradient(to bottom, rgba(0,0,0,0) 40%, rgba(0,0,0,0.1) 60%, rgba(0,0,0,0.02) 100%)'
                              }}
                            />
                          </div>
                          <div className="absolute top-0 right-0 w-64 h-64 bg-white/10 rounded-full blur-3xl"></div>
                          <div className="absolute bottom-0 left-0 w-48 h-48 bg-white/10 rounded-full blur-3xl"></div>
                        </div>

                        {/* Card Content */}
                        <div className="relative h-full flex flex-col justify-between p-4 sm:p-6 md:p-8">
                          {/* Header */}
                          <div className="flex items-center justify-between">
                            <div className="flex items-center gap-2 sm:gap-3">
                              <img
                                src="/ZLOGO-SVG.svg"
                                alt="Z.FUN"
                                className="h-6 sm:h-8 md:h-10 w-auto"
                              />
                              <div>
                                <p className="text-white font-bold text-sm sm:text-base md:text-lg">Z.FUN</p>
                                <p className="text-white/60 text-[10px] sm:text-xs uppercase tracking-wider">Verification</p>
                              </div>
                            </div>
                          </div>

                          {/* Main Content */}
                          <div className="space-y-2">
                            <div className="space-y-3">
                              <div>
                                <p className="text-5xl leading-[1.1] tracking-tight">
                                  <span className="text-white/60">I verified my </span>
                                  <span className="text-white/80 font-medium">Zcash</span>
                                  <span className="text-white/60"> shielded balance</span>
                                </p>
                              </div>
                              <div className="inline-flex items-baseline gap-3 px-4 py-2.5 bg-[#f3b724]/10 border border-[#f3b724]/30 max-w-full">
                                <span className="text-7xl font-black text-[#f3b724] leading-none tracking-tight whitespace-nowrap">
                                  {sharePercentage < 100 ? '≥' : ''} {showAmount ? shareAmount.toLocaleString(undefined, { minimumFractionDigits: 0, maximumFractionDigits: maxDecimals }) : '***'}
                                </span>
                                <span className="text-xl text-[#f3b724]/60 font-bold uppercase tracking-wider leading-none whitespace-nowrap">ZEC</span>
                              </div>
                            </div>
                          </div>

                          {/* Footer */}
                          <div className="flex items-center justify-between">
                            <div className="flex flex-col gap-1 max-w-[65%] pr-2">
                              <span className="text-white/40 text-xs sm:text-sm uppercase tracking-wider">Request ID</span>
                              <span className="text-white/60 text-xs sm:text-sm font-mono break-all">
                                {showAmount && sharePercentage === 100 ? requestId :
                                  '[REDACTED]'
                                  // '█'.repeat(requestId.length)
                                }
                              </span>
                            </div>
                            <p className="text-white/40 text-lg sm:text-xl whitespace-nowrap">z.fun/verify</p>
                          </div>
                        </div>
                      </div>

                      {/* Tooltip on the right */}
                      <div
                        className={cn(
                          "absolute -right-2 top-1/2 -translate-y-1/2 translate-x-full ml-4 px-3 py-2 bg-white text-black text-xs font-medium rounded shadow-lg whitespace-nowrap pointer-events-none transition-all duration-200",
                          isCardHovered || isCopying || imageCopied ? "opacity-100 translate-x-full" : "opacity-0 translate-x-[calc(100%-8px)]"
                        )}
                      >
                        <div className="flex items-center gap-2">
                          {isCopying ? (
                            <>
                              <div className="w-3 h-3 border-2 border-black/20 border-t-black rounded-full animate-spin"></div>
                              <span>Copying...</span>
                            </>
                          ) : imageCopied ? (
                            <>
                              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
                              </svg>
                              <span>Copied!</span>
                            </>
                          ) : (
                            <>
                              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
                              </svg>
                              <span>Click to copy</span>
                            </>
                          )}
                        </div>
                        {/* Arrow pointing left */}
                        <div className="absolute -left-1 top-1/2 -translate-y-1/2 w-2 h-2 bg-white rotate-45"></div>
                      </div>
                    </div>

                    {/* Right Side: Balance, Customization & Actions */}
                    <div className="flex-1 space-y-6">
                      {/* Verified Balance & Customize Section */}
                      <div className="grid gap-6 sm:grid-cols-2">
                        {/* Verified Balance */}
                        <div className="space-y-3">
                          <div className="flex items-center gap-2">
                            <svg className="w-4 h-4 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
                            </svg>
                            <p className="text-xs uppercase tracking-[0.2em] text-white/40 font-medium">Verified Balance</p>
                          </div>
                          <div className="bg-black/40 border border-white/10 px-4 py-3.5">
                            <div className="flex items-baseline gap-2">
                              <span className="text-2xl font-bold text-white/90">{verifiedBalance.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 3 })}</span>
                              <span className="text-xs uppercase tracking-wider text-white/80">ZEC</span>
                            </div>
                          </div>
                        </div>

                        {/* Customize Share */}
                        <div className="space-y-3">
                          <div className="flex items-center gap-2">
                            <svg className="w-4 h-4 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4" />
                            </svg>
                            <p className="text-xs uppercase tracking-[0.2em] text-white/40 font-medium">Customize Share</p>
                          </div>
                          <div className="bg-black/40 border border-white/10 px-4 py-3.5">
                            <div className="flex items-baseline gap-3">
                              <div className="flex items-baseline gap-2">
                                <span className="text-2xl font-bold text-white/90">{sharePercentage}%</span>
                                <span className="text-xs text-white/60">of balance</span>
                              </div>
                              <div className="flex items-baseline gap-1.5 ml-auto">
                                <span className="text-lg font-bold text-white">{shareAmount.toLocaleString(undefined, { minimumFractionDigits: 1, maximumFractionDigits: 1 })}</span>
                                <span className="text-xs text-white/50 uppercase">ZEC</span>
                              </div>
                            </div>
                          </div>
                        </div>
                      </div>

                      {/* Slider Control */}
                      <div className="space-y-3">
                        <div className="px-1">
                          <input
                            type="range"
                            min="1"
                            max="100"
                            value={sharePercentage}
                            onChange={(e) => setSharePercentage(parseInt(e.target.value))}
                            className="custom-slider w-full h-1.5 bg-white/10 rounded appearance-none cursor-pointer"
                            style={{
                              background: `linear-gradient(to right, rgba(243, 183, 36, 0.3) 0%, rgba(243, 183, 36, 0.3) ${sharePercentage}%, rgb(255 255 255 / 0.1) ${sharePercentage}%, rgb(255 255 255 / 0.1) 100%)`
                            }}
                          />
                        </div>

                        {/* Visibility Toggle */}
                        <button
                          onClick={() => setShowAmount(!showAmount)}
                          className={cn(
                            "w-full px-3 py-2 text-xs uppercase tracking-wide transition-all duration-200 cursor-pointer",
                            showAmount
                              ? "bg-[#f3b724]/20 border border-[#f3b724]/50 text-[#f3b724] hover:bg-[#f3b724]/30"
                              : "bg-white/5 border border-white/10 text-white/60 hover:bg-white/10"
                          )}
                        >
                          {showAmount ? "✓ Visible" : "Hidden"}
                        </button>
                      </div>

                      {/* Download & Share Actions */}
                      <div className="flex items-center gap-2">
                        <button
                          onClick={handleShareToTwitter}
                          disabled={isSharing}
                          className="group relative inline-flex items-center justify-center gap-2 px-3 py-2.5 border border-white/20 text-white/60 hover:text-white transition-colors duration-200 disabled:opacity-30 disabled:cursor-not-allowed"
                        >
                          {isSharing ? (
                            <>
                              <div className="w-4 h-4 border-2 border-white/20 border-t-white rounded-full animate-spin"></div>
                              <span className="text-sm uppercase tracking-wider">Share on X</span>
                            </>
                          ) : (
                            <>
                              <svg className="w-4 h-4 transition-transform group-hover:scale-110" fill="currentColor" viewBox="0 0 24 24">
                                <path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z" />
                              </svg>
                              <span className="text-sm uppercase tracking-wider">Share on X</span>
                            </>
                          )}
                        </button>

                        <button
                          onClick={handleCopyCard}
                          disabled={isCopying}
                          className="group relative inline-flex items-center justify-center p-2.5 border border-white/20 text-white/60 hover:text-white transition-colors duration-200 disabled:opacity-30 disabled:cursor-not-allowed"
                          aria-label="Copy Card"
                        >
                          {isCopying ? (
                            <div className="w-4 h-4 border-2 border-white/20 border-t-white rounded-full animate-spin"></div>
                          ) : (
                            <svg className="w-4 h-4 transition-transform group-hover:scale-110" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
                            </svg>
                          )}
                        </button>

                        <button
                          onClick={handleDownloadCard}
                          disabled={isDownloading}
                          className="group relative inline-flex items-center justify-center p-2.5 border border-white/20 text-white/60 hover:text-white transition-colors duration-200 disabled:opacity-30 disabled:cursor-not-allowed"
                          aria-label="Download Card"
                        >
                          {isDownloading ? (
                            <div className="w-4 h-4 border-2 border-white/20 border-t-white rounded-full animate-spin"></div>
                          ) : (
                            <svg className="w-4 h-4 transition-transform group-hover:scale-110" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                            </svg>
                          )}
                        </button>
                      </div>
                    </div>
                  </div>
                </InfoCard>
              </section>
            </div>
          )
          }

        </div >
        <Footer />
      </div >
    </>
  )
}
