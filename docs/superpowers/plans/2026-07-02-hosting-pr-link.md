# Hosting PR Link Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a v1 hosting integration that opens the current branch's GitHub/GitLab/Bitbucket create PR/MR page from the command palette.

**Architecture:** Keep provider detection and URL construction in a pure frontend module so it is easy to test. App state will reuse existing remote and branch queries, then call Tauri opener to launch the generated URL.

**Tech Stack:** React, Vitest, Tauri opener plugin, existing command palette.

---

### Task 1: Pure Hosting URL Builder

**Files:**

- Create: `app/src/lib/hosting.ts`
- Test: `app/src/lib/hosting.test.ts`

- [ ] Write failing tests for HTTPS and SSH remote URL parsing across GitHub, GitLab, and Bitbucket.
- [ ] Write failing tests for unsupported remotes and missing branches.
- [ ] Implement `buildCreateChangeRequestUrl(remotes, branch, preferredRemote)` with no Tauri dependencies.
- [ ] Run `pnpm -C app test -- src/lib/hosting.test.ts`.

### Task 2: Command Palette Integration

**Files:**

- Modify: `app/src/App.tsx`

- [ ] Import `openUrl` from `@tauri-apps/plugin-opener`.
- [ ] Import `buildCreateChangeRequestUrl`.
- [ ] Fetch full remote URLs with `useRemoteList(repo ?? "", !!repo)`.
- [ ] Add `openCreateChangeRequest()` that chooses `selectedRemote`, opens the generated URL, and shows toast feedback on unsupported configuration.
- [ ] Add command `remote:create-pr` in group `协作`.

### Task 3: Verification

**Commands:**

- `pnpm -C app test -- src/lib/hosting.test.ts`
- `pnpm -C app test`
- `pnpm -C app build`
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
