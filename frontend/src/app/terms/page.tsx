"use client"

import { Navbar } from "../../components/navbar"
import { Footer } from "../../components/Footer"

export default function TermsPage() {
  return (
    <>
      <Navbar />
      <div className="min-h-screen bg-[#121212] text-slate-100">
        <div className="mx-auto max-w-4xl px-6 py-12">
          
          <h1 className="text-3xl font-bold text-white mb-2">Terms & Legal Documentation</h1>
          <p className="text-sm text-white/50 mb-8">Last Updated: October 30, 2025</p>


          
          <div className="prose prose-invert prose-sm max-w-none space-y-12">

            {/* PART I: TERMS OF SERVICE */}
            <section id="terms">
              <h2 className="text-2xl font-bold text-white border-b border-white/10 pb-2 mb-6">Part I: Terms of Service</h2>
              
              <h3 className="text-lg font-semibold text-white mt-6 mb-2">1. Acceptance of Terms</h3>
              <p className="text-sm text-white/70 mb-2">
                By accessing or using Z.FUN ("the Platform", "we", "us", "our"), you agree to be bound by these Terms of Service. If you do not agree to all terms, you must not use this service.
              </p>
              <p className="text-sm text-white/70 mb-4">
                These terms apply to all users, including visitors, registered users, and anyone who accesses the platform in any capacity.
              </p>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">2. Service Description</h3>
              <p className="text-sm text-white/70 mb-2">
                Z.FUN provides zero-knowledge proof generation services for Zcash shielded balance verification. The platform enables users to:
              </p>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li>Upload Zcash wallet data for verification</li>
                <li>Generate cryptographic proofs of shielded balances using Trusted Execution Environments (TEE)</li>
                <li>Submit proofs to blockchain smart contracts</li>
              </ul>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">3. Eligibility</h3>
              <p className="text-sm text-white/70 mb-2">To use this service, you must:</p>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li>Be at least 18 years old or the age of majority in your jurisdiction</li>
                <li>Have the legal capacity to enter into binding contracts</li>
                <li>Not be located in a prohibited jurisdiction</li>
                <li>Comply with all applicable laws and regulations</li>
                <li>Not be subject to sanctions or export restrictions</li>
              </ul>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">4. User Responsibilities</h3>
              <p className="text-sm text-white/70 mb-2">You are solely responsible for:</p>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li>Maintaining the security of your wallet files and private keys</li>
                <li>All activity that occurs through your use of the service</li>
                <li>Ensuring your use complies with applicable laws</li>
                <li>Understanding the risks associated with cryptocurrency and cryptographic proofs</li>
                <li>Backing up your wallet data before uploading</li>
                <li>Verifying the authenticity of the platform before use</li>
              </ul>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">5. Prohibited Activities</h3>
              <p className="text-sm text-white/70 mb-2">You may not:</p>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li>Use the service for any illegal purpose or in violation of any laws</li>
                <li>Attempt to manipulate, exploit, or game the verification system</li>
                <li>Submit fraudulent proofs or false information</li>
                <li>Reverse engineer, decompile, or disassemble any part of the platform</li>
                <li>Use automated systems or bots to access the service</li>
                <li>Interfere with or disrupt the platform's infrastructure</li>
                <li>Upload malicious code or attempt to compromise security</li>
                <li>Impersonate others or provide misleading information</li>
              </ul>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">6. No Warranties</h3>
              <p className="text-sm text-white/70 mb-2">
                THE SERVICE IS PROVIDED "AS IS" AND "AS AVAILABLE" WITHOUT WARRANTIES OF ANY KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO:
              </p>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li>Warranties of merchantability or fitness for a particular purpose</li>
                <li>Accuracy, reliability, or completeness of results</li>
                <li>Uninterrupted or error-free operation</li>
                <li>Security or privacy guarantees</li>
                <li>Compatibility with your systems or requirements</li>
              </ul>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">7. Limitation of Liability</h3>
              <p className="text-sm text-white/70 mb-4">
                TO THE MAXIMUM EXTENT PERMITTED BY LAW, WE SHALL NOT BE LIABLE FOR ANY INDIRECT, INCIDENTAL, SPECIAL, CONSEQUENTIAL, OR PUNITIVE DAMAGES, OR ANY LOSS OF PROFITS OR REVENUES, WHETHER INCURRED DIRECTLY OR INDIRECTLY, OR ANY LOSS OF DATA, USE, GOODWILL, OR OTHER INTANGIBLE LOSSES.
              </p>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">8. Modifications to Service</h3>
              <p className="text-sm text-white/70 mb-2">We reserve the right to:</p>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li>Modify or discontinue the service at any time without notice</li>
                <li>Change features, functionality, or eligibility requirements</li>
                <li>Impose limits on certain features or restrict access</li>
                <li>Update these terms at any time</li>
              </ul>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">9. Governing Law</h3>
              <p className="text-sm text-white/70 mb-4">
                These terms shall be governed by and construed in accordance with applicable law. Any disputes shall be resolved through binding arbitration in accordance with established arbitration rules.
              </p>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">10. Contact Information</h3>
              <p className="text-sm text-white/70 mb-4">
                For questions about these terms, please contact us through our official communication channels listed on the platform.
              </p>
            </section>

            {/* PART II: PRIVACY POLICY */}
            <section id="privacy">
              <h2 className="text-2xl font-bold text-white border-b border-white/10 pb-2 mb-6">Part II: Privacy Policy</h2>

              
              <h3 className="text-lg font-semibold text-white mt-6 mb-2">11. Information We Collect</h3>
              
              <p className="font-semibold text-white/90 text-sm mt-4 mb-2">11.1 Wallet Data (Temporary)</p>
              <p className="text-sm text-white/70 mb-2">When you upload your wallet for verification:</p>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li>Wallet files are processed within the Trusted Execution Environment (TEE)</li>
                <li>Data is used only for proof generation</li>
                <li>Wallet data is not permanently stored on our servers</li>
                <li>TEE memory is cleared after processing</li>
              </ul>

              <p className="font-semibold text-white/90 text-sm mt-4 mb-2">11.2 Request Metadata</p>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li>Request timestamps and queue IDs</li>
                <li>Verification status and results</li>
                <li>Solana wallet addresses</li>
                <li>Proof submission transactions</li>
              </ul>

              <p className="font-semibold text-white/90 text-sm mt-4 mb-2">11.3 Technical Data</p>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li>IP addresses and connection information</li>
                <li>Browser type and version</li>
                <li>Device information</li>
                <li>Error logs and diagnostic data</li>
              </ul>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">12. How We Use Information</h3>
              <p className="text-sm text-white/70 mb-2">We use collected information to:</p>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li>Generate zero-knowledge proofs of your shielded balances</li>
                <li>Manage verification queues and process requests</li>
                <li>Improve platform performance and reliability</li>
                <li>Debug issues and provide technical support</li>
                <li>Prevent fraud and abuse</li>
                <li>Comply with legal obligations</li>
              </ul>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">13. Data Sharing & Third Parties</h3>
              <p className="text-sm text-white/70 mb-2">We may share data with:</p>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li><strong>Infrastructure Providers:</strong> Cloud hosting, TEE services (may have access to metadata)</li>
                <li><strong>Blockchain Networks:</strong> Proof submissions are public on-chain</li>
                <li><strong>Analytics Services:</strong> Aggregated, anonymized usage statistics</li>
                <li><strong>Legal Authorities:</strong> When required by law or to protect our rights</li>
              </ul>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">14. TEE Security & Privacy</h3>
              <p className="text-sm text-white/70 mb-2">
                Trusted Execution Environments provide hardware-based isolation, but:
              </p>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li>TEE security depends on manufacturer implementations (Intel SGX, AMD SEV, etc.)</li>
                <li>Sophisticated attacks may potentially compromise TEE security</li>
                <li>We cannot guarantee absolute protection against all threats</li>
                <li>TEE attestation provides verifiable proof of code execution</li>
              </ul>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">15. Data Retention</h3>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li><strong>Wallet Data:</strong> Not permanently stored; cleared from TEE after processing</li>
                <li><strong>Request Metadata:</strong> Retained for operational purposes and analytics</li>
                <li><strong>Blockchain Data:</strong> Permanently public on distributed ledgers</li>
                <li><strong>Logs:</strong> Retained for debugging and security monitoring (time-limited)</li>
              </ul>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">16. Your Privacy Rights</h3>
              <p className="text-sm text-white/70 mb-2">Depending on your jurisdiction, you may have rights to:</p>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li>Access personal information we hold about you</li>
                <li>Request correction of inaccurate data</li>
                <li>Request deletion of your data (subject to legal obligations)</li>
                <li>Object to certain processing activities</li>
                <li>Data portability</li>
              </ul>
              <p className="text-sm text-white/70 mb-4">
                Note that blockchain data cannot be deleted due to the immutable nature of distributed ledgers.
              </p>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">17. Security Measures</h3>
              <p className="text-sm text-white/70 mb-2">We implement security measures including:</p>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li>TEE-based isolation for sensitive processing</li>
                <li>Encryption in transit (TLS/SSL)</li>
                <li>Access controls and authentication</li>
                <li>Regular security audits and monitoring</li>
                <li>Incident response procedures</li>
              </ul>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">18. Cookies & Tracking</h3>
              <p className="text-sm text-white/70 mb-2">We may use:</p>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li>Essential cookies for platform functionality</li>
                <li>Analytics cookies to understand usage patterns</li>
                <li>Local storage for user preferences</li>
              </ul>
              <p className="text-sm text-white/70 mb-4">
                You can control cookies through your browser settings, though this may affect functionality.
              </p>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">19. International Data Transfers</h3>
              <p className="text-sm text-white/70 mb-4">
                Your data may be transferred to and processed in countries other than your own. We take steps to ensure adequate protection, but international transfers may expose data to different legal frameworks.
              </p>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">20. Children's Privacy</h3>
              <p className="text-sm text-white/70 mb-4">
                This service is not intended for individuals under 18 years of age. We do not knowingly collect information from children.
              </p>
            </section>

            {/* PART III: RISK DISCLAIMERS */}
            <section id="risks">
              <h2 className="text-2xl font-bold text-white border-b border-white/10 pb-2 mb-6">Part III: Risk Disclaimers</h2>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">21. Experimental Software Risks</h3>
              <p className="text-sm text-white/70 mb-2">
                Z.FUN uses cutting-edge cryptographic technologies including:
              </p>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li><strong>Zero-Knowledge Proofs (ZK):</strong> Advanced cryptographic protocols that may have undiscovered vulnerabilities</li>
                <li><strong>Trusted Execution Environments (TEE):</strong> Hardware security features that depend on manufacturer implementations</li>
                <li><strong>Novel Cryptographic Circuits:</strong> Custom-designed verification logic that has limited real-world testing</li>
              </ul>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">22. Cryptocurrency Risks</h3>
              
              <p className="font-semibold text-white/90 text-sm mt-4 mb-2">22.1 General Cryptocurrency Risks</p>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li><strong>Volatility:</strong> Cryptocurrency prices can fluctuate wildly</li>
                <li><strong>Irreversible Transactions:</strong> Blockchain transactions cannot be reversed</li>
                <li><strong>Loss of Funds:</strong> Lost private keys mean permanent loss of access</li>
                <li><strong>Regulatory Changes:</strong> Laws governing cryptocurrencies may change unpredictably</li>
                <li><strong>Market Risk:</strong> Tokens may lose all value</li>
              </ul>

              <p className="font-semibold text-white/90 text-sm mt-4 mb-2">22.2 Zcash-Specific Risks</p>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li><strong>Privacy Technology:</strong> Shielded transactions use complex cryptography that may have vulnerabilities</li>
                <li><strong>Regulatory Scrutiny:</strong> Privacy coins face heightened regulatory attention in many jurisdictions</li>
                <li><strong>Exchange Delistings:</strong> Some exchanges have delisted privacy coins due to regulatory pressure</li>
                <li><strong>Limited Adoption:</strong> Shielded transactions have lower adoption than transparent ones</li>
              </ul>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">23. Security & Privacy Risks</h3>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li><strong>TEE Vulnerabilities:</strong> Hardware security may be compromised by sophisticated attacks or manufacturer flaws</li>
                <li><strong>Metadata Leakage:</strong> IP addresses, timestamps, and usage patterns may be logged and potentially deanonymize users</li>
                <li><strong>Third-Party Risk:</strong> Infrastructure providers may have access to metadata or logs</li>
                <li><strong>Temporary Wallet Access:</strong> Your wallet data is processed within the TEE during verification</li>
                <li><strong>Blockchain Transparency:</strong> Proof submissions and claims are publicly visible on-chain</li>
                <li><strong>Implementation Bugs:</strong> Software bugs may expose data or compromise security</li>
              </ul>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">24. Technical Limitations</h3>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li><strong>Proof Generation May Fail:</strong> ZK proof generation is computationally intensive and may time out or fail</li>
                <li><strong>No Uptime Guarantees:</strong> The service may be unavailable at any time without notice</li>
                <li><strong>Queue Delays:</strong> High demand may result in long wait times</li>
                <li><strong>Snapshot Accuracy:</strong> Balance proofs are only accurate at the specific snapshot block height</li>
                <li><strong>Incomplete Coverage:</strong> Not all transaction types or wallet configurations may be supported</li>
                <li><strong>Hardware Requirements:</strong> TEE functionality requires compatible hardware that may not be available</li>
              </ul>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">25. Regulatory & Compliance Risks</h3>
              <p className="text-sm text-white/70 mb-2">
                Cryptocurrency regulations vary widely by jurisdiction and are rapidly evolving. You are responsible for:
              </p>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li>Determining whether using this service is legal in your jurisdiction</li>
                <li>Complying with all applicable laws, including KYC/AML requirements</li>
                <li>Reporting cryptocurrency holdings and transactions for tax purposes</li>
                <li>Understanding securities laws that may apply to token distributions</li>
                <li>Complying with privacy and data protection regulations</li>
              </ul>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">26. No Liability</h3>
              <p className="text-sm text-white/70 mb-2">
                Z.FUN and its operators are not liable for any losses or damages arising from use of this platform, including but not limited to:
              </p>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li>Lost, stolen, or compromised funds or cryptocurrency</li>
                <li>Incorrect verification results or proof failures</li>
                <li>Privacy breaches or data exposure</li>
                <li>Service interruptions or downtime</li>
                <li>Token devaluation or market losses</li>
                <li>Tax liabilities or legal consequences</li>
                <li>Any other direct or indirect damages</li>
              </ul>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">27. No Financial or Legal Advice</h3>
              <p className="text-sm text-white/70 mb-2">
                Nothing on this platform constitutes financial, investment, legal, or tax advice. We strongly recommend:
              </p>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li>Consulting with a financial advisor before making investment decisions</li>
                <li>Seeking legal counsel to understand regulatory obligations</li>
                <li>Working with a tax professional to understand tax implications</li>
                <li>Conducting your own research (DYOR) before using any cryptocurrency service</li>
              </ul>
            </section>

            {/* PART IV: FINAL ACCEPTANCE */}
            <section id="acceptance">
              <h2 className="text-2xl font-bold text-white border-b border-white/10 pb-2 mb-6">Part IV: Final Acceptance</h2>

              <h3 className="text-lg font-semibold text-white mt-6 mb-2">28. Final Acceptance</h3>
              <p className="text-sm text-white/70 mb-2">
                By using Z.FUN and participating in the verification process, you acknowledge that you have read, understood, and agree to be bound by all parts of these Terms & Legal Documentation, including:
              </p>
              <ul className="list-disc pl-6 text-sm text-white/70 space-y-1 mb-4">
                <li>Part I: Terms of Service</li>
                <li>Part II: Privacy Policy</li>
                <li>Part III: Risk Disclaimers</li>
              </ul>
            </section>

          </div>

          {/* ==================== END EDITABLE CONTENT ==================== */}

        </div>

        <Footer />
      </div>
    </>
  )
}
