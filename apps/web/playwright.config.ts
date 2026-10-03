import { defineConfig, devices } from "@playwright/test";

// End-to-end tests run against the real Rust server serving the production build.
const port = Number(process.env.E2E_PORT ?? 7899);

export default defineConfig({
  testDir: "./e2e",
  timeout: 60_000,
  fullyParallel: false,
  workers: 1,
  reporter: [["list"]],
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    trace: "off",
    screenshot: "off",
  },
  projects: [
    { name: "desktop-chromium", use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 860 } } },
    { name: "phone-chromium", use: { ...devices["Pixel 7"] } },
  ],
  webServer: {
    command: `bash ../../scripts/e2e-server.sh ${port}`,
    url: `http://127.0.0.1:${port}/readyz`,
    reuseExistingServer: false,
    timeout: 180_000,
  },
});
