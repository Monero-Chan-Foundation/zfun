"use client"

import React, { useState } from "react"
import type { FaqItem } from "@/lib/types"
import { cn } from "@/lib/utils"

const FAQ_ITEMS: FaqItem[] = [
  {
    question: "What is this?",
    paragraphs: [
      "ZFUN is an experimental token leveraging zero knowledge proofs to verify ZEC holdings. There is no planned utility for the token.",
    ],
  },
  {
    question: "How does this work?",
    paragraphs: [
      "Holders must generate a zero knowledge proof of their holdings. The user submits the input to the TEE prover which generates a proof declaring the total number of ZEC holdings. The Zfun backend receives the proof and records the outputs and corresponding Solana address.",
      "The zero knowledge proof only proves that the holder has a certain amount of ZEC holdings. It does not connect the holder's Solana address to their shielded holdings or the source of their shielded holdings. However, the public commitment does contain hashes of transparent UTXOs."
    ],
  },
  {
    question: "Does this leak my shielded transaction data?",
    paragraphs: [
      "No. The resulting ZK proof does not expose the user's private Zcash addresses. There are several layers of protection towards maintaining privacy of your shielded transactions.",
      "The private data that is exported from your wallet and used to prove your holdings is read-only. It does not have the secrets necessary to spend tokens, only the viewing keys which allow read-only access to your wallet.",
      <p key="p-1">The private data is then processed on your computer in the frontend, using Javascript and WebAssembly code that is fully open source. You can view this code <a href="https://github.com/Monero-Chan-Foundation/zfun" target="_blank" rel="noreferrer" className="text-white underline underline-offset-4 hover:text-white/80 transition-colors">here</a>, verify its correctness, and run it locally if you wish.</p>,
      <p key="p-2">The frontend creates a proof input which is sent securely using zkTLS directly to a TEE prover, powered by <a href="https://github.com/monerochan-labs/monerochan.rs" target="_blank" rel="noreferrer" className="text-white underline underline-offset-4 hover:text-white/80 transition-colors">Monerochan Private Proving</a>. Because it is encrypted, only the verifiable prover running in the TEE can decrypt the data and use it to generate a Monerochan proof. More info: <a href="https://github.com/monerochan-labs/monerochan.rs" target="_blank" rel="noreferrer" className="text-white underline underline-offset-4 hover:text-white/80 transition-colors">monerochan.rs</a>.</p>,
      "The prover generates a zero knowledge proof declaring proof of a certain amount of ZEC. It also includes unique identifiers used to prevent repeat claims for the same holdings. These identifiers are unique to your holdings but cannot be linked back to your shielded notes, meaning your Solana address is not linked back to your shielded holdings.",
      "Z.fun's servers have the proof ID corresponding to your proof input which makes it possible to track the status of your proof and download the result from the TEE prover. It does not enable access to the shielded input data."
    ],
  },
  {
    question: "I still don't trust it. Can I run it fully locally?",
    paragraphs: [
      <p key="p-3">If you wish, you can run the open source <a href="https://github.com/Monero-Chan-Foundation/zfun" target="_blank" rel="noreferrer" className="text-white underline underline-offset-4 hover:text-white/80 transition-colors">Rust proving tool</a> locally which runs the entire witness generation and proving flow locally, meaning that all of your read-only wallet data never leaves your computer. The local tool generates a proof for the exact same verifier program as this interface. You can also inspect the frontend source code and run it fully locally if you don&apos;t trust this site.</p>,
    ],
  },
  {
    question: "Is this open source?",
    paragraphs: [
      <p key="p-4">Yes! All of the code for ZFUN is open source and publicly available on <a href="https://github.com/Monero-Chan-Foundation/zfun" target="_blank" rel="noreferrer" className="text-white underline underline-offset-4 hover:text-white/80 transition-colors">GitHub</a>. This includes the frontend web application, the Rust proving tool, and all related infrastructure. You can audit the code, verify its correctness, and even contribute improvements.</p>,
      "The open source nature of the project ensures transparency and allows anyone to verify that the system operates as claimed without compromising user privacy.",
    ],
  },
  {
    question: "Why don't my recent transactions appear in the exported data?",
    paragraphs: [
      "Your transactions must be finalized on the Zcash blockchain before they will appear in your wallet's exported data. This means the transaction needs to receive sufficient confirmations.",
      "If you've made a recent transaction, wait for it to be fully confirmed on the blockchain before exporting your wallet database.",
      "Additionally, your holdings must have been present in your wallet at the snapshot height in order to be verified. Users with wallet activity from after the snapshot may still be able to generate a ZK proof of their wallet's existence."
    ],
  },
]

interface FAQProps {
  items?: FaqItem[]
}

export function FAQ({ items = FAQ_ITEMS }: FAQProps) {
  const [openFaqIndex, setOpenFaqIndex] = useState<number | null>(null)

  return (
    <section className="space-y-8 mt-8">
      <div>
        <h2 className="font-inria-serif text-3xl text-white">Frequently Asked Questions</h2>
        <div className="mt-3 h-px w-64 bg-gradient-to-r from-white/30 via-white/10 to-transparent"></div>
      </div>
      <dl className="divide-y divide-white/10 overflow-hidden border border-white/10 bg-gradient-to-br from-white/[0.02] to-transparent">
        {items.map((item, index) => {
          const isOpen = openFaqIndex === index
          return (
            <div key={item.question} className="group">
              <dt>
                <button
                  type="button"
                  onClick={() => setOpenFaqIndex(isOpen ? null : index)}
                  aria-expanded={isOpen}
                  className="flex w-full items-center justify-between gap-4 px-6 py-5 text-left transition-all duration-200 hover:bg-white/[0.03]"
                >
                  <span className="text-base font-medium text-white/90">{item.question}</span>
                  <svg
                    className={cn(
                      "h-5 w-5 text-white/60 transition-transform duration-200 flex-shrink-0",
                      isOpen && "rotate-180"
                    )}
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                  >
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
                  </svg>
                </button>
              </dt>
              {isOpen && (
                <dd className="space-y-4 border-t border-white/10 bg-white/[0.01] px-6 py-6 text-sm text-white/70 leading-relaxed">
                  {item.paragraphs?.map((paragraph, paragraphIndex) => (
                    <div key={paragraphIndex} className="text-white/70">
                      {typeof paragraph === 'string' ? <p>{paragraph}</p> : paragraph}
                    </div>
                  ))}

                  {item.orderedList && item.orderedList.length > 0 && (
                    <div className="space-y-3">
                      {item.orderedListTitle && (
                        <p className="text-[10px] uppercase tracking-[0.3em] text-white/40">{item.orderedListTitle}</p>
                      )}
                      <ol className="list-decimal space-y-2 pl-6 text-white/70">
                        {item.orderedList.map((step, stepIndex) => (
                          <li key={stepIndex} className="pl-2">{step}</li>
                        ))}
                      </ol>
                    </div>
                  )}

                  {item.unorderedList && item.unorderedList.length > 0 && (
                    <div className="space-y-3">
                      {item.unorderedListTitle && (
                        <p className="text-[10px] uppercase tracking-[0.3em] text-white/40">{item.unorderedListTitle}</p>
                      )}
                      <ul className="space-y-2 pl-6 text-white/70">
                        {item.unorderedList.map((entry, entryIndex) => (
                          <li key={entryIndex} className="flex gap-3">
                            <span className="text-[#f3b724] mt-1.5">•</span>
                            <span>{entry}</span>
                          </li>
                        ))}
                      </ul>
                    </div>
                  )}

                  {item.concluding && (
                    <div className="text-white/70">
                      {typeof item.concluding === 'string' ? <p>{item.concluding}</p> : item.concluding}
                    </div>
                  )}
                </dd>
              )}
            </div>
          )
        })}
      </dl>
    </section>
  )
}
