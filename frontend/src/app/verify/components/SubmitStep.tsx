"use client"

import { useState } from "react"
import { CurrentStatus } from "./CurrentStatus"
import { InfoCard } from "../../../components/InfoCard"

type ProofStatusResponse = {
  status?: string
  proof_url?: string
  [key: string]: unknown
}

export type ProofMode = "zk-tee" | "zk-only" | null

interface SubmitStepProps {
  proofQueued: boolean
  isRequestingProof: boolean
  connected: boolean
  snapshotHeightLabel: string | null
  targetHeightLabel: string | null
  onRequestProof: (mode: ProofMode) => void
  onBack: () => void
  formattedStatus: string
  statusIndicatorColor: string
  statusMessage: string
  statusError: string | null
  proofQueueId: string | null
  submittedAtDisplay: string | null
  statusUpdatedDisplay: string | null
  statusData: ProofStatusResponse | null
  isStatusLoading: boolean
  onRefreshStatus: () => void
  submittedProofMode?: ProofMode
  error?: string | null
  onReset?: () => void
}

export function SubmitStep({
  proofQueued,
  isRequestingProof,
  connected,
  snapshotHeightLabel,
  targetHeightLabel,
  onRequestProof,
  onBack,
  formattedStatus,
  statusIndicatorColor,
  statusMessage,
  statusError,
  proofQueueId,
  submittedAtDisplay,
  statusUpdatedDisplay,
  statusData,
  isStatusLoading,
  onRefreshStatus,
  submittedProofMode,
  error,
  onReset,
}: SubmitStepProps) {
  const [selectedMode, setSelectedMode] = useState<ProofMode>("zk-only")

  if (proofQueued) {
    return (
      <CurrentStatus
        formattedStatus={formattedStatus}
        statusIndicatorColor={statusIndicatorColor}
        statusMessage={statusMessage}
        statusError={statusError}
        proofQueueId={proofQueueId}
        submittedAtDisplay={submittedAtDisplay}
        statusUpdatedDisplay={statusUpdatedDisplay}
        statusData={statusData}
        isStatusLoading={isStatusLoading}
        onRefreshStatus={onRefreshStatus}
        submittedProofMode={submittedProofMode}
      />
    )
  }

  return (
    <section className="space-y-6">
      <InfoCard
        title="Ready to Submit Proof"
        description={
          <p className="text-xs text-white/40 mt-2">
            {snapshotHeightLabel ? (
              <>
                Snapshot captured at block <span className="text-white/60 font-mono">{snapshotHeightLabel}</span>
                {targetHeightLabel && snapshotHeightLabel !== targetHeightLabel && (
                  <> (target <span className="text-white/60 font-mono">{targetHeightLabel}</span>)</>
                )}
              </>
            ) : targetHeightLabel ? (
              <>
                Target snapshot at block <span className="text-white/60 font-mono">{targetHeightLabel}</span>
              </>
            ) : (
              <>
                Snapshot captured at block <span className="text-white/60 font-mono">2,650,000 (PLACEHOLDER)</span>
              </>
            )}
            <br />
            Verification is based on shielded balance at snapshot time.
          </p>
        }
      >
        {/* Proof Mode Selection */}
        <div className="space-y-4">
          <p className="text-xs text-white/60 mb-4">Select your proof generation method:</p>

          {/* ZK Only Option */}
          <button
            type="button"
            onClick={() => setSelectedMode("zk-only")}
            disabled={!!error}
            className={`w-full text-left border transition-all duration-200 ${error
              ? "border-white/5 bg-white/[0.01] cursor-not-allowed opacity-50"
              : selectedMode === "zk-only"
                ? "border-[#f3b724]/50 bg-[#f3b724]/10"
                : "border-white/10 bg-white/[0.02] hover:border-white/20"
              }`}
          >
            <div className="p-4 space-y-3">
              <div className="flex items-start justify-between gap-3">
                <div className="flex items-center gap-3">
                  <div
                    className={`w-4 h-4 rounded-full border-2 flex items-center justify-center ${selectedMode === "zk-only" ? "border-[#f3b724]" : "border-white/30"
                      }`}
                  >
                    {selectedMode === "zk-only" && (
                      <div className="w-2 h-2 rounded-full bg-[#f3b724]"></div>
                    )}
                  </div>
                  <div>
                    <h4 className="text-sm font-medium text-white/90">ZK Only</h4>
                    <p className="text-xs text-white/40 mt-0.5">Faster Processing</p>
                  </div>
                </div>
              </div>
              <p className="text-xs text-white/50 leading-relaxed pl-7">
                Your proof is generated using standard zero-knowledge proving infrastructure with parallel
                processing capabilities on the Succinct Prover Network. This is faster, but you are trusting the ZK infrastructure operators
                to not reveal your (read-only) private data during the proving process.
              </p>
              <div className="flex items-center gap-4 text-xs pl-7">
                <div className="inline-flex items-center gap-2 rounded-md border border-[#f3b724]/40 bg-[#f3b724]/10 px-2.5 py-1 text-[#f3b724]">
                  <span className="opacity-80">Estimated Time</span>
                  <span className="font-mono font-semibold">~4 min</span>
                </div>
              </div>
            </div>
          </button>

          {/* ZK + TEE Option - Disabled */}
          <button
            type="button"
            onClick={() => {}}
            disabled={true}
            className="w-full text-left border border-white/5 bg-white/[0.01] cursor-not-allowed opacity-40"
          >
            <div className="p-4 space-y-3">
              <div className="flex items-start justify-between gap-3">
                <div className="flex items-center gap-3">
                  <div className="w-4 h-4 rounded-full border-2 border-white/20 flex items-center justify-center">
                  </div>
                  <div>
                    <h4 className="text-sm font-medium text-white/70">ZK + TEE</h4>
                    <p className="text-xs text-white/30 mt-0.5">Maximum Privacy</p>
                  </div>
                </div>
                <div className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded text-[0.65rem] uppercase tracking-wider bg-red-500/10 border border-red-500/30 text-red-400">
                  <span>Disabled</span>
                </div>
              </div>
              <p className="text-xs text-white/30 leading-relaxed pl-7">
                Your proof is generated within a Trusted Execution Environment (TEE), ensuring your
                (read-only) shielded transaction data never leaves the secure enclave. The verifiable code running in the TEE is the only place the data can be decrypted.
              </p>
              <div className="flex items-center gap-4 text-xs pl-7">
                <div className="inline-flex items-center gap-2 rounded-md border border-white/10 bg-white/5 px-2.5 py-1 text-white/40">
                  <span className="opacity-80">Estimated Time</span>
                  <span className="font-mono font-semibold">~15 min</span>
                </div>
              </div>
              <div className="pl-7 pt-2">
                <p className="text-xs text-red-400/80 leading-relaxed">
                  ⚠️ Currently disabled. TEE proving takes significantly longer to process, which is not the focus of this demo.
                </p>
              </div>
            </div>
          </button>

          {/* Error Message */}
          {error && (
            <div className="rounded-md border border-red-500/50 bg-red-500/10 p-4">
              <div className="flex items-start gap-3">
                <svg
                  className="h-5 w-5 text-red-400 flex-shrink-0"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                    d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                  />
                </svg>
                <div className="flex-1 space-y-3">
                  <p className="text-sm text-red-400">Error: {error}</p>
                  {onReset && (
                    <button
                      type="button"
                      onClick={onReset}
                      className="px-4 py-2 text-xs font-medium uppercase tracking-[0.2em] text-red-300 border border-red-500/50 bg-red-500/20 hover:bg-red-500/30 hover:border-red-500/70 hover:text-red-200 transition-all duration-200"
                    >
                      Start Over
                    </button>
                  )}
                </div>
              </div>
            </div>
          )}

          {/* Submit Button */}
          <button
            type="button"
            onClick={() => selectedMode && onRequestProof(selectedMode)}
            disabled={isRequestingProof || proofQueued || !connected || !selectedMode || !!error}
            className={`w-full inline-flex items-center justify-center gap-2 px-4 py-2.5 text-xs font-medium uppercase tracking-[0.2em] transition-all duration-200 ${isRequestingProof || proofQueued || !connected || !selectedMode || !!error
              ? "border border-white/5 text-white/30 cursor-not-allowed"
              : "border border-[#f3b724]/50 text-[#f3b724] bg-[#f3b724]/10 hover:bg-[#f3b724]/20 hover:shadow-[0_0_20px_rgba(243,183,36,0.3)]"
              }`}
          >
            {isRequestingProof ? (
              <>
                <div className="w-4 h-4 border-2 border-[#f3b724]/30 border-t-[#f3b724] rounded-full animate-spin"></div>
                <span>Submitting...</span>
              </>
            ) : proofQueued ? (
              "Requested"
            ) : !selectedMode ? (
              "Select a Method Above"
            ) : (
              `Submit Proof (${selectedMode === "zk-tee" ? "ZK + TEE" : "ZK Only"})`
            )}
          </button>
        </div>
      </InfoCard>

      {/* Back button */}
      <div className="flex items-center gap-3">
        <button
          type="button"
          onClick={onBack}
          className="px-4 py-2.5 text-xs font-medium uppercase tracking-[0.2em] text-white/60 hover:text-white transition-colors duration-200"
        >
          Back
        </button>
      </div>
    </section>
  )
}

