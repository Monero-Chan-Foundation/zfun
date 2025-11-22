"use client"

import { memo, useCallback, useMemo, useState, useEffect } from "react"
import { createPortal } from "react-dom"
import { useWallet } from "@solana/wallet-adapter-react"
import type { WalletName } from "@solana/wallet-adapter-base"

function truncate(pubkey?: string | null) {
  if (!pubkey) return ""
  return `${pubkey.slice(0, 4)}…${pubkey.slice(-4)}`
}

export const WalletConnectButton = memo(function WalletConnectButton() {
  const { connected, publicKey, disconnect, wallets, select, connect, wallet, connecting } = useWallet()
  const [open, setOpen] = useState(false)
  const [mounted, setMounted] = useState(false)
  const [pending, setPending] = useState<WalletName | null>(null)
  useEffect(() => setMounted(true), [])

  const addr = useMemo(() => publicKey?.toBase58() ?? "", [publicKey])

  const onConnectClick = useCallback(() => setOpen((v) => !v), [])
  const onToggleMenu = useCallback(() => setOpen((v) => !v), [])
  const onCopy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(addr)
      setOpen(false)
    } catch {
      // ignore
    }
  }, [addr])
  const onDisconnect = useCallback(async () => {
    try {
      await disconnect()
    } finally {
      setOpen(false)
    }
  }, [disconnect])

  const uniqueWallets = useMemo(() => {
    const map = new Map<WalletName, typeof wallets[number]>()
    for (const w of wallets) map.set(w.adapter.name, w)
    return Array.from(map.values())
  }, [wallets])

  useEffect(() => {
    if (pending && wallet?.adapter.name === pending && !connected && !connecting) {
      ; (async () => {
        try {
          await connect()
          setOpen(false)
        } catch {
          // ignore connect errors
        } finally {
          setPending(null)
        }
      })()
    }
  }, [pending, wallet, connected, connecting, connect])

  // select a wallet; connect runs after selection is applied in the effect below
  const onPick = async (name: WalletName) => {
    try {
      await select(name)
      setPending(name)
    } catch {
      // ignore
    }
  }

  const WalletIcon = ({ name }: { name: string }) => {
    const n = name.toLowerCase()
    if (n.includes("phantom")) {
      return (
        <svg viewBox="0 0 32 32" className="h-4 w-4" aria-hidden>
          <defs>
            <linearGradient id="p" x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor="#8a5cff" />
              <stop offset="100%" stopColor="#5c2aff" />
            </linearGradient>
          </defs>
          <circle cx="16" cy="16" r="16" fill="url(#p)" />
          <path d="M11 17c0 3 2 5 5 5s5-2 5-5-2-5-5-5-5 2-5 5zm3.2-1.3c.5-.6 1.5-.6 2 0 .6.5.6 1.5 0 2a1.4 1.4 0 0 1-2 0c-.5-.5-.5-1.4 0-2z" fill="#fff" />
        </svg>
      )
    }
    if (n.includes("solflare")) {
      return (
        <svg viewBox="0 0 32 32" className="h-4 w-4" aria-hidden>
          <circle cx="16" cy="16" r="16" fill="#f46a1e" />
          <circle cx="16" cy="16" r="7" fill="#ffd7b3" />
        </svg>
      )
    }
    if (n.includes("backpack")) {
      return (
        <svg viewBox="0 0 32 32" className="h-4 w-4" aria-hidden>
          <rect x="4" y="6" width="24" height="20" rx="6" fill="#ff3b30" />
          <rect x="10" y="10" width="12" height="8" rx="2" fill="#ffd1ce" />
        </svg>
      )
    }
    return (
      <svg viewBox="0 0 32 32" className="h-4 w-4" aria-hidden>
        <circle cx="16" cy="16" r="16" fill="#64748b" />
      </svg>
    )
  }

  if (!connected || !publicKey) {
    return (
      <div className="relative">
        <button
          type="button"
          onClick={onConnectClick}
          className="group/btn cursor-pointer inline-flex items-center gap-3 px-8 py-3.5 text-xs font-medium uppercase tracking-[0.25em] text-white border border-white/20 hover:border-[#f3b724]/50 hover:bg-[#f3b724]/10 transition-all duration-200"
        >
          <span>Connect Wallet</span>
          <svg className="w-4 h-4 transition-transform group-hover/btn:translate-x-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
          </svg>
        </button>
        {open && mounted &&
          createPortal(
            <div className="fixed inset-0 z-[1000] flex items-center justify-center bg-black/60 backdrop-blur-sm" onClick={() => setOpen(false)}>
              <div className="w-full max-w-md overflow-hidden border border-white/10 bg-[rgb(18,18,18)]/95 backdrop-blur-xl text-white shadow-2xl" onClick={(e) => e.stopPropagation()}>
                <div className="flex items-center justify-between border-b border-white/10 px-6 py-4">
                  <p className="text-xs uppercase tracking-[0.2em] text-white/60">Select Wallet</p>
                  <button 
                    type="button" 
                    onClick={() => setOpen(false)} 
                    className="relative group px-3 py-1.5 text-xs uppercase tracking-[0.15em] text-white/60 hover:text-white transition-colors cursor-pointer"
                  >
                    <span className="relative z-10">Close</span>
                    <div className="absolute inset-0 bg-white/0 group-hover:bg-white/5 transition-colors"></div>
                </button>
                </div>
                <div className="max-h-[60vh] overflow-y-auto px-4 py-3">
                  {uniqueWallets.map((w) => {
                    const adapterAny = w.adapter as unknown as { icon?: string }
                    const icon = adapterAny.icon
                    return (
                      <button
                        key={w.adapter.name}
                        type="button"
                        onClick={() => onPick(w.adapter.name)}
                        className="group flex w-full items-center gap-3 px-4 py-3 text-left text-sm transition-all duration-200 border-b border-white/5 last:border-0 hover:bg-white/5 cursor-pointer"
                      >
                        {icon ? (
                          // eslint-disable-next-line @next/next/no-img-element
                          <img src={icon} alt={`${w.adapter.name} logo`} className="h-5 w-5" />
                        ) : (
                          <WalletIcon name={w.adapter.name} />
                        )}
                        <span className="text-white/80 group-hover:text-white transition-colors">{w.adapter.name}</span>
                      </button>
                    )
                  })}
                </div>
              </div>
            </div>,
            document.body,
          )}
      </div>
    )
  }

  return (
    <div className="relative">
      <button
        type="button"
        onClick={onToggleMenu}
        className="relative group px-4 py-2.5 text-xs font-medium uppercase tracking-[0.15em] transition-all duration-300 cursor-pointer"
      >
        <span className="relative z-10 flex items-center gap-2">
          <span className="w-2 h-2 rounded-full bg-emerald-400 animate-pulse"></span>
          <span className="text-white">{truncate(addr)}</span>
        </span>
        <div className="absolute inset-0 border border-white/20 bg-white/5 backdrop-blur-sm transition-all duration-300 group-hover:border-white/30 group-hover:bg-white/10"></div>
      </button>
      {open && (
        <div className="absolute right-0 mt-2 w-48 overflow-hidden border border-white/10 bg-[rgb(18,18,18)]/95 backdrop-blur-xl text-white shadow-xl">
          <button
            type="button"
            onClick={onCopy}
            className="group relative flex w-full items-center gap-2 px-4 py-3 text-left text-xs uppercase tracking-[0.15em] transition-all border-b border-white/5 cursor-pointer"
          >
            <svg className="w-4 h-4 text-white/60 group-hover:text-white transition-colors" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
            </svg>
            <span className="relative z-10 text-white/80 group-hover:text-white transition-colors">Copy Address</span>
            <div className="absolute inset-0 bg-white/0 group-hover:bg-white/5 transition-colors"></div>
          </button>
          <button
            type="button"
            onClick={onDisconnect}
            className="group relative flex w-full items-center gap-2 px-4 py-3 text-left text-xs uppercase tracking-[0.15em] transition-all cursor-pointer"
          >
            <svg className="w-4 h-4 text-red-400/60 group-hover:text-red-400 transition-colors" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1" />
            </svg>
            <span className="relative z-10 text-red-400/80 group-hover:text-red-400 transition-colors">Disconnect</span>
            <div className="absolute inset-0 bg-white/0 group-hover:bg-red-500/5 transition-colors"></div>
          </button>
        </div>
      )}
    </div>
  )
})
