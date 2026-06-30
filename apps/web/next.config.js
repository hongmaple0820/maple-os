const path = require("path");

/** @type {import('next').NextConfig} */
const nextConfig = {
  transpilePackages: ["@mapleos/ui", "@mapleos/sdk"],
  output: process.env.NEXT_STATIC_EXPORT ? "export" : undefined,
  async rewrites() {
    if (process.env.NEXT_STATIC_EXPORT) return [];
    return [
      { source: "/api/maple/:path*", destination: "http://127.0.0.1:7788/:path*" },
      { source: "/rpc", destination: "http://127.0.0.1:7788/rpc" },
      { source: "/ws/agents", destination: "http://127.0.0.1:7788/ws/agents" },
      { source: "/api/scale/:path*", destination: "http://127.0.0.1:7790/:path*" },
    ];
  },
};

module.exports = nextConfig;