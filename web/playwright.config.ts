import { defineConfig, devices } from "@playwright/test";

// End-to-end convergence tests (spec §11, gate M2). The webServer runs the real
// Rust server against the built frontend (`web/dist`), so `npm run e2e` builds
// first. Monaco is loaded via its default (CDN) loader, so these tests need
// network access until the editor is self-hosted (W2D4).

const PORT = 8080;
const BASE_URL = `http://127.0.0.1:${PORT}`;

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: false,
  workers: 1,
  timeout: 60_000,
  expect: { timeout: 15_000 },
  use: {
    baseURL: BASE_URL,
    trace: "on-first-retry",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    command: "cargo run -p syncpad-server",
    cwd: "..",
    url: BASE_URL,
    reuseExistingServer: !process.env.CI,
    timeout: 240_000,
    env: {
      PORT: String(PORT),
      SYNCPAD_STATIC_DIR: "web/dist",
      SYNCPAD_DATA_DIR: "target/e2e-data",
      // Keep documents alive for the whole run.
      SYNCPAD_DOC_TTL_SECS: "3600",
      SYNCPAD_REAP_SECS: "3600",
    },
  },
});
