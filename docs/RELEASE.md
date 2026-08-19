# Release Runbook

This project ships desktop builds through GitHub Actions.

## Workflows

- `.github/workflows/ci.yml`: PR/push quality gate across Linux, macOS, and Windows.
  It runs formatting, Clippy, Rust tests, dependency-boundary validation,
  frontend tests/build/bundle budgets, release-validator tests, and the real
  desktop init-to-history E2E workflow.
- `.github/workflows/build-artifacts.yml`: manual artifact build and `app-v*` tag release.
  It builds Windows x64, Linux x86_64, macOS arm64 (`macos-15`), and
  macOS x86_64 (`macos-15-intel`) bundles.

Manual runs may upload unsigned workflow artifacts for inspection. Pushing a tag
named `app-vX.Y.Z` is fail-closed: application versions, updater configuration,
updater signing, Windows signing, and macOS signing/notarization inputs must all
pass `pnpm -C app release:check` before a GitHub prerelease is created.

When signed credentials are intentionally unavailable, a maintainer may manually
dispatch `Build Artifacts` with an existing `app-vX.Y.Z` tag and
`allow_unsigned=true`. This explicit path creates an unsigned prerelease; normal
tag-triggered releases remain fail-closed.

## Required Secrets

Updater signing:

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
- `TAURI_UPDATER_PUBKEY`
- `TAURI_UPDATER_ENDPOINT` optional, defaults to `https://github.com/<owner>/<repo>/releases/latest/download/latest.json`

Windows code signing with Azure Trusted Signing:

- `AZURE_CLIENT_ID`
- `AZURE_CLIENT_SECRET`
- `AZURE_TENANT_ID`
- `AZURE_TRUSTED_SIGNING_ENDPOINT`
- `AZURE_TRUSTED_SIGNING_ACCOUNT`
- `AZURE_TRUSTED_SIGNING_CERTIFICATE_PROFILE`

macOS signing and notarization:

- `APPLE_CERTIFICATE`
- `APPLE_CERTIFICATE_PASSWORD`
- `APPLE_SIGNING_IDENTITY`
- `APPLE_ID`
- `APPLE_PASSWORD`
- `APPLE_TEAM_ID`

Tagged releases do not fall back to unsigned output. If any updater, Windows
signing, or macOS signing/notarization input required by the matrix is missing,
the preflight fails before a GitHub prerelease is created. Use a manual workflow
dispatch for explicitly unsigned inspection artifacts.

## Local Preflight

Run these before creating a release tag:

```powershell
pnpm -C app install --frozen-lockfile
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
powershell -NoProfile -File scripts/check-dependency-boundaries.ps1
cargo test --workspace
node --test scripts/release-preflight.test.mjs
node --test scripts/check-bundle-size.test.mjs
pnpm -C app test
pnpm -C app build
pnpm -C app e2e:ci
pnpm -C app release:check -- --allow-unsigned
pnpm -C app tauri build --ci --no-sign
```

Expected Windows unsigned bundle paths:

- `target/release/bundle/msi/VersionArc_0.1.4_x64_en-US.msi`
- `target/release/bundle/nsis/VersionArc_0.1.4_x64-setup.exe`

## Manual Artifact Dry Run

Use GitHub Actions > Build Artifacts > Run workflow. This does not create a release. It builds all three platforms and uploads workflow artifacts for inspection.

Check:

- Linux, both macOS architecture jobs, and Windows finish successfully.
- Artifact names are `tauri-linux`, `tauri-macos-arm64`,
  `tauri-macos-x64`, and `tauri-windows`.
- Manual artifacts may be unsigned. Tagged releases must be signed and
  notarized where applicable.
- Install and launch each artifact on the matching architecture. CI coverage
  does not replace this real-device acceptance step.

## Manual Unsigned Prerelease

Use GitHub Actions > Build Artifacts > Run workflow, set `release_tag` to an
existing version tag such as `app-v0.1.4`, and enable `allow_unsigned`. The
workflow validates that the tag matches the application version, creates a
prerelease, and uploads unsigned bundles. Updater metadata is omitted when
updater signing inputs are unavailable.

## Tag Release

1. Update the version in `app/package.json`, `app/src-tauri/Cargo.toml`, and
   `app/src-tauri/tauri.conf.json`, then update release notes.
2. Run the local preflight.
3. Create and push a tag:

```powershell
git tag app-v0.1.4
git push origin app-v0.1.4
```

4. Wait for Build Artifacts to finish.
5. Open the generated GitHub prerelease and inspect uploaded bundles.
6. If updater secrets are configured, verify `latest.json` and `.sig` assets are present.
7. Promote the prerelease to a normal release only after installing the bundles on real machines.

The normal production frontend excludes the WDIO bridge and fixture commands.
They are compiled and permitted only by the dedicated `e2e` feature/config.
The production shell enforces a restrictive CSP, and the initial JavaScript
entry chunk must stay at or below 500,000 bytes.

## Rollback

If a tagged release is bad:

1. Mark the GitHub release as draft or delete the release assets.
2. Do not reuse the same tag for a fixed release unless the original release was never consumed.
3. Create a new patch version tag, for example `app-v0.1.1`.
