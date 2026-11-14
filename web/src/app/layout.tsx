import type { Metadata } from "next";
import "./globals.css";
import "@near-wallet-selector/modal-ui/styles.css";

export const metadata: Metadata = {
  title: "GAINZ — Guard AI NEAR ZK",
  description:
    "A red-team range and loss-bound report for AI agent wallets on NEAR. Attacks your agent's spending policy, finds the legal sequence that breaks it, and reports the maximum you can lose.",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
