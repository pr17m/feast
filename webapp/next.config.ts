import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Self-contained server for Docker (one image for every country; see runtime-config.js).
  output: "standalone",
  poweredByHeader: false,
  // Don't gzip-buffer proxied Server-Sent Events (/v1/me/events); the CDN/LB compresses.
  compress: false,
  async headers() {
    return [
      {
        source: "/:path*",
        headers: [
          { key: "X-Content-Type-Options", value: "nosniff" },
          { key: "Referrer-Policy", value: "strict-origin-when-cross-origin" },
          { key: "X-Frame-Options", value: "DENY" },
          { key: "Permissions-Policy", value: "camera=(self), geolocation=(self), microphone=()" },
        ],
      },
    ];
  },
};

export default nextConfig;
