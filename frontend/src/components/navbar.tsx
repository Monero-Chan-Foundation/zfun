"use client";

import { usePathname } from "next/navigation";
import { useState } from "react";
import Link from "next/link";

const navLinks: { href: string; label: string }[] = [
  { href: "/", label: "Dashboard" },
  { href: "/verify", label: "Verify" },
];

export function Navbar() {
  const pathname = usePathname();
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);

  return (
    <header className="sticky top-0 z-50 backdrop-blur-md overflow-visible relative border-b border-white/5" style={{ background: 'linear-gradient(180deg,rgb(18,18,18),rgba(18,18,18,.95))' }}>
      {/* Bottom glow */}
      <div className="absolute inset-x-0 bottom-0 h-px bg-gradient-to-r from-transparent via-white/10 to-transparent"></div>
      
      <div className="flex w-full items-center justify-between px-4 sm:px-6 py-3 sm:py-2 lg:px-8 overflow-visible min-w-0">
        {/* Left side - Logo */}
        <div className="relative flex items-center flex-shrink-0 min-w-0">
          <Link href="/" className="relative group overflow-visible flex-shrink-0 flex items-center justify-center">
            {/* Animated shield on hover */}
            <svg 
              className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-[40px] h-[40px] sm:w-[48px] sm:h-[48px] opacity-0 group-hover:opacity-100 transition-opacity duration-300 pointer-events-none z-10"
              viewBox="0 0 100 100" 
              fill="none" 
              xmlns="http://www.w3.org/2000/svg"
            >
              <path
                d="M50 10L80 25V50C80 67 65 82 50 90C35 82 20 67 20 50V25L50 10Z"
                stroke="white"
                strokeWidth="2"
                fill="none"
                strokeDasharray="280"
                strokeDashoffset="280"
                strokeLinejoin="round"
                strokeLinecap="round"
                className="group-hover:animate-[drawShield_0.6s_ease-out_forwards]"
              />
            </svg>
            <img
              src="/ZLOGO-SVG.svg"
              alt="Z.fun"
              className="relative z-20 h-[20px] sm:h-[24px] w-auto cursor-pointer transition-all duration-300 group-hover:scale-110 group-hover:drop-shadow-[0_0_8px_rgba(255,255,255,0.6)] flex-shrink-0"
            />
          </Link>
        </div>

        {/* Right side - Desktop Navigation and Mobile Menu */}
        <div className="flex items-center gap-2 sm:gap-3 flex-shrink-0">
          {/* Desktop Navigation */}
          <nav className="hidden lg:flex items-center gap-2 min-w-0 mr-2 sm:mr-3">
            {navLinks.map(({ href, label }, index) => {
              const isActive = pathname === href || (href !== "/" && pathname.startsWith(href));
              
              return (
                <Link
                  key={`nav-${label}-${index}`}
                  href={href}
                  className="relative group px-5 py-2.5 text-xs font-medium uppercase tracking-[0.2em] transition-all duration-300"
                >
                  <span className={`relative z-10 transition-all duration-300 ${
                    isActive
                      ? "text-white/80"
                      : "text-white/30 group-hover:text-white group-hover:brightness-[1.8] group-hover:drop-shadow-[0_0_20px_rgba(255,255,255,1)]"
                  }`}>
                    {label}
                  </span>
                  <div className="absolute inset-0 bg-white/0 group-hover:bg-white/5 transition-colors duration-300"></div>
                </Link>
              );
            })}
          </nav>

          {/* Mobile Menu Button */}
          <button
            className="lg:hidden text-white p-1.5 sm:p-2 hover:bg-white/10 transition-colors rounded-md flex-shrink-0"
            onClick={() => setMobileMenuOpen(!mobileMenuOpen)}
            aria-label="Toggle menu"
          >
            {mobileMenuOpen ? (
              <svg className="w-5 h-5 sm:w-6 sm:h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            ) : (
              <svg className="w-5 h-5 sm:w-6 sm:h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16M4 18h16" />
              </svg>
            )}
          </button>
        </div>
      </div>

      {/* Mobile Menu */}
      {mobileMenuOpen && (
        <div className="lg:hidden border-t border-white/10" style={{ background: 'rgb(18,18,18)' }}>
          <nav className="flex flex-col px-4 py-4">
            {navLinks.map(({ href, label }, index) => {
              const isActive = pathname === href || (href !== "/" && pathname.startsWith(href));

              return (
                <Link
                  key={`mobile-nav-${label}-${index}`}
                  href={href}
                  onClick={() => setMobileMenuOpen(false)}
                  className={`px-4 py-3 text-sm uppercase tracking-[0.25em] border-b border-white/10 last:border-0 transition-all duration-200 ${
                    isActive
                      ? "text-white hover:bg-white/5"
                      : "text-white hover:bg-white/5"
                  }`}
                >
                  {label}
                </Link>
              );
            })}
          </nav>
        </div>
      )}

      {/* Add keyframes for shield animation */}
      <style jsx>{`
        @keyframes drawShield {
          to {
            stroke-dashoffset: 0;
          }
        }
      `}</style>
    </header>
  );
}
