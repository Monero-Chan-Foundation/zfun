"use client"

import { useMemo } from "react"
import { ConnectionProvider, WalletProvider } from "@solana/wallet-adapter-react"
import { clusterApiUrl } from "@solana/web3.js"
import { PhantomWalletAdapter } from "@solana/wallet-adapter-phantom"
import { BackpackWalletAdapter } from "@solana/wallet-adapter-backpack"
import { SolflareWalletAdapter } from "@solana/wallet-adapter-solflare"

// Using a custom modal to avoid duplicate wallet keys from Wallet Standard in some environments.

type Props = {
  children: React.ReactNode
}

export function SolanaProvider({ children }: Props) {
  const endpoint = useMemo(() => {
    const envUrl = process.env.NEXT_PUBLIC_SOLANA_RPC_URL
    const cluster = (process.env.NEXT_PUBLIC_SOLANA_CLUSTER || "devnet") as
      | "devnet"
      | "mainnet-beta"
      | "testnet"
    return envUrl && envUrl.length > 0 ? envUrl : clusterApiUrl(cluster)
  }, [])

  const wallets = useMemo(
    () => [new PhantomWalletAdapter(), new BackpackWalletAdapter(), new SolflareWalletAdapter()],
    [],
  )

  return (
    <ConnectionProvider endpoint={endpoint}>
      <WalletProvider wallets={wallets} autoConnect={false}>{children}</WalletProvider>
    </ConnectionProvider>
  )
}
