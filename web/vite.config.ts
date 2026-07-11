import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// The dev server proxies API and WebSocket traffic to the Rust server so the
// frontend can be developed with hot reload while the backend runs normally.
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      "/api": "http://127.0.0.1:8090",
      "/ws": { target: "ws://127.0.0.1:8090", ws: true },
    },
  },
  // Unit tests live in src/; the Playwright e2e suite (e2e/) runs separately.
  test: {
    include: ["src/**/*.{test,spec}.{ts,tsx}"],
  },
});
