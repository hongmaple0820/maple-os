import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/e2e",
  fullyParallel: true,
  timeout: 30000,
  retries: 0,
  use: {
    baseURL: "http://localhost:3000",
    trace: "on-first-retry",
  },
  projects: [
    { name: "chromium", use: { browserName: "chromium" } },
  ],
  webServer: [
    {
      command: "node scripts/qa/start-e2e-backend.mjs",
      url: "http://127.0.0.1:7788/health",
      reuseExistingServer: true,
      timeout: 180000,
    },
    {
      command: "pnpm --filter=mapleos-web dev",
      url: "http://127.0.0.1:3000",
      reuseExistingServer: true,
      timeout: 120000,
    },
  ],
});
