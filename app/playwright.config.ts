import { defineConfig, devices } from "@playwright/test";

// UI smoke tests run the real UI against the mock backend (no Tauri needed),
// so they work identically on Windows, macOS and Linux CI.
export default defineConfig({
  testDir: "tests/ui",
  timeout: 30_000,
  retries: process.env.CI ? 1 : 0,
  use: { baseURL: "http://localhost:1420", viewport: { width: 1280, height: 860 } },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 860 } } },
    { name: "reduced-motion", use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 860 }, reducedMotion: "reduce" } },
  ],
  webServer: { command: "npm run dev", url: "http://localhost:1420", reuseExistingServer: !process.env.CI, timeout: 60_000 },
});
