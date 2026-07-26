import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  cargoPackageVersion,
  resolveUpdaterEndpoints,
  validateRelease,
} from "./release-preflight.mjs";

const updater = {
  pubkey: "production-public-key",
  endpoints: ["https://example.test/latest.json"],
};

const secureCsp =
  "default-src 'self'; connect-src ipc: http://ipc.localhost https://api.github.com https://gitlab.com https:; img-src 'self' asset: http://asset.localhost data: blob:; style-src 'self' 'unsafe-inline'; font-src 'self' data:; object-src 'none'; frame-src 'none'; base-uri 'none'";

const completeEnv = {
  TAURI_SIGNING_PRIVATE_KEY: "updater-private-key",
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD: "updater-password",
  AZURE_CLIENT_ID: "client",
  AZURE_CLIENT_SECRET: "secret",
  AZURE_TENANT_ID: "tenant",
  AZURE_TRUSTED_SIGNING_ENDPOINT: "https://eus.codesigning.azure.net",
  AZURE_TRUSTED_SIGNING_ACCOUNT: "account",
  AZURE_TRUSTED_SIGNING_CERTIFICATE_PROFILE: "profile",
  APPLE_CERTIFICATE: "certificate",
  APPLE_CERTIFICATE_PASSWORD: "certificate-password",
  APPLE_SIGNING_IDENTITY: "Developer ID Application: Example",
  APPLE_ID: "release@example.test",
  APPLE_PASSWORD: "app-password",
  APPLE_TEAM_ID: "TEAM123",
};

function validInput(overrides = {}) {
  return {
    versions: { package: "0.1.3", tauri: "0.1.3", cargo: "0.1.3" },
    tag: "app-v0.1.3",
    updater,
    csp: secureCsp,
    env: completeEnv,
    platform: "windows",
    release: true,
    ...overrides,
  };
}

test("rejects inconsistent application versions", () => {
  const errors = validateRelease(
    validInput({
      versions: { package: "0.1.3", tauri: "0.1.4", cargo: "0.1.3" },
    }),
  );

  assert.match(errors.join("\n"), /versions differ/i);
});

test("rejects a release tag that does not match the application version", () => {
  const errors = validateRelease(validInput({ tag: "app-v0.1.2" }));

  assert.match(errors.join("\n"), /tag.*app-v0\.1\.3/i);
});

test("rejects the development updater placeholder for a release", () => {
  const errors = validateRelease(
    validInput({
      updater: { ...updater, pubkey: "local-development-placeholder" },
    }),
  );

  assert.match(errors.join("\n"), /placeholder/i);
});

test("rejects an empty updater endpoint for a release", () => {
  const errors = validateRelease(
    validInput({ updater: { ...updater, endpoints: [] } }),
  );

  assert.match(errors.join("\n"), /updater endpoint/i);
});

test("uses the GitHub release endpoint when no updater endpoint is configured", () => {
  assert.deepEqual(
    resolveUpdaterEndpoints(
      { GITHUB_REPOSITORY: "example/git-client" },
      [],
    ),
    [
      "https://github.com/example/git-client/releases/latest/download/latest.json",
    ],
  );
});

test("an explicit updater endpoint overrides the GitHub fallback", () => {
  assert.deepEqual(
    resolveUpdaterEndpoints(
      {
        GITHUB_REPOSITORY: "example/git-client",
        TAURI_UPDATER_ENDPOINT: "https://updates.example.test/latest.json",
      },
      [],
    ),
    ["https://updates.example.test/latest.json"],
  );
});

test("reports every missing updater signing input", () => {
  const errors = validateRelease(
    validInput({
      env: {
        ...completeEnv,
        TAURI_SIGNING_PRIVATE_KEY: "",
        TAURI_SIGNING_PRIVATE_KEY_PASSWORD: "",
      },
    }),
  );
  const message = errors.join("\n");

  assert.match(message, /TAURI_SIGNING_PRIVATE_KEY/);
  assert.match(message, /TAURI_SIGNING_PRIVATE_KEY_PASSWORD/);
});

test("reports every missing platform signing input", () => {
  const errors = validateRelease(
    validInput({
      platform: "macos",
      env: {
        ...completeEnv,
        APPLE_CERTIFICATE: "",
        APPLE_PASSWORD: "",
        APPLE_TEAM_ID: "",
      },
    }),
  );
  const message = errors.join("\n");

  assert.match(message, /APPLE_CERTIFICATE/);
  assert.match(message, /APPLE_PASSWORD/);
  assert.match(message, /APPLE_TEAM_ID/);
});

test("accepts a complete production release configuration", () => {
  assert.deepEqual(validateRelease(validInput()), []);
  assert.deepEqual(
    validateRelease(validInput({ platform: "macos" })),
    [],
  );
});

test("rejects a disabled or permissive content security policy", () => {
  assert.match(
    validateRelease(validInput({ csp: null })).join("\n"),
    /content security policy/i,
  );
  assert.match(
    validateRelease(
      validInput({ csp: "default-src *; object-src *; frame-src *" }),
    ).join("\n"),
    /content security policy/i,
  );
});

test("unsigned manual mode still enforces version consistency", () => {
  const errors = validateRelease(
    validInput({
      release: false,
      tag: "",
      env: {},
      updater: {
        pubkey: "local-development-placeholder",
        endpoints: [],
      },
      versions: { package: "0.1.3", tauri: "0.1.2", cargo: "0.1.3" },
    }),
  );

  assert.equal(errors.length, 1);
  assert.match(errors[0], /versions differ/i);
});

test("reads the version from a Cargo package section with later sections", () => {
  const cargoToml = `[package]
name = "app"
version = "0.1.3"
edition = "2024"

[lib]
name = "app_lib"
`;

  assert.equal(cargoPackageVersion(cargoToml), "0.1.3");
});

test("release documentation does not retain completed hardening as pending work", async () => {
  const root = path.resolve(
    path.dirname(fileURLToPath(import.meta.url)),
    "..",
  );
  const text = (
    await Promise.all([
      readFile(path.join(root, "README.md"), "utf8"),
      readFile(path.join(root, "docs", "RELEASE.md"), "utf8"),
      readFile(path.join(root, "docs", "HANDOFF.md"), "utf8"),
    ])
  ).join("\n");

  assert.doesNotMatch(text, /Cross-platform CI should be tightened/i);
  assert.doesNotMatch(text, /main 领先 origin/);
});

test("the packaged frontend does not depend on font CDNs blocked by CSP", async () => {
  const root = path.resolve(
    path.dirname(fileURLToPath(import.meta.url)),
    "..",
  );
  const index = await readFile(path.join(root, "app", "index.html"), "utf8");

  assert.doesNotMatch(index, /fonts\.(?:googleapis|gstatic)\.com/i);
});

test("desktop E2E failures are retained as CI artifacts", async () => {
  const root = path.resolve(
    path.dirname(fileURLToPath(import.meta.url)),
    "..",
  );
  const workflow = await readFile(
    path.join(root, ".github", "workflows", "ci.yml"),
    "utf8",
  );

  assert.match(
    workflow,
    /if:\s*failure\(\)[\s\S]*actions\/upload-artifact@v4[\s\S]*app\/\.e2e-tmp\/\*\*/i,
  );
});
