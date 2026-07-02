import path from "node:path";
import { fileURLToPath } from "node:url";

const configDir = path.dirname(fileURLToPath(import.meta.url));
const binaryName = process.platform === "win32" ? "app.exe" : "app";
const appBinaryPath =
  process.env.TAURI_APP_BINARY ??
  path.resolve(configDir, "..", "target", "release", binaryName);

export const config = {
  runner: "local",
  specs: ["./e2e/**/*.e2e.js"],
  maxInstances: 1,
  services: [
    [
      "@wdio/tauri-service",
      {
        appBinaryPath,
        driverProvider: "embedded",
        autoInstallTauriDriver: false,
        embeddedPort: 4445,
        startTimeout: 90_000,
        statusPollTimeout: 10_000,
        captureBackendLogs: true,
        captureFrontendLogs: true,
      },
    ],
  ],
  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": {
        application: appBinaryPath,
      },
    },
  ],
  logLevel: "info",
  bail: 1,
  waitforTimeout: 10_000,
  connectionRetryTimeout: 120_000,
  connectionRetryCount: 1,
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: {
    ui: "bdd",
    timeout: 60_000,
  },
};
