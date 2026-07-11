import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The dev server proxies API and WebSocket traffic to the Rust server so the
// frontend can be developed with hot reload while the backend runs normally.
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      "/api": "http://127.0.0.1:8080",
      "/ws": { target: "ws://127.0.0.1:8080", ws: true },
    },
  },
});
