import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const UPDATER_KEYS = [
  "TAURI_SIGNING_PRIVATE_KEY",
  "TAURI_SIGNING_PRIVATE_KEY_PASSWORD",
];

const WINDOWS_SIGNING_KEYS = [
  "AZURE_CLIENT_ID",
  "AZURE_CLIENT_SECRET",
  "AZURE_TENANT_ID",
  "AZURE_TRUSTED_SIGNING_ENDPOINT",
  "AZURE_TRUSTED_SIGNING_ACCOUNT",
  "AZURE_TRUSTED_SIGNING_CERTIFICATE_PROFILE",
];

const MACOS_SIGNING_KEYS = [
  "APPLE_CERTIFICATE",
  "APPLE_CERTIFICATE_PASSWORD",
  "APPLE_SIGNING_IDENTITY",
  "APPLE_ID",
  "APPLE_PASSWORD",
  "APPLE_TEAM_ID",
];

function missingKeys(env, keys) {
  return keys.filter((key) => !String(env[key] ?? "").trim());
}

export function validateRelease(input) {
  const errors = [];
  const values = Object.values(input.versions);
  const uniqueVersions = new Set(values);

  if (uniqueVersions.size !== 1) {
    errors.push(
      `Application versions differ: package=${input.versions.package}, tauri=${input.versions.tauri}, cargo=${input.versions.cargo}.`,
    );
  }

  const csp = String(input.csp ?? "");
  const requiredCspDirectives = [
    "default-src 'self'",
    "object-src 'none'",
    "frame-src 'none'",
  ];
  if (
    !csp ||
    requiredCspDirectives.some((directive) => !csp.includes(directive))
  ) {
    errors.push(
      "Content Security Policy must enable self-only defaults and disable objects and frames.",
    );
  }

  if (!input.release) return errors;

  const version = input.versions.package;
  const expectedTag = `app-v${version}`;
  if (input.tag !== expectedTag) {
    errors.push(
      `Release tag must be ${expectedTag}; received ${input.tag || "(empty)"}.`,
    );
  }

  if (
    !input.updater.pubkey?.trim() ||
    input.updater.pubkey.includes("local-development-placeholder")
  ) {
    errors.push(
      "Updater public key is missing or still uses the local development placeholder.",
    );
  }
  if (
    !Array.isArray(input.updater.endpoints) ||
    input.updater.endpoints.every((endpoint) => !String(endpoint).trim())
  ) {
    errors.push("At least one production updater endpoint is required.");
  }

  for (const key of missingKeys(input.env, UPDATER_KEYS)) {
    errors.push(`Missing updater signing input: ${key}.`);
  }

  const platformKeys =
    input.platform === "windows"
      ? WINDOWS_SIGNING_KEYS
      : input.platform === "macos"
        ? MACOS_SIGNING_KEYS
        : [];
  for (const key of missingKeys(input.env, platformKeys)) {
    errors.push(`Missing ${input.platform} signing input: ${key}.`);
  }

  return errors;
}

export function cargoPackageVersion(cargoToml) {
  let inPackageSection = false;
  for (const line of cargoToml.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (trimmed.startsWith("[") && trimmed.endsWith("]")) {
      inPackageSection = trimmed === "[package]";
      continue;
    }
    if (!inPackageSection) continue;
    const version = trimmed.match(/^version\s*=\s*"([^"]+)"$/)?.[1];
    if (version) return version;
  }
  throw new Error("Could not read [package].version from Cargo.toml");
}

export function resolveUpdaterEndpoints(env, configuredEndpoints = []) {
  const explicitEndpoint = String(env.TAURI_UPDATER_ENDPOINT ?? "").trim();
  if (explicitEndpoint) return [explicitEndpoint];

  const configured = configuredEndpoints.filter((endpoint) =>
    String(endpoint).trim(),
  );
  if (configured.length) return configured;

  const repository = String(env.GITHUB_REPOSITORY ?? "").trim();
  return repository
    ? [
        `https://github.com/${repository}/releases/latest/download/latest.json`,
      ]
    : [];
}

export async function loadReleaseInput({
  root,
  env,
  release,
  platform,
  tag,
}) {
  const [packageText, tauriText, cargoText] = await Promise.all([
    readFile(path.join(root, "app", "package.json"), "utf8"),
    readFile(
      path.join(root, "app", "src-tauri", "tauri.conf.json"),
      "utf8",
    ),
    readFile(path.join(root, "app", "src-tauri", "Cargo.toml"), "utf8"),
  ]);
  const packageJson = JSON.parse(packageText);
  const tauriConfig = JSON.parse(tauriText);
  const envPubkey = String(env.TAURI_UPDATER_PUBKEY ?? "").trim();

  return {
    versions: {
      package: packageJson.version,
      tauri: tauriConfig.version,
      cargo: cargoPackageVersion(cargoText),
    },
    tag,
    updater: {
      pubkey: envPubkey || tauriConfig.plugins?.updater?.pubkey || "",
      endpoints: resolveUpdaterEndpoints(
        env,
        tauriConfig.plugins?.updater?.endpoints || [],
      ),
    },
    csp: tauriConfig.app?.security?.csp,
    env,
    platform,
    release,
  };
}

async function main() {
  const args = new Set(process.argv.slice(2));
  const release = !args.has("--allow-unsigned");
  const platformArg = process.argv.find((arg) => arg.startsWith("--platform="));
  const platform =
    platformArg?.slice("--platform=".length) ||
    (process.platform === "win32"
      ? "windows"
      : process.platform === "darwin"
        ? "macos"
        : "linux");
  const root = path.resolve(
    path.dirname(fileURLToPath(import.meta.url)),
    "..",
  );
  const input = await loadReleaseInput({
    root,
    env: process.env,
    release,
    platform,
    tag: process.env.GITHUB_REF_NAME ?? "",
  });
  const errors = validateRelease(input);

  if (errors.length) {
    console.error("Release preflight failed:");
    for (const error of errors) console.error(`- ${error}`);
    process.exitCode = 1;
    return;
  }

  console.log(
    release
      ? `Release preflight passed for ${input.tag} (${platform}).`
      : `Unsigned artifact preflight passed for ${input.versions.package}.`,
  );
}

const invokedPath = process.argv[1]
  ? path.resolve(process.argv[1])
  : "";
if (invokedPath === fileURLToPath(import.meta.url)) {
  await main();
}
