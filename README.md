# Git Client

A production-oriented desktop Git client built with Tauri 2, React, and a multi-crate Rust workspace.

The long-term target is the quality bar of the Git tooling built into mature IDEs: fast on large repositories, safe for real work, predictable during complex operations, and easy to extend without mixing UI code with Git implementation details.

## Current Status

The app already covers the core daily Git workflow and several advanced workflows:

- Repository open, clone, and init
- Status, stage, unstage, hunk and line staging
- Commit and amend, with Git hooks and signing respected where routed through the CLI backend
- Commit history, graph view, reflog, file history, line history, blame, and pickaxe search
- Branch create, checkout, delete, merge, upstream tracking, fetch, pull, and push
- Remote management: add, rename, remove
- Stash, tag create/delete, cherry-pick, revert, reset, and interactive rebase
- Conflict handling with a three-pane CodeMirror merge editor
- Word-level diff, side-by-side diff, syntax highlighting, image diff, Git LFS pointer handling, submodule/worktree/sparse-checkout awareness
- GitHub and GitLab collaboration panels for tokens, PR/MR creation, and PR/MR review details
- GitHub PR and GitLab MR AI Review workspaces, plus GitHub Issue Triage, with snapshot-pinned analysis and human-confirmed publishing
- Provider-neutral agent runtime with DeepSeek, OpenAI Responses, and Anthropic Messages adapters and seven allowlisted models
- Unified credential settings for DeepSeek, OpenAI, Anthropic, GitHub, and GitLab secrets stored by the Rust backend
- Frontend test coverage for the major UI slices added during development

The 0.1.4 line now has release-candidate hardening: Linux/macOS/Windows CI,
a real desktop init-to-commit E2E loop, fail-closed tagged releases, a restrictive
CSP, dependency-boundary checks, complete GitLab MR detail localization, and an
enforced initial JavaScript bundle budget. A public production release still
requires operator-owned signing/notarization credentials, updater keys and
endpoint, plus final installation and visual acceptance on supported hardware.

## Tech Stack

- Desktop shell: Tauri 2.x
- Frontend: React 19, TypeScript, Vite, TanStack Query, CodeMirror, Tailwind CSS v4 tokens
- Backend: Rust workspace with separate domain, backend adapter, application service, IPC type, and Tauri crates
- Git backends: `git2` for common local operations, Git CLI for complex workflows and network/auth-sensitive operations, routed through `CompositeBackend`
- Type sharing: Rust DTOs exported to TypeScript through `ts-rs`
- Package manager: pnpm

## Repository Layout

```text
git-client/
  app/                    React frontend and Tauri shell
    src/                  UI, hooks, IPC wrappers, generated bindings
    src-tauri/            Tauri commands and desktop app bootstrap
  crates/
    agent-runtime/        Provider-neutral model requests, responses, capabilities, usage, and catalog metadata
    git-core/             Domain models, GitBackend trait, typed errors
    git-engine/           git2 / CLI backend implementations and routing
    app-service/          Use cases, repository context, cache, operation orchestration
    ipc-types/            DTOs shared across Rust and TypeScript
    review-agent/         Sandboxed PR review and Issue Triage workflows, provider adapters, validation, and traces
  docs/                   Handoff notes, feature plans, implementation specs
  Cargo.toml              Rust workspace
```

## Architecture Rules

These rules matter more than any individual implementation detail:

1. All blocking Git work must run inside `spawn_blocking` when called from Tauri async commands.
2. Upper layers depend on the `GitBackend` trait, not on `git2`, CLI, or any concrete backend.
3. `git-core` owns domain models and typed errors. It must stay independent of Tauri and backend implementation details.
4. `git-engine` implements backend adapters. Use the CLI backend for operations where native libraries do not faithfully cover real Git behavior, such as network/auth flows, hooks, signing, interactive rebase, and complex porcelain workflows.
5. `app-service` orchestrates use cases and repository state. UI components should not know backend details.
6. Tauri commands are thin adapters: validate input, call `app-service`, map errors to structured IPC errors, and never panic.
7. Frontend components should call typed IPC wrappers/hooks instead of calling Tauri `invoke` directly.
8. Frontend colors and typography should use theme tokens from `app/src/index.css`; do not hard-code ad hoc hex colors in components.

## Prerequisites

Install these before running the project:

- Rust via rustup, matching `rust-toolchain.toml`
- Node.js LTS
- pnpm
- Git, available on `PATH`
- Tauri system dependencies for your OS

Tauri prerequisites:

- macOS: Xcode Command Line Tools

  ```bash
  xcode-select --install
  ```

- Windows: Microsoft C++ Build Tools with MSVC and WebView2 Runtime
- Ubuntu/Debian:

  ```bash
  sudo apt update
  sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
    libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
  ```

For the most current platform list, use the official Tauri prerequisites documentation.

## Setup

```bash
git clone <repo-url>
cd git-client
pnpm --dir app install
```

If dependencies already exist, do not run `npm install` in `app/`. This project is configured for pnpm and npm can disturb the installed dependency layout.

## Development

Run the desktop app:

```bash
pnpm --dir app tauri dev
```

Run the frontend-only dev server:

```bash
pnpm --dir app dev
```

Run the frontend build:

```bash
pnpm --dir app build
```

## Verification

Backend checks:

```bash
cargo test --workspace
cargo clippy --workspace
cargo fmt --check
```

Frontend checks:

```bash
pnpm --dir app test
pnpm --dir app exec tsc -p tsconfig.json --noEmit
pnpm --dir app build
```

End-to-end checks:

```bash
pnpm --dir app e2e:ci
```

Release and architecture gates:

```bash
node --test scripts/release-preflight.test.mjs
node --test scripts/check-bundle-size.test.mjs
powershell -NoProfile -File scripts/check-dependency-boundaries.ps1
pnpm --dir app release:check -- --allow-unsigned
```

When a DTO changes, regenerate TypeScript bindings by running:

```bash
cargo test -p ipc-types
```

## Development Workflow

Most features should be implemented vertically:

```text
git-core trait/model
  -> git-engine backend implementation and tests
  -> app-service use case and fake-backend tests
  -> ipc-types DTO
  -> src-tauri command with spawn_blocking
  -> app/src/ipc.ts wrapper
  -> app/src/lib/queries.ts hook
  -> React UI
```

Small pure-frontend slices can skip the Rust layers, but should still have focused UI tests when behavior changes.

## Language Policy

Use English for new project-facing documentation, code comments, issue-style notes, commit descriptions, and user-visible source strings unless the feature is explicitly about localization.

The current repository still contains older Chinese handoff notes and some Chinese source comments. Treat those as legacy context rather than the preferred style for new work. New UI copy should go through the locale dictionaries instead of being hard-coded in components.

## Known Gaps

- Production signing/notarization credentials and updater key material are
  intentionally not stored in the repository; a release operator must provision
  them before an `app-v*` tag can publish.
- The updater endpoint remains a development placeholder until the production
  release channel exists.
- Installation, updater, and advanced diff/image-diff acceptance still need to
  be completed on real Windows, Linux, Intel Mac, and Apple Silicon machines.
- GitHub PR detail API-provided statuses and some collaboration strings remain
  candidates for further localization.
- AI Review supports GitHub pull requests and GitLab merge requests, while AI Issue
  Triage currently supports GitHub issues. Local code-editing agents remain a separate
  future security milestone.

## Useful Project Docs

- `docs/HANDOFF.md` records the latest implementation state and next-step context.
- `docs/HANDOFF-pr-review-agent.md` records the AI Review implementation and follow-up roadmap.
- `docs/HANDOFF-issue-triage-agent.md` records Issue Triage, human-confirmed publishing, safety boundaries, and the manual acceptance checklist.
- `docs/superpowers/plans/2026-08-08-multi-provider-agent-runtime.md` records the multi-provider runtime design, implementation decisions, and verification evidence.
- `ARCHITECTURE.md` contains the original full architecture write-up. Parts of it are older than the current codebase.
- `PRODUCT.md` describes product direction.
- `docs/superpowers/specs/` and `docs/superpowers/plans/` contain feature-level design and implementation notes.

## Troubleshooting

- If Rust compilation fails because the toolchain is too old, run `rustup update` and make sure rustup is the Rust provider.
- If Tauri fails to link or complains about WebKit/WebView dependencies, re-check the OS-specific Tauri prerequisites.
- If the app cannot open a repository, confirm the selected directory is a Git repository root or inside a valid Git worktree.
- If the UI freezes during a Git operation, audit the Tauri command path and confirm blocking Git work is inside `spawn_blocking`.
- If TypeScript bindings look stale after a Rust DTO change, run `cargo test -p ipc-types`.
