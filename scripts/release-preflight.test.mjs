import assert from "node:assert/strict";
import test from "node:test";

import {
  cargoPackageVersion,
  validateRelease,
} from "./release-preflight.mjs";

const updater = {
  pubkey: "production-public-key",
  endpoints: ["https://example.test/latest.json"],
};

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
