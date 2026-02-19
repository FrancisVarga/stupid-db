import type { NextConfig } from "next";

const apiBase = process.env.API_BASE || "http://localhost:56415";

const nextConfig: NextConfig = {
  output: "standalone",
  reactCompiler: true,
  serverExternalPackages: ["postgres"],
  async rewrites() {
    return [
      // Proxy all API routes to the Rust backend
      {
        source: "/stats",
        destination: `${apiBase}/stats`,
      },
      {
        source: "/compute/:path*",
        destination: `${apiBase}/compute/:path*`,
      },
      {
        source: "/graph/:path*",
        destination: `${apiBase}/graph/:path*`,
      },
      {
        source: "/api/:path*",
        destination: `${apiBase}/api/:path*`,
      },
      {
        source: "/ws",
        destination: `${apiBase}/ws`,
      },
    ];
  },
};

export default nextConfig;
