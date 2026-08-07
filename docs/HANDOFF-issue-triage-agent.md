# GitHub Issue Triage Agent Handoff

## Status

A3 read-only triage and A4 human-confirmed publication are implemented on
`codex/issue-triage-agent`. The implementation is ready for review and manual
credential-backed acceptance. GitHub writes are limited to adding existing
labels and posting a comment after an explicit confirmation step.

The final hardening pass also makes the app re-read the issue immediately
before opening triage. A locally cached result is accepted only when its
`updated_at` and comment-count snapshot matches that fresh read; otherwise the
cache is deleted and the user is explicitly asked to run triage again.

## Delivered behavior

- A GitHub Issues workspace lists open issues, filters pull requests out, and
  displays the selected issue, labels, body, and bounded comment history.
- The AI triage dialog requires workflow-specific consent and uses the existing
  backend-owned model catalog and credentials.
- The backend pins each run to the issue `updated_at` value and comment count,
  then checks that snapshot again before making a model request.
- Model input contains the issue, bounded comments, repository labels, and up
  to five same-repository duplicate candidates.
- Output is constrained to summary, category, priority, confidence, existing
  labels, supplied duplicate candidates, a reply draft, and rationale.
- Deterministic validation removes invented labels and duplicate numbers.
  Unstructured fallback text can become only a summary.
- Results are cached locally per repository and issue snapshot. Stale results
  are discarded rather than displayed as current.
- Cancellation shares the existing run registry.
- Publication defaults to no selected actions. The user must select individual
  labels and/or the reply draft, review the exact batch, and confirm it.
- The backend rejects an unconfirmed batch, a stale snapshot, unavailable
  labels, empty batches, excessive label counts, and oversized comments before
  issuing any write.
- The only write endpoints are additive existing-label updates and issue
  comments. There is no label removal or creation, assignment, close, lock, or
  other Issue mutation.
- Each action returns `applied`, `already_applied`, or `failed` independently.
  A partial result includes a fresh snapshot when available so failed actions
  can be retried safely.
- Every publication uses a stable `publish_id`. Labels are naturally
  idempotent; comments contain a hidden batch marker that is checked before a
  retry, preventing duplicate public comments after ambiguous network results.

## Main implementation locations

- `crates/review-agent/src/issue.rs`: domain contract, orchestration, model
  adapter, GitHub source, budgets, snapshot checks, publication validation,
  per-action results, and comment idempotency.
- `app/src-tauri/src/review_commands.rs`: credential-owned IPC services,
  progress, cancellation, and production backend construction.
- `crates/ipc-types/src/lib.rs`: stable DTO boundary and generated TypeScript
  bindings.
- `app/src/components/IssuesView.tsx`: list/detail workspace and fresh snapshot
  read before opening triage.
- `app/src/components/IssueTriageWorkspace.tsx`: consent, model/language choice,
  progress, result rendering, snapshot-aware cache, action selection, inline
  confirmation, publication, and safe retry.

## Verification completed

Run from the repository root:

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-dependency-boundaries.ps1
pnpm.cmd -C app test -- --run
pnpm.cmd -C app build
```

The final pass includes explicit coverage for fresh context reads, stale local
cache removal, zero writes without confirmation, zero writes for stale or
tampered batches, per-action partial results, retry snapshot advancement, and
duplicate-comment prevention.

## Manual acceptance still required

Use a non-production test repository and test credentials:

1. Open a GitHub-backed repository and confirm the Issues workspace excludes
   pull requests and renders issue details.
2. Open AI Triage and confirm the workflow-specific disclosure appears before
   any model request.
3. Run both supported DeepSeek models in Chinese and English and confirm the
   result starts with every publication action unselected.
4. Update the issue or add a comment on GitHub, reopen triage, and confirm the
   saved result is discarded and the stale-result notice appears.
5. Select one existing label and the reply, verify the exact confirmation view,
   then publish and inspect the action results and GitHub Issue.
6. Confirm no UI or request can remove/create labels, assign, close, or lock the
   Issue.
7. Simulate a partial/network failure, retry, and confirm the comment appears
   only once.
8. Start a long analysis request, cancel it, and confirm the dialog returns to
   an idle state without a result.

## Next decision

After manual acceptance, merge A3/A4 or return to A1/A2 if provider diversity
is the higher priority. Further mutation types must not be added to this
contract. A5 should unify run concurrency, bounded transient retries,
diagnostics, and the provider × workflow contract matrix.
