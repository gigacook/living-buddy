/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The Rust server (tendly serve) listens on 127.0.0.1:7878 by default.
const api = process.env.TENDLY_API ?? "http://127.0.0.1:7878";

export default defineConfig({
  plugins: [react()],
  server: {
    host: "127.0.0.1",
    port: 5173,
    strictPort: true,
    proxy: {
      "/api": { target: api, changeOrigin: false },
      "/share": { target: api, changeOrigin: false },
      "/healthz": { target: api },
    },
  },
  build: {
    outDir: "dist",
    // No source maps in production builds; the code is public anyway, but
    // maps would bloat release bundles and native packages.
    sourcemap: false,
    target: "es2022",
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
    css: false,
  },
});
