import path from "node:path";
import { fileURLToPath } from "node:url";
import fs from "node:fs";

const configDir = path.dirname(fileURLToPath(import.meta.url));
const binaryName = process.platform === "win32" ? "app.exe" : "app";
const e2eRoot = path.resolve(configDir, ".e2e-tmp");
const harnessLog = path.join(e2eRoot, "harness.log");
process.env.GIT_CLIENT_E2E_ROOT = e2eRoot;
const fatalHarnessPatterns = [
  /thread ['"].+['"] panicked|panicked at/i,
  /Tauri core\.invoke not available|core\.invoke unavailable/i,
  /failed to start .*driver|driver startup (?:failed|error)|webdriver.*startup.*error/i,
];
let restoreOutputCapture;

function installOutputCapture() {
  fs.mkdirSync(e2eRoot, { recursive: true });
  const originalStdout = process.stdout.write.bind(process.stdout);
  const originalStderr = process.stderr.write.bind(process.stderr);
  const capture = (chunk) => {
    fs.appendFileSync(harnessLog, Buffer.isBuffer(chunk) ? chunk : String(chunk));
  };
  process.stdout.write = (chunk, ...args) => {
    capture(chunk);
    return originalStdout(chunk, ...args);
  };
  process.stderr.write = (chunk, ...args) => {
    capture(chunk);
    return originalStderr(chunk, ...args);
  };
  return () => {
    process.stdout.write = originalStdout;
    process.stderr.write = originalStderr;
  };
}

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
  onPrepare() {
    fs.rmSync(e2eRoot, { recursive: true, force: true });
    restoreOutputCapture = installOutputCapture();
  },
  onComplete(exitCode) {
    restoreOutputCapture?.();
    const output = fs.existsSync(harnessLog)
      ? fs.readFileSync(harnessLog, "utf8")
      : "";
    const fatalPattern = fatalHarnessPatterns.find((pattern) => pattern.test(output));
    if (fatalPattern) {
      throw new Error(
        `Desktop E2E harness emitted a fatal warning matching ${fatalPattern}. Logs retained at ${harnessLog}`,
      );
    }
    if (exitCode === 0) {
      fs.rmSync(e2eRoot, { recursive: true, force: true });
    }
  },
};
