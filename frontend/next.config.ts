import type { NextConfig } from "next";

const isProductionBuild = process.env.NODE_ENV === "production";

const nextConfig: NextConfig = {
  output: isProductionBuild ? "export" : undefined,
  trailingSlash: true,
};

export default nextConfig;
