"use client"

import type { Analysis } from "@/lib/types"
import { InfoCard } from "../../../components/InfoCard"

type WasmShieldedSnapshot = {
  height: number
  accounts: unknown[]
}

type WasmProcessedSnapshot = {
  total_zatoshis: number
  alternate_nullifiers: unknown[]
}

interface InspectStepProps {
  mode: "shielded"
  isProcessing: boolean
  error: string | null
  analysis: Analysis | null
  hasWasmResult: boolean
  proofQueued: boolean
  isRequestingProof: boolean
  uploadedFileName: string | null
  placeholderShieldedAddress: string
  placeholderShieldedBalance: string
  snapshotHeightLabel: string | null
  targetHeightLabel: string | null
  balanceDisplay: string | null
  eligible: boolean
  shieldedBalanceDisplay: string | null
  processedInput: WasmProcessedSnapshot | null
  snapshot: WasmShieldedSnapshot | null
  onContinue: () => void
  onReset: () => void
  onBack: () => void
}

export function InspectStep({
  mode,
  isProcessing,
  error,
  analysis,
  hasWasmResult,
  proofQueued,
  isRequestingProof,
  uploadedFileName,
  placeholderShieldedAddress,
  placeholderShieldedBalance,
  snapshotHeightLabel,
  targetHeightLabel,
  balanceDisplay,
  eligible,
  shieldedBalanceDisplay,
  processedInput,
  snapshot,
  onContinue,
  onReset,
  onBack,
}: InspectStepProps) {
  return (
    <section className="space-y-6">
      <div className="space-y-6">
        {mode === "shielded" && !isProcessing && !error && !analysis && (
          <div className="space-y-6">
            {/* Wallet Analysis Results */}
            <InfoCard
              title="Wallet Analysis Results"
              description="Your wallet export has been processed locally and prepared for TEE proof generation. Review the data below before submitting."
            >
              <div className="grid gap-6 sm:grid-cols-2">
                {/* Shielded Balance */}
                <div className="space-y-3">
                  <div className="flex items-center gap-2">

                    <p className="text-xs uppercase tracking-[0.2em] text-white/40 font-medium">Shielded Balance</p>
                  </div>
                  <div className="bg-black/40 border border-white/10 px-4 py-3.5">
                    <div className="flex items-baseline gap-2">
                      <span className="text-2xl font-bold text-white/90">{hasWasmResult ? (shieldedBalanceDisplay ?? "0") : placeholderShieldedBalance}</span>
                      <span className="text-xs uppercase tracking-wider text-white/40">ZEC</span>
                    </div>
                    {hasWasmResult && processedInput && (
                      <p className="text-xs text-white/40 mt-2">
                        {processedInput.total_zatoshis.toLocaleString()} zatoshis
                      </p>
                    )}
                  </div>
                </div>

                {/* Snapshot Height */}
                <div className="space-y-3">
                  <div className="flex items-center gap-2">

                    <p className="text-xs uppercase tracking-[0.2em] text-white/40 font-medium">Snapshot Height</p>
                  </div>
                  <div className="bg-black/40 border border-white/10 px-4 py-3.5">
                    <div className="flex items-baseline gap-2">
                      <span className="text-2xl font-bold text-white/90 font-mono">
                        {hasWasmResult
                          ? (snapshotHeightLabel ?? targetHeightLabel ?? <span className="animate-pulse text-white/50">Loading...</span>)
                          : (targetHeightLabel ?? <span className="animate-pulse text-white/50">Loading...</span>)
                        }
                      </span>
                    </div>
                    <p className="text-xs text-white/40 mt-2">
                      {hasWasmResult
                        ? (
                          <>
                            Target reference: block {targetHeightLabel ?? <span className="animate-pulse">loading...</span>}
                          </>
                        )
                        : "Target reference block"
                      }
                    </p>
                  </div>
                </div>
              </div>

              {/* Info Message */}
              {(!proofQueued && !isRequestingProof) && (
                <div className={`${hasWasmResult ? 'mt-6' : 'mt-4'} flex items-start gap-3 px-4 py-3 bg-white/5 border border-white/10`}>
                  <div className="flex-1">
                    <p className="text-sm text-white/70 leading-relaxed">
                      When you request a proof, the derived input is securely sent directly to the TEE prover enclave where it is decrypted and used for proof generation.
                      <br />
                      The only information visible publicly is the total amount of ZEC proven, rounded to 2 decimal places for increased privacy.
                    </p>
                  </div>
                </div>
              )}
            </InfoCard>


            {!proofQueued && !isRequestingProof && (
              <div className="flex items-center gap-3">
                <button
                  type="button"
                  onClick={onBack}
                  className="px-4 py-2.5 text-xs font-medium uppercase tracking-[0.2em] text-white/60 hover:text-white transition-colors duration-200"
                >
                  Back
                </button>
                <button
                  type="button"
                  onClick={onContinue}
                  className="flex-1 inline-flex items-center justify-center gap-2 px-4 py-2.5 text-xs font-medium uppercase tracking-[0.2em] transition-all duration-200 border border-[#f3b724]/50 text-[#f3b724] bg-[#f3b724]/10 hover:bg-[#f3b724]/20 hover:shadow-[0_0_20px_rgba(243,183,36,0.3)]"
                >
                  Continue to Wallet
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 7l5 5m0 0l-5 5m5-5H6" />
                  </svg>
                </button>
              </div>
            )}
          </div>
        )}

        {mode === "shielded" && isProcessing && (
          <div className="flex h-40 flex-col items-center justify-center gap-4 border border-white/10 bg-white/[0.02] backdrop-blur-sm">
            <div className="relative">
              <div className="h-12 w-12 animate-spin rounded-full border-2 border-white/10 border-t-white" />
              <div className="absolute inset-0 h-12 w-12 animate-ping rounded-full border-2 border-white/20" />
            </div>
            <div className="text-center">
              <p className="text-sm font-medium text-white mb-1">Scanning Locally</p>
              <p className="text-xs text-white/60">Processing your database...</p>
            </div>
          </div>
        )}

        {mode === "shielded" && !isProcessing && error && (
          <div className="flex items-start gap-3 border border-red-500/30 bg-red-500/10 backdrop-blur-sm p-5">
            <svg className="w-5 h-5 text-red-400 flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
            <div>
              <p className="text-xs font-medium text-red-300 mb-2 uppercase tracking-[0.15em]">Error</p>
              <p className="text-sm text-red-200">{error}</p>
            </div>
          </div>
        )}

        {mode === "shielded" && !isProcessing && !error && analysis && (
          <div className="space-y-6">
            {/* Detected Balance */}
            <div className="border border-white/10 bg-white/[0.02] backdrop-blur-sm overflow-hidden">
              <div className="flex flex-col">
                {/* Header Section */}
                <div className="p-4">
                  <h3 className="text-sm font-bold uppercase tracking-[0.15em] text-white">Detected Balance</h3>
                  <p className="text-xs text-white/40 mt-2">Analysis of your wallet export file.</p>
                </div>

                {/* Divider */}
                <div className="h-px bg-gradient-to-r from-white/10 to-transparent"></div>

                {/* Content Section */}
                <div className="p-4 space-y-6">
                  {/* Balance Display */}
                  <div className="space-y-3">
                    <div className="flex items-center gap-2">
                      <svg className="w-4 h-4 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8c-1.657 0-3 .895-3 2s1.343 2 3 2 3 .895 3 2-1.343 2-3 2m0-8c1.11 0 2.08.402 2.599 1M12 8V7m0 1v8m0 0v1m0-1c-1.11 0-2.08-.402-2.599-1M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                      </svg>
                      <p className="text-xs uppercase tracking-[0.2em] text-white/40 font-medium">Shielded Balance</p>
                    </div>
                    <div className="bg-black/40 border border-white/10 px-6 py-5">
                      <div className="flex items-baseline gap-3">
                        <span className="text-5xl md:text-6xl font-bold text-white tracking-tight">{balanceDisplay ?? "0"}</span>
                        <span className={`text-lg uppercase tracking-[0.15em] ${eligible ? "text-emerald-400" : "text-white/60"}`}>ZEC</span>
                      </div>
                    </div>
                  </div>

                  {/* Status Message */}
                  <div className={`flex items-start gap-3 px-4 py-3 border ${eligible ? "border-emerald-500/30 bg-emerald-500/10" : "border-white/10 bg-white/5"}`}>
                    <svg className={`w-5 h-5 flex-shrink-0 mt-0.5 ${eligible ? "text-emerald-400" : "text-white/60"}`} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      {eligible ? (
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
                      ) : (
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                      )}
                    </svg>
                    <div className="flex-1">
                      <p className={`text-sm ${eligible ? "text-emerald-300" : "text-white/80"}`}>
                        {eligible
                          ? "Shielded balance detected. Move ahead with your claim."
                          : "No shielded balance detected. You can still register your interest."}
                      </p>
                    </div>
                  </div>
                </div>
              </div>
            </div>

            {/* Signal Breakdown */}
            <InfoCard
              title="Signal Breakdown"
              description="Detailed analysis of detected signals in your wallet."
            >
              {analysis.matches.length ? (
                <ul className="space-y-2">
                  {analysis.matches.map((match, index) => (
                    <li
                      key={`${match.path}-${index}`}
                      className="border border-white/10 bg-white/[0.02] hover:bg-[#f3b724]/5 hover:border-[#f3b724]/20 transition-colors duration-200 px-5 py-4"
                    >
                      <div className="flex items-center justify-between gap-4">
                        <span className="font-mono text-xs text-white/60 truncate">{match.path || match.rawKey}</span>
                        <span className="font-semibold text-white whitespace-nowrap">{match.value}</span>
                      </div>
                    </li>
                  ))}
                </ul>
              ) : (
                <div className="flex items-start gap-3 px-4 py-3 bg-white/5 border border-white/10">
                  <svg className="w-5 h-5 text-white/60 flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4" />
                  </svg>
                  <div className="flex-1">
                    <p className="text-sm text-white/60">No shielded-looking fields were found.</p>
                  </div>
                </div>
              )}
            </InfoCard>
          </div>
        )}

        {mode === "shielded" && !isProcessing && !error && analysis && !proofQueued && !isRequestingProof && (
          <div className="flex items-center gap-3">
            <button
              type="button"
              onClick={onBack}
              className="px-4 py-2.5 text-xs font-medium uppercase tracking-[0.2em] text-white/60 hover:text-white transition-colors duration-200"
            >
              Back
            </button>
            <button
              type="button"
              onClick={onContinue}
              className="flex-1 inline-flex items-center justify-center gap-2 px-4 py-2.5 text-xs font-medium uppercase tracking-[0.2em] transition-all duration-200 border border-[#f3b724]/50 text-[#f3b724] bg-[#f3b724]/10 hover:bg-[#f3b724]/20 hover:shadow-[0_0_20px_rgba(243,183,36,0.3)]"
            >
              Continue to Wallet
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 7l5 5m0 0l-5 5m5-5H6" />
              </svg>
            </button>
          </div>
        )}

      </div>
    </section>
  )
}

