"use client"

import { useMemo, useState, useEffect, useRef } from "react"
import Link from "next/link"
import { Navbar } from "../components/navbar"
import { Footer } from "../components/Footer"
import { LineChart, Line, BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, Legend, ResponsiveContainer, ComposedChart, Area } from 'recharts'

const PROOF_API_BASE =
  process.env.NEXT_PUBLIC_SERVER_BASE_URL?.replace(/\/+$/, "") ?? "http://localhost:3000"

// Transaction types and data
type Transaction = {
  id: string
  address: string
  amount?: number
  timestamp: number
}

type CumulativeProofDataPoint = {
  date: string
  full_date: string
  total_proofs: number
  daily_increase: number
}

type CumulativeAmountsDataPoint = {
  date: string
  full_date: string
  amount: number
}

type RecentTransaction = {
  id: string
  address: string
  amount?: number
  timestamp: number
}

type StatsResponse = {
  cumulative_proofs: CumulativeProofDataPoint[]
  cumulative_amounts: CumulativeAmountsDataPoint[]
  recent_transactions: RecentTransaction[]
}


const QUEUES = {
  zkTEE: {
    title: "Fully ZK + TEE",
    eta: "6 minutes",
    description: "Full privacy verification with TEE",
    backlog: 5000,
    processedToday: 300,
  },
  zkOnly: {
    title: "ZK ONLY",
    eta: "30 seconds",
    description: "Fast ZK verification",
    backlog: 20,
    processedToday: 10000,
  },
}

const SYSTEM_STATUS = [
  {
    label: "SP1 workers",
    status: "Operational",
    detail: "4 shielded circuits",
  },
  {
    label: "Zcash light client",
    status: "Synced",
    detail: "Height 2,108,342",
  },
  {
    label: "Verification contracts",
    status: "Funding",
    detail: "Claims enabled for waves 1-3",
  },
]

export default function DashboardPage() {
  const totalBacklog = useMemo(() => QUEUES.zkTEE.backlog + QUEUES.zkOnly.backlog, [])
  const [cumulativeProofData, setCumulativeProofData] = useState<CumulativeProofDataPoint[]>([])
  const [cumulativeAmountsData, setCumulativeAmountsData] = useState<CumulativeAmountsDataPoint[]>([])
  const [transactions, setTransactions] = useState<Transaction[]>([])
  const [queueView, setQueueView] = useState<'combined' | 'zkOnly'>('combined')
  const [searchInput, setSearchInput] = useState('')
  const [isSearchFocused, setIsSearchFocused] = useState(false)
  const [isLoading, setIsLoading] = useState(true)

  // Fetch stats from server
  useEffect(() => {
    const fetchStats = async () => {
      try {
        const response = await fetch(`${PROOF_API_BASE}/stats`, {
          method: "GET",
          headers: {
            "Content-Type": "application/json",
          },
          cache: "no-store",
        })

        if (!response.ok) {
          throw new Error(`Stats fetch failed: ${response.status} ${response.statusText}`)
        }

        const data = (await response.json()) as StatsResponse
        setCumulativeProofData(data.cumulative_proofs)
        setCumulativeAmountsData(data.cumulative_amounts)

        // Convert recent transactions to Transaction format
        const txList: Transaction[] = data.recent_transactions.map(tx => ({
          id: tx.id,
          address: tx.address,
          amount: tx.amount,
          timestamp: tx.timestamp,
        }))
        setTransactions(txList)
        setIsLoading(false)
      } catch (err) {
        console.error("Failed to fetch stats", err)
        setIsLoading(false)
        // Keep empty arrays on error
      }
    }

    void fetchStats()
    // Refresh every 30 seconds
    const interval = setInterval(() => {
      void fetchStats()
    }, 30000)
    return () => clearInterval(interval)
  }, [])

  const chartRef = useRef<HTMLDivElement>(null)

  // Make SVG non-focusable and prevent selection
  useEffect(() => {
    const chartContainer = chartRef.current
    if (!chartContainer) return

    const setupSVG = () => {
      const svg = chartContainer.querySelector('svg')
      if (svg) {
        svg.setAttribute('tabindex', '-1')
        svg.style.outline = 'none'
        svg.style.userSelect = 'none'

        // Prevent focus on click
        const handleFocus = (e: FocusEvent) => {
          e.preventDefault()
            ; (e.target as HTMLElement).blur()
        }

        // Prevent selection
        const handleSelectStart = (e: Event) => {
          e.preventDefault()
        }

        svg.addEventListener('focus', handleFocus)
        svg.addEventListener('focusin', handleFocus)
        chartContainer.addEventListener('selectstart', handleSelectStart)

        return () => {
          svg.removeEventListener('focus', handleFocus)
          svg.removeEventListener('focusin', handleFocus)
          chartContainer.removeEventListener('selectstart', handleSelectStart)
        }
      }
      return undefined
    }

    // Try immediately
    let cleanup = setupSVG()

    // If SVG not found, try again after a short delay (Recharts renders asynchronously)
    if (!cleanup) {
      const timeout = setTimeout(() => {
        cleanup = setupSVG()
      }, 100)

      return () => {
        clearTimeout(timeout)
        cleanup?.()
      }
    }

    return cleanup
  }, [])

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

        <div className="mx-auto flex min-h-screen w-full max-w-7xl flex-col gap-8 px-4 sm:px-6 lg:px-8 pt-8 pb-16 relative z-10">
          {/* Page Header */}
          <div className="space-y-6">
            <div className="flex items-start justify-between gap-8">
              <div className="space-y-4">
                <h1 className="text-4xl sm:text-5xl md:text-6xl font-bold text-white uppercase tracking-tight">
                  Z.FUN Dashboard
                </h1>
                <p className="text-base sm:text-lg text-white/60">
                  Track verification queues, claims, and platform analytics
                </p>

                {/* Search Bar - Stacked under description */}
                <div className="max-w-2xl relative">
                  <form
                    onSubmit={(e) => {
                      e.preventDefault()
                      if (searchInput.trim()) {
                        window.location.href = `/proof/${searchInput.trim()}`
                      }
                    }}
                    className="relative"
                  >
                    <input
                      type="text"
                      name="requestId"
                      value={searchInput}
                      onChange={(e) => setSearchInput(e.target.value)}
                      onFocus={() => setIsSearchFocused(true)}
                      onBlur={() => setTimeout(() => setIsSearchFocused(false), 100)}
                      placeholder="Search by Request ID..."
                      className="w-full px-4 py-3 pl-12 bg-white/5 border border-white/10 rounded-lg text-white placeholder:text-white/40 focus:outline-none focus:ring-2 focus:ring-white/20 focus:border-white/20 transition-all"
                    />
                    <svg
                      className="absolute left-4 top-1/2 transform -translate-y-1/2 w-5 h-5 text-white/40"
                      fill="none"
                      stroke="currentColor"
                      viewBox="0 0 24 24"
                    >
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                    </svg>
                  </form>

                  {/* Search Result Preview */}
                  {searchInput.trim() && isSearchFocused && (
                    <div className="absolute z-10 w-full mt-1 bg-[rgb(18,18,18)] border border-white/10 rounded overflow-hidden">
                      <button
                        onClick={() => {
                          if (searchInput.trim()) {
                            window.location.href = `/proof/${searchInput.trim()}`
                          }
                        }}
                        className="w-full px-3 py-2 flex items-center gap-2 hover:bg-white/5 transition-colors text-left group"
                      >
                        <div className="flex-1 min-w-0">
                          <div className="text-[10px] text-white/40 mb-0.5 uppercase tracking-wider">Go to proof request</div>
                          <div className="font-mono text-xs text-white/70 group-hover:text-white transition-colors truncate">
                            {searchInput.trim()}
                          </div>
                        </div>
                        <svg className="w-3 h-3 text-white/30 group-hover:text-white/50 transition-colors flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                        </svg>
                      </button>
                    </div>
                  )}
                </div>
              </div>

              {/* Queue Statistics - Right side of header */}
              {/* <div className="flex flex-col gap-4 min-w-[280px]">
                <div className="flex items-center gap-3 justify-end">
                  <h3 className="text-xs text-white/50 uppercase tracking-wider">Queue Statistics</h3>
                  <div className="flex items-center gap-2">
                    <button
                      onClick={() => setQueueView('combined')}
                      className={`text-[10px] font-medium transition-colors ${queueView === 'combined'
                          ? 'text-white'
                          : 'text-white/30 hover:text-white/50'
                        }`}
                    >
                      ZK+TEE
                    </button>
                    <span className="text-white/20">|</span>
                    <button
                      onClick={() => setQueueView('zkOnly')}
                      className={`text-[10px] font-medium transition-colors ${queueView === 'zkOnly'
                          ? 'text-white'
                          : 'text-white/30 hover:text-white/50'
                        }`}
                    >
                      ZK ONLY
                    </button>
                  </div>
                </div>

                <div className="space-y-4">
                  <div className="text-right">
                    <div className="text-[10px] text-white/30 mb-1.5 uppercase tracking-wider">Queue Length</div>
                    <div className="text-4xl font-bold text-white tabular-nums">
                      {queueView === 'combined'
                        ? QUEUES.zkTEE.backlog.toLocaleString()
                        : QUEUES.zkOnly.backlog.toLocaleString()}
                    </div>
                  </div>

                  <div className="text-right">
                    <div className="text-[10px] text-white/30 mb-1.5 uppercase tracking-wider">Avg Verification Time</div>
                    <div className="text-4xl font-bold text-white tabular-nums">
                      {(() => {
                        const parseMinutes = (eta: string): number => {
                          if (eta.includes('minute')) {
                            return parseInt(eta) || 0
                          } else if (eta.includes('second')) {
                            const seconds = parseInt(eta) || 0
                            return seconds / 60
                          }
                          return 0
                        }

                        const currentQueue = queueView === 'combined' ? QUEUES.zkTEE : QUEUES.zkOnly
                        const time = parseMinutes(currentQueue.eta)

                        if (time < 1) {
                          return `${Math.round(time * 60)}s`
                        } else if (time < 60) {
                          return `${Math.round(time)}m`
                        } else {
                          const hours = Math.floor(time / 60)
                          const minutes = Math.round(time % 60)
                          return minutes > 0 ? `${hours}h ${minutes}m` : `${hours}h`
                        }
                      })()}
                    </div>
                  </div>
                </div>
              </div> */}
            </div>
          </div>

          {/* Verification Queue Section */}
          <section className="space-y-6">

            {/* Stats Boxes Grid */}
            <div className="grid gap-4 grid-cols-2">
              {/* Cumulative Proofs */}
              <div className="border border-white/10 bg-white/[0.02] backdrop-blur-sm p-4 flex flex-col gap-4 min-w-0">
                <div className="space-y-3">
                  <h3 className="text-xs text-white/50 uppercase tracking-wider">Cumulative Proofs</h3>
                  <div className="flex items-baseline gap-2">
                    <p className="text-5xl font-bold text-white tabular-nums">
                      {isLoading ? '...' : cumulativeProofData.length > 0
                        ? cumulativeProofData[cumulativeProofData.length - 1].total_proofs.toLocaleString()
                        : '0'}
                    </p>
                    {!isLoading && cumulativeProofData.length > 0 && (
                      <span className="inline-flex items-center gap-1 text-xs font-medium text-emerald-400">
                        <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 10l7-7m0 0l7 7m-7-7v18" />
                        </svg>
                        +{cumulativeProofData[cumulativeProofData.length - 1].daily_increase}
                      </span>
                    )}
                  </div>
                </div>

                {/* Chart */}
                <div
                  ref={chartRef}
                  className="h-16 select-none chart-no-select"
                  style={{
                    userSelect: 'none',
                    WebkitUserSelect: 'none',
                    MozUserSelect: 'none',
                    msUserSelect: 'none',
                    overflow: 'visible',
                  }}
                  onDragStart={(e) => e.preventDefault()}
                >
                  <ResponsiveContainer width="100%" height="100%">
                    <LineChart
                      data={cumulativeProofData.map(d => ({ ...d, totalProofs: d.total_proofs }))}
                      margin={{ top: 0, right: 0, left: 0, bottom: 0 }}
                      style={{
                        userSelect: 'none',
                        WebkitUserSelect: 'none',
                        MozUserSelect: 'none',
                        msUserSelect: 'none',
                        outline: 'none',
                      }}
                    >
                      <YAxis
                        hide
                        domain={['dataMin', 'dataMax']}
                      />
                      <Tooltip
                        contentStyle={{
                          backgroundColor: 'rgba(0, 0, 0, 0.9)',
                          border: '1px solid rgba(255, 255, 255, 0.1)',
                          borderRadius: '8px',
                          padding: '12px'
                        }}
                        labelStyle={{
                          color: 'rgba(255, 255, 255, 0.6)',
                          fontSize: '12px',
                          marginBottom: '4px'
                        }}
                        itemStyle={{
                          color: '#f3b724',
                          fontSize: '14px',
                          fontWeight: 'bold'
                        }}
                        formatter={(value: any) => [value.toLocaleString() + ' proofs']}
                        labelFormatter={(label, payload) => {
                          if (payload && payload.length > 0 && payload[0].payload) {
                            return payload[0].payload.full_date
                          }
                          return label
                        }}
                      />
                      <Line
                        type="monotone"
                        dataKey="totalProofs"
                        stroke="#f3b724"
                        strokeWidth={2}
                        dot={false}
                        name="Total Proofs"
                      />
                    </LineChart>
                  </ResponsiveContainer>
                </div>
              </div>

              {/* Cumulative Amounts */}
              <div className="border border-white/10 bg-white/[0.02] backdrop-blur-sm p-4 flex flex-col gap-4 min-w-0">
                <div className="space-y-3">
                  <h3 className="text-xs text-white/50 uppercase tracking-wider">Cumulative Value Proven</h3>
                  <div className="flex items-baseline gap-2">
                    <p className="text-5xl font-bold text-white tabular-nums">
                      {isLoading ? '...' : cumulativeAmountsData.length > 0
                        ? cumulativeAmountsData[cumulativeAmountsData.length - 1].amount.toLocaleString(undefined, { minimumFractionDigits: 0, maximumFractionDigits: 3 })
                        : '0'}
                    </p>
                    {!isLoading && cumulativeAmountsData.length > 0 && (
                      <span className="inline-flex items-center gap-1 text-xs font-medium text-emerald-400">
                        <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 10l7-7m0 0l7 7m-7-7v18" />
                        </svg>
                        {(() => {
                          const first = cumulativeAmountsData[0]?.amount || 0
                          const last = cumulativeAmountsData[cumulativeAmountsData.length - 1]?.amount || 0
                          const growth = first > 0 ? ((last - first) / first * 100) : 0
                          return `${growth.toFixed(1)}%`
                        })()}
                      </span>
                    )}
                  </div>
                </div>
                <div
                  className="h-16 select-none chart-no-select"
                  style={{
                    userSelect: 'none',
                    WebkitUserSelect: 'none',
                    MozUserSelect: 'none',
                    msUserSelect: 'none',
                    overflow: 'visible',
                  }}
                  onDragStart={(e) => e.preventDefault()}
                >
                  <ResponsiveContainer width="100%" height="100%">
                    <LineChart
                      data={cumulativeAmountsData}
                      margin={{ top: 0, right: 0, left: 0, bottom: 0 }}
                      style={{
                        userSelect: 'none',
                        WebkitUserSelect: 'none',
                        MozUserSelect: 'none',
                        msUserSelect: 'none',
                        outline: 'none',
                      }}
                    >
                      <YAxis
                        hide
                        domain={['dataMin', 'dataMax']}
                      />
                      <Tooltip
                        contentStyle={{
                          backgroundColor: 'rgba(0, 0, 0, 0.9)',
                          border: '1px solid rgba(255, 255, 255, 0.1)',
                          borderRadius: '8px',
                          padding: '12px'
                        }}
                        labelStyle={{
                          color: 'rgba(255, 255, 255, 0.6)',
                          fontSize: '12px',
                          marginBottom: '4px'
                        }}
                        itemStyle={{
                          color: '#f3b724',
                          fontSize: '14px',
                          fontWeight: 'bold'
                        }}
                        formatter={(value: any) => [value.toLocaleString(undefined, { maximumFractionDigits: 2 }) + ' ZEC']}
                        labelFormatter={(label, payload) => {
                          if (payload && payload.length > 0 && payload[0].payload) {
                            return payload[0].payload.full_date
                          }
                          return label
                        }}
                      />
                      <Line
                        type="monotone"
                        dataKey="amount"
                        stroke="#f3b724"
                        strokeWidth={2}
                        dot={false}
                        name="Total Amounts"
                      />
                    </LineChart>
                  </ResponsiveContainer>
                </div>
              </div>
            </div>
          </section>

          {/* Live Transaction Stream */}
          <section className="border border-white/10 bg-white/[0.02] backdrop-blur-sm p-4">
            <div className="flex items-center justify-between mb-3">
              <h3 className="text-xs text-white/80 uppercase tracking-wider">Activity</h3>
              <span className="text-[10px] text-white/30 uppercase tracking-wider">Recent</span>
            </div>

            <div className="relative h-[28rem] overflow-hidden">
              <div className="absolute inset-0 overflow-y-auto scrollbar-hide">
                <table className="w-full">
                  <thead className="sticky top-0 bg-white/[0.02] backdrop-blur-sm border-b border-white/10">
                    <tr>
                      <th className="text-left text-[10px] text-white/40 uppercase tracking-wider px-3 py-2">Request ID</th>
                      <th className="text-left text-[10px] text-white/40 uppercase tracking-wider px-3 py-2">Time</th>
                      <th className="text-left text-[10px] text-white/40 uppercase tracking-wider px-3 py-2">Amount</th>
                    </tr>
                  </thead>
                  <tbody>
                    {transactions.length === 0 ? (
                      <tr>
                        <td colSpan={3} className="text-center py-6 text-white/40">
                          <p className="text-sm">{isLoading ? 'Loading...' : 'No transactions yet'}</p>
                        </td>
                      </tr>
                    ) : (
                      transactions.map((tx, index) => {
                        return <TransactionRow key={tx.id} tx={tx} index={index} />
                      })
                    )}
                  </tbody>
                </table>
              </div>

              {/* Bottom fade effect */}
              <div className="absolute bottom-0 left-0 right-0 h-16 bg-gradient-to-t from-white/[0.02] to-transparent pointer-events-none"></div>
            </div>

          </section>
        </div>

        <Footer />
      </div>
    </>
  )
}

function TransactionRow({ tx, index }: { tx: Transaction; index: number }) {
  const [copied, setCopied] = useState(false)
  const timeAgo = useMemo(() => Math.floor((Date.now() / 1000) - tx.timestamp), [tx.timestamp])
  const timeDisplay = timeAgo < 60 ? `${timeAgo}s` : timeAgo < 3600 ? `${Math.floor(timeAgo / 60)}m` : `${Math.floor(timeAgo / 3600)}h`

  const handleCopy = () => {
    navigator.clipboard.writeText(tx.id)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  return (
    <tr
      className={`border-b border-white/10 hover:bg-white/[0.03] transition-colors ${index === 0 ? '[animation:slide-in_0.3s_ease-out]' : ''}`}
    >
      {/* Request ID */}
      <td className="text-xs text-white/50 font-mono px-3 py-2.5">
        <div className="flex items-center gap-2 group">
          <Link href={`/proof/${tx.id}`} className="hover:text-white/70 transition-colors cursor-pointer px-2 py-1 -mx-2 -my-1 rounded border border-transparent group-hover:border-white/20 group-hover:bg-white/5">
            {tx.id}
          </Link>
          <div className="relative group/tooltip">
            <button
              onClick={handleCopy}
              className="text-white/30 hover:text-white/60 transition-colors opacity-0 group-hover:opacity-100 mx-1"
            >
              {copied ? (
                <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                </svg>
              ) : (
                <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
                </svg>
              )}
            </button>
            <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-1 px-2 py-1 bg-black text-white text-[10px] rounded whitespace-nowrap opacity-0 group-hover/tooltip:opacity-100 pointer-events-none transition-opacity">
              {copied ? 'Copied' : 'Copy Request ID'}
            </div>
          </div>
        </div>
      </td>

      {/* Time */}
      <td className="text-xs text-white/30 px-3 py-2.5">
        {timeDisplay}
      </td>

      {/* Amount */}
      <td className="text-xs text-white/60 px-3 py-2.5">
        <div className="flex items-center gap-1.5">
          {tx.amount !== undefined && tx.amount !== null ? (
            <>
              <span>{tx.amount < 0.01 ? tx.amount.toFixed(3) : tx.amount.toFixed(2)}</span>
              <img src="/zec-logo.svg" alt="ZEC" className="w-3.5 h-3.5 opacity-60" />
            </>
          ) : (
            <span className="text-white/30">-</span>
          )}
        </div>
      </td>
    </tr>
  )
}
