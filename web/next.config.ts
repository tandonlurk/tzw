import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Dev server only: without this, opening the app via http://127.0.0.1:PORT never hydrates (stuck on "Loading...").
  allowedDevOrigins: ["127.0.0.1"],
};

export default nextConfig;
