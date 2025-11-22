"use client"

import { cn } from "@/lib/utils"
import { InfoCard } from "../../../components/InfoCard"

type ProofStatusResponse = {
  status?: string
  proof_url?: string
  [key: string]: unknown
}

interface CurrentStatusProps {
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
  submittedProofMode?: "zk-tee" | "zk-only" | null
}

export function CurrentStatus({
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
}: CurrentStatusProps) {
  return (
    <section className="space-y-6">
      <InfoCard
        title=""
        headerContent={
          <>
            <div>
              <h3 className="text-xs text-white/40 uppercase tracking-wider mb-2">Proof Status</h3>
              <p className={cn(
                "text-xl font-medium uppercase tracking-wide",
                formattedStatus === "Processed" ? "text-white/90" : "text-white"
              )}>{formattedStatus}</p>
            </div>
            {statusMessage && (
              <p className="text-xs text-white/40 mt-2">
                {statusMessage}
              </p>
            )}
          </>
        }
      >
        {isStatusLoading && !statusData ? (
          <div className="flex items-center gap-3 text-xs text-white/40">
            <div className="h-3 w-3 animate-ping rounded-full bg-[#f3b724]" />
            Checking status with the prover…
          </div>
        ) : (
          <>
            {statusUpdatedDisplay && (
              <div className="flex items-center gap-2 text-xs text-white/40 mb-6">
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                </svg>
                <span>Last updated: <span className="text-white/60">{statusUpdatedDisplay}</span></span>
              </div>
            )}

            {statusError && (
              <div className="border border-red-500/30 bg-red-500/5 p-4 mb-6">
                <div className="flex items-start gap-3">
                  <svg className="w-5 h-5 text-red-400 flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                  </svg>
                  <p className="text-sm text-red-300">{statusError}</p>
                </div>
              </div>
            )}

            {statusData && statusData.had_duplicate_nullifiers && (
              <div className="border border-blue-500/30 bg-blue-500/5 p-4 mb-6">
                <div className="flex items-start gap-3">
                  <svg className="w-5 h-5 text-blue-400 flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                  </svg>
                  <div className="flex-1">
                    <p className="text-sm font-medium text-blue-300 mb-1">Re-verification Detected</p>
                    <p className="text-xs text-blue-200/80 leading-relaxed">
                      We detected that your note's alt nullifier has already verified itself. In production environments, we typically wouldn't let you verify your note again to prevent double counting, but due to this being a demo you can verify as many times as you want.
                    </p>
                  </div>
                </div>
              </div>
            )}

            <div className="space-y-3">
              <div className="flex items-center justify-between mb-4">
                <p className="text-xs uppercase tracking-[0.2em] text-white/40 font-medium">Additional Details</p>
                <div className="flex flex-wrap items-center gap-3">
                  {proofQueueId && (
                    <button
                      type="button"
                      onClick={() => proofQueueId && navigator.clipboard?.writeText(proofQueueId)}
                      className="border border-white/20 bg-white/5 hover:bg-white/10 px-3 py-1.5 text-[10px] uppercase tracking-[0.3em] text-white transition-all duration-200"
                    >
                      Copy ID
                    </button>
                  )}
                  <button
                    type="button"
                    onClick={onRefreshStatus}
                    disabled={!proofQueueId || isStatusLoading}
                    className={cn(
                      "border px-3 py-1.5 text-[10px] uppercase tracking-[0.3em] transition-all duration-200",
                      !proofQueueId || isStatusLoading
                        ? "border-white/10 text-white/30 cursor-not-allowed"
                        : "border-white/20 bg-white/5 hover:bg-white/10 text-white",
                    )}
                  >
                    {isStatusLoading ? "Refreshing…" : "Refresh"}
                  </button>
                </div>
              </div>

              {proofQueueId && (
                <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between">
                  <span className="text-xs uppercase tracking-[0.2em] text-white/30">Request ID</span>
                  <span className="font-mono text-sm text-white/70 break-all">
                    <a href={`/proof/${proofQueueId}`} className="text-[#f3b724] hover:text-[#ffd700] underline underline-offset-4 break-all transition-colors">
                      {proofQueueId}
                    </a>
                  </span>
                </div>
              )}
              {statusUpdatedDisplay && (
                <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between">
                  <span className="text-xs uppercase tracking-[0.2em] text-white/30">Last Update</span>
                  <span className="text-white/70 font-mono text-sm">{statusUpdatedDisplay}</span>
                </div>
              )}
              {proofQueueId && (
                <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between">
                  <span className="text-xs uppercase tracking-[0.2em] text-white/30">Explorer</span>
                  <a
                    href={`https://explorer.monero-chan.org/proof/${proofQueueId.startsWith('0x') ? proofQueueId : '0x' + proofQueueId}`}
                    target="_blank"
                    rel="noreferrer"
                    className="text-[#f3b724] hover:text-[#ffd700] underline underline-offset-4 break-all transition-colors text-sm"
                  >
                    View in Explorer →
                  </a>
                </div>
              )}
              {statusData && statusData.proof_url && (
                <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between">
                  <span className="text-xs uppercase tracking-[0.2em] text-white/30">Proof URL</span>
                  <a href={typeof statusData.proof_url === "string" ? statusData.proof_url : "#"} target="_blank" rel="noreferrer" className="text-[#f3b724] hover:text-[#ffd700] underline underline-offset-4 break-all transition-colors text-sm">
                    Download Proof →
                  </a>
                </div>
              )}
              {(submittedProofMode || (statusData && statusData.is_tee !== undefined)) && (
                <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between">
                  <span className="text-xs uppercase tracking-[0.2em] text-white/30">PROVER MODE</span>
                  <span className="text-white/70 font-mono text-sm">
                    {submittedProofMode 
                      ? (submittedProofMode === "zk-tee" ? "ZK + TEE" : "ZK Only")
                      : (statusData?.is_tee === true || statusData?.is_tee === "true" ? "ZK + TEE" : "ZK")}
                  </span>
                </div>
              )}
              {statusData && statusData.created_at !== undefined && (
                <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between">
                  <span className="text-xs uppercase tracking-[0.2em] text-white/30">Created At</span>
                  <span className="text-white/70 font-mono text-sm">
                    {typeof statusData.created_at === "number"
                      ? new Date(statusData.created_at * 1000).toLocaleString()
                      : typeof statusData.created_at === "string"
                        ? new Date(statusData.created_at).toLocaleString()
                        : String(statusData.created_at)}
                  </span>
                </div>
              )}
              {statusData && statusData.fulfilled_at !== undefined && statusData.fulfilled_at !== null && (
                <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between">
                  <span className="text-xs uppercase tracking-[0.2em] text-white/30">Fulfilled At</span>
                  <span className="text-white/70 font-mono text-sm">
                    {typeof statusData.fulfilled_at === "number"
                      ? new Date(statusData.fulfilled_at * 1000).toLocaleString()
                      : typeof statusData.fulfilled_at === "string"
                        ? new Date(statusData.fulfilled_at).toLocaleString()
                        : String(statusData.fulfilled_at)}
                  </span>
                </div>
              )}
              {statusData && statusData.balance !== undefined && statusData.balance !== null && (
                <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between">
                  <span className="text-xs uppercase tracking-[0.2em] text-white/30">Balance</span>
                  <span className="text-white/70 font-mono text-sm">
                    {typeof statusData.balance === "number"
                      ? statusData.balance.toLocaleString(undefined, { minimumFractionDigits: 0, maximumFractionDigits: 6 })
                      : String(statusData.balance)}
                  </span>
                </div>
              )}
            </div>
          </>
        )}
      </InfoCard>
    </section>
  )
}

