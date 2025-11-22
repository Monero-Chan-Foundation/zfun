"use client"

import { SolanaProvider } from "./solana/WalletProvider"

export default function AppProviders({ children }: { children: React.ReactNode }) {
  return <SolanaProvider>{children}</SolanaProvider>
}

