import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  reactStrictMode: true,
  poweredByHeader: false,
  // The contract-verification route hashes the release wasm at runtime, so the
  // artifact has to travel with that lambda instead of living on the build host.
  outputFileTracingIncludes: {
    "/api/contract-verification": ["./contracts/guard_vault.wasm"],
  },
};

export default nextConfig;
