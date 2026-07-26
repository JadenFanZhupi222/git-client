# Production Readiness Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the existing 0.1.x prerelease into a release-candidate-quality desktop build with fail-closed release checks, meaningful desktop E2E coverage, clean dependency boundaries, complete GitLab MR translations, and a bounded initial frontend bundle.

**Architecture:** Keep release validation in a pure Node module so it is testable without secrets or GitHub Actions. Keep all E2E-only filesystem setup behind the Rust `e2e` feature and `spawn_blocking`. Preserve the existing `GitBackend` boundary by moving FakeBackend to test-only dependencies, and reduce frontend startup cost with React lazy boundaries plus a post-build chunk-budget check.

**Tech Stack:** Tauri 2, Rust 2024, React 19, TypeScript 5.8, Vite 7, Vitest 4, WebdriverIO, pnpm, GitHub Actions.

---

### Task 1: Add a fail-closed release preflight

**Files:**
- Create: `scripts/release-preflight.mjs`
- Create: `scripts/release-preflight.test.mjs`
- Modify: `app/package.json`
- Modify: `.github/workflows/build-artifacts.yml`

- [ ] **Step 1: Write failing preflight tests**

Use `node:test` to cover version mismatch, tag mismatch, placeholder updater config, incomplete updater signing inputs, incomplete Windows signing inputs, incomplete macOS signing inputs, and a valid release configuration:

```js
import test from "node:test";
import assert from "node:assert/strict";
import { validateRelease } from "./release-preflight.mjs";

test("rejects inconsistent application versions", () => {
  const errors = validateRelease({
    versions: { package: "0.1.3", tauri: "0.1.4", cargo: "0.1.3" },
    tag: "app-v0.1.3",
    updater: { pubkey: "real", endpoints: ["https://example.test/latest.json"] },
    env: completeEnv,
    platform: "windows",
  });
  assert.match(errors.join("\n"), /versions differ/);
});
```

- [ ] **Step 2: Run the test and verify RED**

Run: `node --test scripts/release-preflight.test.mjs`

Expected: FAIL because `release-preflight.mjs` does not exist.

- [ ] **Step 3: Implement the pure validator and CLI**

Export `validateRelease(input): string[]`. The CLI reads all three version files, checks `GITHUB_REF_NAME`, applies environment overrides used by the workflow, prints all failures, and exits with code 1 for release mode. `--allow-unsigned` skips credential requirements but never skips version consistency.

- [ ] **Step 4: Wire scripts and workflow**

Add:

```json
"release:check": "node ../scripts/release-preflight.mjs"
```

Run the strict preflight before tag release jobs. Keep workflow-dispatch artifacts on `--allow-unsigned`.

- [ ] **Step 5: Verify GREEN**

Run:

```powershell
node --test scripts/release-preflight.test.mjs
pnpm -C app release:check -- --allow-unsigned
```

Expected: all validator tests pass and the local unsigned check succeeds.

- [ ] **Step 6: Commit**

Commit only the preflight files and workflow integration.

### Task 2: Enable CSP and make the distribution matrix explicit

**Files:**
- Modify: `app/src-tauri/tauri.conf.json`
- Modify: `.github/workflows/build-artifacts.yml`
- Modify: `docs/RELEASE.md`

- [ ] **Step 1: Add a failing configuration test**

Extend `scripts/release-preflight.test.mjs` with a test that rejects a null CSP and accepts:

```text
default-src 'self'; connect-src ipc: http://ipc.localhost https://api.github.com https://gitlab.com https:; img-src 'self' asset: http://asset.localhost data: blob:; style-src 'self' 'unsafe-inline'; font-src 'self' data:; object-src 'none'; frame-src 'none'; base-uri 'none'
```

- [ ] **Step 2: Run the test and verify RED**

Run: `node --test scripts/release-preflight.test.mjs`

Expected: FAIL because null CSP is not checked.

- [ ] **Step 3: Implement CSP validation and configuration**

Set a non-null CSP in `tauri.conf.json`. Reject missing `default-src 'self'`, `object-src 'none'`, or `frame-src 'none'`.

- [ ] **Step 4: Add explicit macOS architecture jobs**

Use separate Apple Silicon and Intel matrix entries with architecture-specific Rust targets and artifact names. Document which runner/target produces each artifact and retain Windows x64 and Linux x86_64.

- [ ] **Step 5: Verify GREEN**

Run:

```powershell
node --test scripts/release-preflight.test.mjs
pnpm -C app build
```

Expected: tests and production frontend build pass.

- [ ] **Step 6: Commit**

Commit CSP, matrix, and release documentation together.

### Task 3: Restore the GitBackend dependency boundary

**Files:**
- Modify: `crates/app-service/Cargo.toml`
- Create: `scripts/check-dependency-boundaries.ps1`
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Add a failing dependency assertion**

Create a PowerShell check that parses:

```powershell
$tree = cargo tree -p app-service -e normal --depth 1
if ($tree -match "(?m)^[├└].*git-engine ") {
  throw "app-service must not have a normal dependency on git-engine"
}
```

- [ ] **Step 2: Run the check and verify RED**

Run: `pwsh -File scripts/check-dependency-boundaries.ps1`

Expected: FAIL because `git-engine` is currently a normal dependency.

- [ ] **Step 3: Move FakeBackend to test-only dependency**

Delete the normal `git-engine` dependency and its forwarding feature from `app-service`; retain:

```toml
[dev-dependencies]
git-engine = { path = "../git-engine", default-features = false }
```

- [ ] **Step 4: Add the boundary check to CI**

Run it after Cargo tests on all platforms using `pwsh`.

- [ ] **Step 5: Verify GREEN**

Run:

```powershell
pwsh -File scripts/check-dependency-boundaries.ps1
cargo test -p app-service
cargo tree -p app-service -e normal --depth 1
```

Expected: no normal `git-engine` dependency and all app-service tests pass.

- [ ] **Step 6: Commit**

Commit the manifest and boundary gate.

### Task 4: Complete GitLab MR detail internationalization

**Files:**
- Modify: `app/src/lib/locales/en.ts`
- Modify: `app/src/lib/locales/zh.ts`
- Modify: `app/src/components/GitlabMrPanel.tsx`
- Modify: `app/src/components/GitlabMrPanel.test.tsx`

- [ ] **Step 1: Write failing locale behavior tests**

Render representative MR details after `setLang("zh")` and assert localized metrics, approval, pipeline, merge, and comment controls. Repeat a smaller assertion in English. Include translated token-required and success toasts.

- [ ] **Step 2: Run the focused test and verify RED**

Run: `pnpm -C app test -- src/components/GitlabMrPanel.test.tsx`

Expected: FAIL on existing hard-coded English strings.

- [ ] **Step 3: Add typed locale keys**

Add `gitlabMrDetail.*` keys for all labels, actions, statuses, placeholders, accessibility labels, blocked reasons, and toast templates to both dictionaries.

- [ ] **Step 4: Replace product copy with `useT()`**

Pass the translator through the details component or call `useT()` there. Keep API-provided names and raw status values as data, localizing only surrounding copy.

- [ ] **Step 5: Verify GREEN**

Run:

```powershell
pnpm -C app test -- src/components/GitlabMrPanel.test.tsx
pnpm -C app build
```

Expected: focused tests and type-checked build pass.

- [ ] **Step 6: Commit**

Commit translations, component changes, and tests.

### Task 5: Split heavyweight frontend panels and enforce a bundle budget

**Files:**
- Modify: `app/src/App.tsx`
- Modify: `app/vite.config.ts`
- Create: `scripts/check-bundle-size.mjs`
- Create: `scripts/check-bundle-size.test.mjs`
- Modify: `app/package.json`

- [ ] **Step 1: Write failing budget tests**

Test a pure `findOversizedEntryChunks(manifest, maxBytes)` helper with one under-budget manifest and one 501 kB entry chunk.

- [ ] **Step 2: Run and verify RED**

Run: `node --test scripts/check-bundle-size.test.mjs`

Expected: FAIL because the checker does not exist.

- [ ] **Step 3: Implement the budget checker**

Enable Vite manifest output. Inspect entry chunks only and fail above 500,000 bytes with the chunk name and actual size.

- [ ] **Step 4: Lazy-load infrequent panels**

Replace eager imports for GitHub/GitLab panels and dialogs, CodeMirror-backed conflict UI where the import boundary permits, and other infrequent overlays with `React.lazy`. Wrap mounted overlays in a shared `Suspense` fallback using the existing spinner.

- [ ] **Step 5: Wire the build gate**

Make `pnpm build` run TypeScript, Vite, then the bundle checker.

- [ ] **Step 6: Verify GREEN**

Run:

```powershell
node --test scripts/check-bundle-size.test.mjs
pnpm -C app build
```

Expected: budget tests pass and the initial entry chunk is at or below 500,000 bytes.

- [ ] **Step 7: Commit**

Commit lazy boundaries and budget tooling.

### Task 6: Add an isolated E2E Git commit loop

**Files:**
- Modify: `app/src-tauri/src/lib.rs`
- Modify: `app/src/ipc.ts`
- Modify: `app/src/App.tsx`
- Modify: `app/src/views/ChangesView.tsx`
- Modify: `app/src/views/HistoryView.tsx`
- Modify: `app/e2e/smoke.e2e.js`
- Modify: `app/wdio.conf.js`
- Modify: `.gitignore`

- [ ] **Step 1: Write failing Rust fixture tests**

Under `#[cfg(all(test, feature = "e2e"))]`, test safe root containment, repository initialization, deterministic Git identity setup, and fixture file creation. Verify traversal outside the fixture root returns an `IpcError`.

- [ ] **Step 2: Run Rust tests and verify RED**

Run: `cargo test -p app --features e2e e2e_fixture`

Expected: FAIL because the fixture helpers do not exist.

- [ ] **Step 3: Implement feature-gated fixture commands**

Add `e2e_prepare_repo` and `e2e_write_file` only under `feature = "e2e"`. Execute filesystem and Git work inside `spawn_blocking`; validate canonical paths before writing. Register commands through an E2E-specific builder branch so production builds cannot invoke them.

- [ ] **Step 4: Add stable selectors**

Add `data-testid` attributes for repository shell, unstaged file row, stage action, commit message, commit action, history tab, and history commit subject. These selectors carry no E2E-only production behavior.

- [ ] **Step 5: Replace launch-only smoke with the commit loop**

Use the Tauri global invocation API to prepare/open a temporary repository, write `hello.txt`, drive stage and commit through visible UI controls, and assert `e2e initial commit` appears in history. Store artifacts under `app/.e2e-tmp/<run-id>`.

- [ ] **Step 6: Make harness warnings fail**

Capture frontend/backend logs and fail `onComplete` for application panic, invoke-unavailable warnings, or driver startup errors. Keep logs on failure and remove `.e2e-tmp` on success.

- [ ] **Step 7: Verify GREEN**

Run:

```powershell
cargo test -p app --features e2e e2e_fixture
pnpm -C app e2e:ci
```

Expected: fixture tests and the init-to-history desktop workflow pass without Tauri invoke warnings.

- [ ] **Step 8: Commit**

Commit the E2E fixture, selectors, harness, and scenario.

### Task 7: Synchronize release documentation and gates

**Files:**
- Modify: `README.md`
- Modify: `docs/RELEASE.md`
- Modify: `docs/HANDOFF.md`
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Add documentation consistency assertions**

Extend the preflight test to read docs and reject the stale claims “Cross-platform CI should be tightened” and “main 领先 origin”.

- [ ] **Step 2: Run and verify RED**

Run: `node --test scripts/release-preflight.test.mjs`

Expected: FAIL while stale text remains.

- [ ] **Step 3: Update documents**

Record implemented three-platform CI/E2E, strict tag release requirements, unsigned manual artifacts, updater/signing operator inputs, CSP, supported architectures, and remaining real-device acceptance work.

- [ ] **Step 4: Tighten CI**

Run release validator tests and bundle-budget tests in the frontend job, and the dependency boundary assertion in the Rust job.

- [ ] **Step 5: Verify GREEN**

Run:

```powershell
node --test scripts/release-preflight.test.mjs
node --test scripts/check-bundle-size.test.mjs
pnpm -C app release:check -- --allow-unsigned
```

Expected: all checks pass and stale statements are absent.

- [ ] **Step 6: Commit**

Commit documentation and final CI gates.

### Task 8: Run the complete release-candidate verification

**Files:**
- Verify only; modify files only for failures attributable to this plan.

- [ ] **Step 1: Run source and dependency gates**

```powershell
pnpm -C app install --frozen-lockfile
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
pwsh -File scripts/check-dependency-boundaries.ps1
```

- [ ] **Step 2: Run all automated tests**

```powershell
cargo test --workspace
node --test scripts/release-preflight.test.mjs
node --test scripts/check-bundle-size.test.mjs
pnpm -C app test
```

- [ ] **Step 3: Run production and desktop builds**

```powershell
pnpm -C app build
pnpm -C app e2e:ci
pnpm -C app tauri build --ci --no-sign
```

- [ ] **Step 4: Audit results**

Confirm:

- no failing or ignored release gate;
- no `Tauri core.invoke not available` warning;
- initial entry JavaScript is at or below 500,000 bytes;
- MSI and NSIS bundles exist;
- `git status` contains only intentional changes and the user's pre-existing untracked `AGENTS.md`.

- [ ] **Step 5: Commit any verification-only corrections**

Do not stage `AGENTS.md`. Report signing, notarization, updater secrets, and physical-device acceptance as external remaining work.
