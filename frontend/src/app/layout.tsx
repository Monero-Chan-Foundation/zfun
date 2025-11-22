import type { Metadata } from "next";
import { Inter } from "next/font/google";
import "./globals.css";
import AppProviders from "../components/AppProviders";
import { Suspense } from "react";

const inter = Inter({
  variable: "--font-inter",
  subsets: ["latin"],
});
export const metadata: Metadata = {
  title: "Z: Prove ownership. Keep privacy.",
  description: "Z.fun lets Zcash holders verify shielded ownership without revealing balances or transactions. claim rewards based on proof, not exposure.",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body className={`${inter.variable} antialiased`}>
        <AppProviders>
          <Suspense>
            {children}
          </Suspense>
        </AppProviders>
      </body>
    </html>
  );
}
