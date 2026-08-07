# Production Readiness Hardening Design

## Goal

Move Strata 0.1.x from a feature-rich prerelease toward a trustworthy release candidate by closing repository-local release, reliability, architecture, internationalization, and performance gaps. External signing certificates and production updater secrets remain operator-provided.

## Scope and Delivery Order

The work is divided into three independently verifiable phases:

1. Release and security hardening.
2. Critical desktop workflow reliability.
3. Architecture and user-experience cleanup.

Each phase must leave the repository buildable and testable. New product capabilities such as worktree creation or switching are outside this hardening effort.

## Phase 1: Release and Security Hardening

### Tauri security policy

Replace the disabled CSP with a restrictive policy that supports the current local Tauri frontend and the explicit GitHub/GitLab HTTPS API access used by collaboration features. The policy must not enable arbitrary remote scripts, frames, or object content.

### Release preflight

Add a repository-owned preflight command that checks:

- `app/package.json`, `app/src-tauri/tauri.conf.json`, and `app/src-tauri/Cargo.toml` use the same application version.
- Release tags use the exact form `app-v<version>`.
- A production release does not use the development updater public-key placeholder.
- A production release has a non-empty updater endpoint.
- Required updater signing inputs exist.
- Windows and macOS signing inputs are complete for release-tag builds.

Manual artifact builds may explicitly remain unsigned. A tag-triggered release must fail closed when production signing or updater configuration is incomplete. The check must report every missing input in one run rather than fail on the first one.

### Distribution matrix

Keep Windows x64 and Linux x86_64 artifacts. Build both Apple Silicon and Intel macOS artifacts, or a universal binary if the supported runner/toolchain makes that path more reliable. Artifact names must expose the architecture.

### Release documentation

Update `README.md`, `docs/RELEASE.md`, and `docs/HANDOFF.md` to describe the implemented CI, current prerelease status, exact release gates, supported architectures, and the remaining operator steps. Remove stale statements that CI is missing or that local `main` is ahead of `origin`.

## Phase 2: Critical Desktop Workflow Reliability

### E2E scope

Replace the launch-only confidence signal with an isolated local Git workflow:

1. Create a temporary repository through the application.
2. Open the repository.
3. Create or modify a file through an E2E-only deterministic fixture command.
4. Observe the working-tree change.
5. Stage the file.
6. Commit it.
7. Confirm that the commit appears in history.

The fixture command is compiled only with the existing `e2e` feature and is unavailable in production builds. It accepts only paths beneath the E2E repository root and returns structured errors.

Network-dependent GitHub, GitLab, fetch, pull, and push flows remain covered by deterministic unit/component tests rather than live accounts.

### Harness reliability

Configure the Webdriver/Tauri harness so missing Tauri invocation support, driver startup failures, and application panics fail the run. Remove or resolve the repeated `Tauri core.invoke not available` warnings instead of accepting a green test with a degraded harness.

### CI gate

Run the critical desktop workflow on Windows, macOS, and Linux. Preserve temporary test artifacts and logs only when a job fails.

## Phase 3: Architecture and User-Experience Cleanup

### Dependency direction

Remove `git-engine` from `app-service` production dependencies. Tests may use `git-engine::FakeBackend` through `dev-dependencies`. The normal Cargo dependency graph must keep `app-service` dependent on the `GitBackend` trait in `git-core`, not on an implementation crate.

### GitLab MR internationalization

Move all user-visible GitLab merge-request detail labels, actions, status text, placeholders, accessibility labels, and toast messages into the existing Chinese and English locale dictionaries. Server-provided names and status values remain data, but surrounding explanatory copy is localized.

Locale tests must demonstrate both English and Chinese output for representative detail, approval, merge, comment, and pipeline-job states.

### Frontend code splitting

Lazy-load heavyweight, infrequently opened collaboration panels and merge-editor code. Keep the launch shell and daily status/history paths in the initial bundle. Add a build-time bundle budget that fails when the initial JavaScript chunk exceeds the agreed threshold; the initial threshold is 500 kB uncompressed to match the existing Vite warning boundary.

Loading states must use the existing spinner and visual tokens. A chunk-load failure must surface through the existing toast/error boundary rather than leave an empty panel.

## Error Handling

- Release preflight failures use actionable messages naming the missing variable or inconsistent file.
- E2E fixture failures cross IPC as structured `IpcError` values.
- No new Git call may run directly in an async Tauri command; Git and filesystem fixture work must use `spawn_blocking`.
- Production application behavior must not depend on the E2E feature.
- Collaboration API errors continue to preserve server error details while local surrounding messages are translated.

## Testing Strategy

Behavior changes follow red-green-refactor:

- Unit tests for version/tag/updater/signing preflight rules.
- A negative production-build check proving the E2E fixture command is absent without the feature.
- Desktop E2E for the local init-to-history workflow.
- Cargo dependency-tree assertion for the `app-service` boundary.
- Vitest coverage for GitLab MR translations.
- Build-output assertion for the initial bundle budget.

Final verification:

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

The release preflight is additionally executed in a test mode with deterministic fake environment inputs. Real signing and updater secrets are verified only by the protected release workflow.

## Completion Criteria

- Repository-local release checks fail closed for incomplete tag releases.
- CSP is enabled and collaboration requests still work.
- macOS architecture support is explicit.
- The E2E suite proves a real local Git commit loop, not only shell visibility.
- `app-service` no longer has a normal dependency on `git-engine`.
- GitLab MR detail contains no untranslated product copy in Chinese mode.
- The initial JavaScript bundle is below 500 kB uncompressed.
- All local and CI verification gates pass.
- Documentation clearly separates completed engineering from operator-owned credentials and real-device acceptance.
