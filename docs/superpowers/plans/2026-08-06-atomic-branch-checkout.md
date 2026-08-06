# Atomic Branch Checkout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent a failed branch checkout from leaving HEAD on the original branch while the index and working tree contain the target branch.

**Architecture:** Keep the public `GitBackend::checkout_branch` contract unchanged and contain the fix in `Git2Backend`. Add a real linked-worktree regression test, then introduce a preflight helper that rejects a target branch checked out elsewhere before `checkout_tree` performs any write; retain the existing safe-checkout conflict mapping.

**Tech Stack:** Rust 2024, git2 0.18/libgit2, tempfile, existing `git-engine` unit-test helpers.

---

### Task 1: Reproduce the partial checkout

**Files:**
- Modify: `crates/git-engine/src/git2_backend.rs:2768-2820`
- Test: `crates/git-engine/src/git2_backend.rs`

- [ ] **Step 1: Write the failing linked-worktree regression test**

Add a test named `checkout_branch_used_by_other_worktree_preserves_current_state`. It must create distinct commits on the original and `dev` branches, create a linked worktree for `dev` with `Repository::worktree`, call `checkout_branch("dev")` in the original worktree, and assert:

```rust
let before_branch = b.current_branch(&repo).unwrap();
let before_file = std::fs::read_to_string(repo.join("a.txt")).unwrap();
let before_index = index_blob_oid(&repo, "a.txt");

let err = b.checkout_branch(&repo, "dev").unwrap_err();

assert!(matches!(err, GitError::Backend(_)));
assert_eq!(b.current_branch(&repo).unwrap(), before_branch);
assert_eq!(std::fs::read_to_string(repo.join("a.txt")).unwrap(), before_file);
assert_eq!(index_blob_oid(&repo, "a.txt"), before_index);
```

Use a small test helper `index_blob_oid` that opens the repository index and returns the entry id for the supplied path. Keep the `TempDir` containing the linked worktree alive until all assertions finish.

- [ ] **Step 2: Run the regression test and verify RED**

Run:

```powershell
cargo test -p git-engine checkout_branch_used_by_other_worktree_preserves_current_state -- --nocapture
```

Expected: FAIL because `checkout_branch` returns an error but `a.txt` and/or its index entry changed to the `dev` version.

- [ ] **Step 3: Commit the isolated regression test**

```powershell
git add crates/git-engine/src/git2_backend.rs
git commit -m "test: 复现 worktree 分支半切换"
```

### Task 2: Reject an occupied branch before checkout writes

**Files:**
- Modify: `crates/git-engine/src/git2_backend.rs:1269-1300`
- Test: `crates/git-engine/src/git2_backend.rs`

- [ ] **Step 1: Add a focused occupancy helper**

Add a private helper near the checkout implementation with this contract:

```rust
fn branch_checked_out_in_other_worktree(
    repo: &git2::Repository,
    target_ref: &str,
) -> Result<Option<std::path::PathBuf>, git2::Error>
```

Enumerate `repo.worktrees()`, resolve each name with `repo.find_worktree(name)`, skip the current repository workdir after canonical path comparison, open the linked worktree repository, and compare `head.symbolic_target()` to `target_ref`. Return the occupying worktree path on a match. Invalid/prunable worktree metadata must surface as a backend error instead of permitting a potentially partial checkout.

- [ ] **Step 2: Call the helper before `checkout_tree`**

Immediately after resolving the target object and before constructing the mutating safe checkout, add:

```rust
if let Some(worktree) = branch_checked_out_in_other_worktree(&repo, &refname)
    .map_err(|e| GitError::Backend(e.to_string()))?
{
    return Err(GitError::Backend(format!(
        "分支 {name} 已在工作区 {} 中检出",
        worktree.display()
    )));
}
```

This check must run before any checkout builder that can write the worktree or index.

- [ ] **Step 3: Run the new test and verify GREEN**

Run:

```powershell
cargo test -p git-engine checkout_branch_used_by_other_worktree_preserves_current_state -- --nocapture
```

Expected: PASS; the call fails before the worktree or index changes.

- [ ] **Step 4: Run all checkout tests**

Run:

```powershell
cargo test -p git-engine checkout_
```

Expected: all checkout-related tests pass, including normal checkout, missing branch, dirty conflict, and occupied worktree.

- [ ] **Step 5: Commit the fix**

```powershell
git add crates/git-engine/src/git2_backend.rs
git commit -m "fix: 阻止 worktree 分支半切换"
```

### Task 3: Verify backend quality gates

**Files:**
- Modify only if formatting requires it: `crates/git-engine/src/git2_backend.rs`

- [ ] **Step 1: Format and inspect the patch**

Run:

```powershell
cargo fmt --all
git diff --check
git diff main...HEAD -- crates/git-engine/src/git2_backend.rs
```

Expected: no formatting or whitespace errors; diff contains only the regression test, helper, and preflight call.

- [ ] **Step 2: Run the git-engine suite**

Run:

```powershell
cargo test -p git-engine
```

Expected: all `git-engine` tests pass.

- [ ] **Step 3: Run workspace compile-quality checks**

Run:

```powershell
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
powershell -NoProfile -File scripts/check-dependency-boundaries.ps1
```

Expected: all commands exit 0 with no warnings promoted to errors.

- [ ] **Step 4: Commit formatting only if it changed tracked files**

```powershell
git add crates/git-engine/src/git2_backend.rs
git commit -m "style: 格式化 checkout 修复"
```

Skip this commit when `git status --short` is clean.
