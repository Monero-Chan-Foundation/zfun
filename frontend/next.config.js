/** @type {import('next').NextConfig} */
const nextConfig = {
  eslint: {
    ignoreDuringBuilds: true,
  },
  typescript: {
    ignoreBuildErrors: false,
  },
  images: {
    unoptimized: true,
  },
  experimental: {
    externalDir: true,
  },
  webpack: (config, { isServer }) => {
    config.experiments = {
      ...(config.experiments ?? {}),
      asyncWebAssembly: true,
      layers: true,
    };

    if (!isServer) {
      config.output.webassemblyModuleFilename = "static/wasm/[modulehash].wasm";
    }

    return config;
  },
  // Exclude large WASM files from static output for Cloudflare Pages
  // They'll need to be hosted separately (e.g., on Cloudflare R2 or a CDN)
  outputFileTracingExcludes: {
    '*': [
      '**/pkg/**/*.wasm',
      '**/pkg/**/*.wasm-opt.wasm',
    ],
  },
};

export default nextConfig;
