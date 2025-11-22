"use client"

import { WalletConnectButton } from "../../../components/solana/WalletConnectButton"
import { InfoCard } from "../../../components/InfoCard"

interface WalletConnectionStepProps {
  connected: boolean
  publicKey: { toBase58: () => string } | null
  addressCopied: boolean
  isRequestingSignature: boolean
  signatureError: string | null
  signatureReceived: boolean
  onCopyAddress: () => void
  onDisconnect: () => void
  onBack: () => void
  onContinue: () => void
  onRequestSignature: () => void
}

export function WalletConnectionStep({
  connected,
  publicKey,
  addressCopied,
  isRequestingSignature,
  signatureError,
  signatureReceived,
  onCopyAddress,
  onDisconnect,
  onBack,
  onContinue,
  onRequestSignature,
}: WalletConnectionStepProps) {
  return (
    <section className="space-y-8">
      {!connected || !publicKey ? (
        <div className="space-y-6">
          <div className="border border-white/10 bg-white/[0.02]">
            <div className="flex flex-col items-center gap-3 px-4 py-16 text-center">
              <div className="space-y-1.5">
                <h2 className="text-base font-medium text-white/80">Connect Your Wallet</h2>
                <p className="text-xs text-white/40 leading-relaxed max-w-md">
                  Connect your Solana wallet to use for verification
                </p>
              </div>
              <div className="pt-1">
                <WalletConnectButton />
              </div>
            </div>
          </div>
          <div className="flex items-center gap-3">
            <button
              type="button"
              onClick={onBack}
              className="px-4 py-2.5 text-xs font-medium uppercase tracking-[0.2em] text-white/60 hover:text-white transition-colors duration-200"
            >
              Back
            </button>
          </div>
        </div>
      ) : (
        <div className="space-y-6 ">
          {/* Step 1: Connected Wallet */}
          <InfoCard
            title="Connected Wallet"
            description="This wallet will be used for verification. If you chose the wrong wallet, please disconnect and connect the correct one."
          >
            <div className="space-y-3">
              <div className="flex items-center gap-2">
                <p className="text-xs uppercase tracking-[0.2em] text-white/40 font-medium">Wallet Address</p>
              </div>
              <div className="flex items-center gap-3">
                <div className="flex-1 bg-black/40 border border-white/10 px-4 py-3.5">
                  <p className="text-sm font-mono text-white/90 break-all leading-relaxed">
                    {publicKey.toBase58()}
                  </p>
                </div>
                <button
                  type="button"
                  onClick={onDisconnect}
                  className="flex-shrink-0 flex items-center justify-center gap-2 px-5 py-3.5 text-xs font-medium uppercase tracking-[0.15em] transition-all duration-200 border border-white/20 bg-white/5 hover:bg-red-500/10 hover:border-red-500/30 text-white/80 hover:text-red-400 cursor-pointer"
                >
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1" />
                  </svg>
                  <span>Disconnect</span>
                </button>
              </div>
            </div>
          </InfoCard>

          {/* Step 2: Sign Message */}
          <InfoCard
            title="Verify Wallet Ownership & Accept Terms"
            description="Sign a message to prove you control this wallet address and legally accept the Terms of Service. This cryptographic signature serves as your binding agreement."
          >
            <p className="text-xs text-white/50 leading-relaxed mb-4">
              By signing, you agree to our{" "}
              <a 
                href="/terms" 
                target="_blank" 
                rel="noopener noreferrer"
                className="underline text-white/80 hover:text-white font-medium transition-colors"
              >
                Terms
              </a>
              . Please review before signing.
            </p>

            {isRequestingSignature ? (
              <div className="flex items-center gap-3 px-4 py-3 bg-white/5 border border-white/10">
                <div className="w-5 h-5 border-2 border-[#f3b724]/30 border-t-[#f3b724] rounded-full animate-spin flex-shrink-0"></div>
                <div>
                  <p className="text-sm font-medium text-white/90">Waiting for signature...</p>
                  <p className="text-xs text-white/50 mt-0.5">Please approve the signature request in your wallet</p>
                </div>
              </div>
            ) : signatureError ? (
              <div className="space-y-4">
                <div className="flex items-start gap-3 px-4 py-3 bg-red-500/10 border border-red-500/30">
                  <svg className="w-5 h-5 text-red-400 flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                  </svg>
                  <div className="flex-1">
                    <p className="text-sm font-medium text-red-400">Signature Failed</p>
                    <p className="text-xs text-red-300/80 mt-1">{signatureError}</p>
                  </div>
                </div>
                <button
                  type="button"
                  onClick={onRequestSignature}
                  className="w-full flex items-center justify-center gap-2 px-5 py-3 text-xs font-medium uppercase tracking-[0.15em] transition-all duration-200 border border-[#f3b724]/50 text-[#f3b724] bg-[#f3b724]/10 hover:bg-[#f3b724]/20 cursor-pointer"
                >
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                  </svg>
                  Try Again
                </button>
              </div>
            ) : signatureReceived ? (
              <div className="flex items-start gap-3 px-4 py-3 bg-[#f3b724]/10 border border-[#f3b724]/30">
                <svg className="w-5 h-5 text-[#f3b724] flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
                </svg>
                <div className="flex-1">
                  <p className="text-sm font-medium text-[#f3b724]">Signature Verified</p>
                  <p className="text-xs text-white/50 mt-0.5">You've successfully set your wallet and agreed to our Terms of Service</p>
                </div>
              </div>
            ) : (
              <button
                type="button"
                onClick={onRequestSignature}
                className="w-full flex items-center justify-center gap-2 px-5 py-3.5 text-xs font-medium uppercase tracking-[0.15em] transition-all duration-200 border border-[#f3b724]/50 text-[#f3b724] bg-[#f3b724]/10 hover:bg-[#f3b724]/20 cursor-pointer"
              >
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" />
                </svg>
                Sign Message
              </button>
            )}
          </InfoCard>

          {/* Continue button */}
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
              disabled={!signatureReceived || isRequestingSignature}
              className="flex-1 inline-flex items-center justify-center gap-2 px-4 py-2.5 text-xs font-medium uppercase tracking-[0.2em] transition-all duration-200 border border-[#f3b724]/50 text-[#f3b724] bg-[#f3b724]/10 hover:bg-[#f3b724]/20 hover:shadow-[0_0_20px_rgba(243,183,36,0.3)] disabled:opacity-50 disabled:cursor-not-allowed disabled:hover:bg-[#f3b724]/10 disabled:hover:shadow-none"
            >
              Continue to Submit
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 7l5 5m0 0l-5 5m5-5H6" />
              </svg>
            </button>
          </div>
        </div>
      )}
    </section>
  )
}

