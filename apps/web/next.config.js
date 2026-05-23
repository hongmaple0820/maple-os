/** @type {import('next').NextConfig} */
const nextConfig = {
  transpilePackages: ["@mapleos/ui", "@mapleos/sdk"],
  async rewrites() {
    return [
      { source: "/api/maple/:path*", destination: "http://127.0.0.1:7788/:path*" },
      { source: "/rpc", destination: "http://127.0.0.1:7788/rpc" },
      { source: "/ws/agents", destination: "http://127.0.0.1:7788/ws/agents" },
      { source: "/api/scale/:path*", destination: "http://127.0.0.1:7790/:path*" },
    ];
  },
};

module.exports = nextConfig;