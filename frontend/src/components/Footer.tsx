"use client"

export function Footer() {
  return (
    <footer className="relative border-t border-white/5 bg-[rgb(18,18,18)] mt-auto">
      {/* Top glow */}
      <div className="absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-white/10 to-transparent"></div>
      
      <div className="w-full px-4 sm:px-6 lg:px-8 py-3">
        <div className="flex flex-col sm:flex-row items-center justify-between gap-4">
          {/* Left side - Text links */}
          <nav className="flex flex-wrap items-center justify-center sm:justify-start gap-3 sm:gap-4">
            <a
              href="/terms"
              className="text-xs text-white/40 hover:text-white transition-all duration-300 hover:drop-shadow-[0_0_12px_rgba(255,255,255,0.6)] uppercase tracking-wider"
            >
              Terms
            </a>
            <a
              href="https://z.fun/whitepaper.pdf"
              target="_blank"
              rel="noreferrer"
              className="text-xs text-white/40 hover:text-white transition-all duration-300 hover:drop-shadow-[0_0_12px_rgba(255,255,255,0.6)] uppercase tracking-wider"
            >
              Whitepaper
            </a>
          </nav>

          {/* Right side - Social icons */}
          <div className="flex items-center gap-3">
            <a
              href="https://github.com/Monero-Chan-Foundation"
              target="_blank"
              rel="noreferrer"
              className="text-white/40 hover:text-white transition-all duration-300 hover:drop-shadow-[0_0_12px_rgba(255,255,255,0.6)]"
              aria-label="GitHub"
            >
              <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
                <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
              </svg>
            </a>
            <a 
              href="https://twitter.com/zdotfun" 
              target="_blank" 
              rel="noreferrer" 
              className="text-white/40 hover:text-white transition-all duration-300 hover:drop-shadow-[0_0_12px_rgba(255,255,255,0.6)]"
              aria-label="X (Twitter)"
            >
              <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
                <path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z" />
              </svg>
            </a>
            <a
              href="https://t.me/MoneroChanSOL"
              target="_blank"
              rel="noreferrer"
              className="text-white/40 hover:text-white transition-all duration-300 hover:drop-shadow-[0_0_12px_rgba(255,255,255,0.6)]"
              aria-label="Telegram"
            >
              <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 100 100">
                <path d="M88.723,12.142C76.419,17.238,23.661,39.091,9.084,45.047c-9.776,3.815-4.053,7.392-4.053,7.392s8.345,2.861,15.499,5.007c7.153,2.146,10.968-0.238,10.968-0.238l33.62-22.652c11.922-8.107,9.061-1.431,6.199,1.431c-6.199,6.2-16.452,15.975-25.036,23.844c-3.815,3.338-1.908,6.199-0.238,7.63c6.199,5.246,23.129,15.976,24.082,16.691c5.037,3.566,14.945,8.699,16.452-2.146c0,0,5.961-37.435,5.961-37.435c1.908-12.637,3.815-24.321,4.053-27.659C97.307,8.804,88.723,12.142,88.723,12.142z"/>
              </svg>
            </a>
          </div>
        </div>
      </div>
    </footer>
  )
}

2