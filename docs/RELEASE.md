# Release Runbook

This project ships desktop builds through GitHub Actions.

## Workflows

- `.github/workflows/ci.yml`: PR/push quality gate across Linux, macOS, and Windows.
- `.github/workflows/build-artifacts.yml`: manual artifact build and `app-v*` tag release.

Manual runs upload unsigned or signed workflow artifacts for inspection. Pushing a tag named `app-vX.Y.Z` creates or reuses a GitHub prerelease and uploads platform bundles.

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

If code-signing secrets are missing, the workflow falls back to unsigned bundles when updater artifacts are not enabled. If updater signing secrets are missing, `latest.json` upload is disabled.

## Local Preflight

Run these before creating a release tag:

```powershell
pnpm -C app install --frozen-lockfile
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm -C app test
pnpm -C app build
pnpm -C app e2e:ci
pnpm -C app tauri build --ci --no-sign
```

Expected Windows unsigned bundle paths:

- `target/release/bundle/msi/Git Client_0.1.0_x64_en-US.msi`
- `target/release/bundle/nsis/Git Client_0.1.0_x64-setup.exe`

## Manual Artifact Dry Run

Use GitHub Actions > Build Artifacts > Run workflow. This does not create a release. It builds all three platforms and uploads workflow artifacts for inspection.

Check:

- Linux, macOS, and Windows jobs finish successfully.
- Artifact names are `tauri-linux`, `tauri-macos`, and `tauri-windows`.
- Signed builds only appear when the relevant signing secrets are present.

## Tag Release

1. Update `app/src-tauri/tauri.conf.json` version and any release notes.
2. Run the local preflight.
3. Create and push a tag:

```powershell
git tag app-v0.1.0
git push origin app-v0.1.0
```

4. Wait for Build Artifacts to finish.
5. Open the generated GitHub prerelease and inspect uploaded bundles.
6. If updater secrets are configured, verify `latest.json` and `.sig` assets are present.
7. Promote the prerelease to a normal release only after installing the bundles on real machines.

## Rollback

If a tagged release is bad:

1. Mark the GitHub release as draft or delete the release assets.
2. Do not reuse the same tag for a fixed release unless the original release was never consumed.
3. Create a new patch version tag, for example `app-v0.1.1`.
