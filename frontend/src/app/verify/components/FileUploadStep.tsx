"use client"

import { useState } from "react"
import type { DragEvent, RefObject } from "react"

interface FileUploadStepProps {
  inputRef: RefObject<HTMLInputElement | null>
  onDrop: (event: DragEvent<HTMLDivElement>) => void
  onFileChange: (file: File | null) => void
  isWasmReady: boolean
  wasmInitError: string | null
  mode: string
  error: string | null
  isProcessing: boolean
}

export function FileUploadStep({
  inputRef,
  onDrop,
  onFileChange,
  isWasmReady,
  wasmInitError,
  mode,
  error,
  isProcessing,
}: FileUploadStepProps) {
  const [instructionsExpanded, setInstructionsExpanded] = useState(false)

  return (
    <section className="space-y-6">
      {/* Collapsible Instructions */}
      <div className="border border-white/10 bg-white/[0.02] backdrop-blur-sm overflow-hidden">
        <button
          type="button"
          onClick={() => setInstructionsExpanded(!instructionsExpanded)}
          className="w-full p-4 flex items-center justify-between hover:bg-white/[0.02] transition-colors duration-200"
        >
          <div className="flex items-center gap-2.5">
            <svg className="w-4 h-4 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
            <h3 className="text-xs text-white/80 uppercase tracking-wider">How to Export Your Zashi Database</h3>
          </div>
          <svg
            className={`w-4 h-4 text-white/60 transition-transform duration-200 ${instructionsExpanded ? 'rotate-180' : ''}`}
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
          </svg>
        </button>

        {instructionsExpanded && (
          <div className="px-4 pb-4">
            {/* Description */}
            <div className="mb-4">
              <p className="text-xs text-white/50 leading-relaxed pt-2">
                Export your Zashi private data file to verify your <span className="text-white/80">shielded balance</span>. This file contains read-only data and cannot transfer funds or control your wallet. Your data is processed securely using zero-knowledge proofs in a Trusted Execution Environment (TEE).
              </p>
              <p className="text-xs text-white/50 leading-relaxed pt-3">
                <span className="font-medium">Note:</span> Your transactions must be finalized on the blockchain before they appear in your exported data. Allow sufficient time for confirmations before exporting.
              </p>
              <p className="text-xs text-white/50 leading-relaxed pt-3">
                By using this service, you will be asked to cryptographically sign an agreement to our{" "}
                <a
                  href="/terms"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="underline text-white/80 hover:text-white transition-colors"
                >
                  Terms
                </a>
                .
              </p>
            </div>

            <div className="h-px bg-gradient-to-r from-white/10 to-transparent mb-4"></div>

            <div className="space-y-3">
              <div className="flex gap-3">
                <div className="flex-shrink-0 w-5 h-5 rounded-full bg-[#f3b724]/10 border border-[#f3b724]/30 flex items-center justify-center">
                  <span className="text-[10px] font-bold text-[#f3b724]">1</span>
                </div>
                <div className="flex-1">
                  <p className="text-xs text-white/80 font-medium mb-0.5">Open Zashi Wallet</p>
                  <p className="text-xs text-white/50 leading-relaxed">Launch the Zashi application on your device.</p>
                </div>
              </div>

              <div className="flex gap-3">
                <div className="flex-shrink-0 w-5 h-5 rounded-full bg-[#f3b724]/10 border border-[#f3b724]/30 flex items-center justify-center">
                  <span className="text-[10px] font-bold text-[#f3b724]">2</span>
                </div>
                <div className="flex-1">
                  <p className="text-xs text-white/80 font-medium mb-0.5">Navigate to Advanced Settings</p>
                  <p className="text-xs text-white/50 leading-relaxed">Click Settings, then click Advanced Settings.</p>
                </div>
              </div>

              <div className="flex gap-3">
                <div className="flex-shrink-0 w-5 h-5 rounded-full bg-[#f3b724]/10 border border-[#f3b724]/30 flex items-center justify-center">
                  <span className="text-[10px] font-bold text-[#f3b724]">3</span>
                </div>
                <div className="flex-1">
                  <p className="text-xs text-white/80 font-medium mb-0.5">Export and Transfer File</p>
                  <p className="text-xs text-white/50 leading-relaxed mb-2">Click "Export Private Data" and save the .db file. Then transfer this file to the device you're using this app on:</p>
                  <ul className="text-xs text-white/50 leading-relaxed space-y-0.5 ml-3">
                    <li>• <span className="text-white/60">Computer:</span> Email or AirDrop the .db file to your computer and download it</li>
                    <li>• <span className="text-white/60">Phone:</span> Save it to your Files app and locate the .db file on your phone</li>
                  </ul>
                </div>
              </div>

              <div className="flex gap-3">
                <div className="flex-shrink-0 w-5 h-5 rounded-full bg-[#f3b724]/10 border border-[#f3b724]/30 flex items-center justify-center">
                  <span className="text-[10px] font-bold text-[#f3b724]">4</span>
                </div>
                <div className="flex-1">
                  <p className="text-xs text-white/80 font-medium mb-0.5">Upload Below</p>
                  <p className="text-xs text-white/50 leading-relaxed">Use the upload area below to select your exported database file.</p>
                </div>
              </div>
            </div>
          </div>
        )}
      </div>

      {/* Modern Upload Card */}
      <div
        onDragOver={(event) => {
          event.preventDefault()
        }}
        onDrop={(event) => {
          event.preventDefault()
          if (!isWasmReady || isProcessing) {
            return
          }
          onDrop(event)
        }}
        className={`group relative border border-white/10 bg-white/[0.02] transition-all duration-200 ${isWasmReady && !isProcessing
          ? "cursor-pointer hover:border-white/20"
          : "cursor-not-allowed opacity-60"
          }`}
        onClick={() => {
          if (isWasmReady && !isProcessing) {
            inputRef.current?.click()
          }
        }}
      >
        <div className="relative flex flex-col items-center gap-3 px-4 py-16 text-center">
          <div className="space-y-2">
            <h2 className="text-lg font-semibold text-white/90 uppercase tracking-tight">
              Upload Database
            </h2>
            <p className="text-xs text-white/50 leading-relaxed max-w-lg">
              {isWasmReady
                ? "Drag & drop your Zashi exported database (.db), or click to browse. Your private data never leaves this page."
                : "Please wait while the WebAssembly runtime initializes..."}
            </p>
          </div>

          <button
            disabled={!isWasmReady || isProcessing}
            className={`group/btn inline-flex items-center gap-2.5 px-6 py-2.5 text-xs font-medium uppercase tracking-[0.2em] text-white border transition-all duration-200 ${isWasmReady && !isProcessing
              ? "cursor-pointer border-white/20 hover:border-white/30"
              : "cursor-not-allowed border-white/10 opacity-50"
              }`}
          >
            <span>Select File</span>
            <svg className="w-3.5 h-3.5 transition-transform group-hover/btn:translate-x-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
            </svg>
          </button>

          {/* Status Message */}
          {(!wasmInitError && !isWasmReady && !error) && (
            <div className="flex items-center gap-2 text-base text-white/40">
              <div className="w-3 h-3 border border-white/20 border-t-transparent rounded-full animate-spin"></div>
              <span>Initializing WASM runtime...</span>
            </div>
          )}
          {isProcessing && (
            <div className="flex items-center gap-2 text-xs text-white/40">
              <div className="w-3 h-3 border border-white/20 border-t-transparent rounded-full animate-spin"></div>
              <span>Processing your database...</span>
            </div>
          )}
          {wasmInitError && (
            <div className="flex items-center gap-2 px-3 py-2 bg-red-500/10 border border-red-500/30 text-xs text-red-300">
              <svg className="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
              {wasmInitError}
            </div>
          )}
          {error && !isProcessing && (
            <div className="flex items-start gap-2 px-3 py-2 bg-red-500/10 border border-red-500/30 text-xs text-red-300 max-w-md">
              <svg className="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
              <span className="text-left">{error}</span>
            </div>
          )}
        </div>

        <input
          ref={inputRef}
          type="file"
          accept=".db,.sqlite,.sqlite3,application/x-sqlite3,application/vnd.sqlite3,application/octet-stream"
          className="hidden"
          disabled={!isWasmReady || isProcessing}
          onChange={(event) => {
            if (!isWasmReady) {
              event.preventDefault()
              return
            }
            const file = event.target.files?.[0]
            onFileChange(file ?? null)
            event.target.value = ""
          }}
        />
      </div>
    </section>
  )
}

