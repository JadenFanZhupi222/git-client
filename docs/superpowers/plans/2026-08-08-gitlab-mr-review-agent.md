# GitLab Merge Request Review Agent

## Outcome

Extend the existing code-review agent horizontally from GitHub pull requests to
GitLab merge requests. The model runtime, tool loop, safety budgets, editable
drafts, explicit publication confirmation, diagnostics, and model catalog remain
shared.

## Architecture

- Add a `GitlabReviewSource` implementation of the existing `ReviewSource`
  contract. It uses GitLab's current `/diffs` endpoint, reads repository files at
  the pinned MR head SHA, and publishes confirmed findings as diff discussions.
- Keep `ReviewTarget` host-neutral for this milestone: `pull_number` represents
  GitHub PR number or GitLab MR IID inside the selected source adapter.
- Parameterize the desktop review command service by hosting provider while
  preserving the existing GitHub IPC API. Add GitLab-specific IPC commands so
  existing callers and persisted data remain compatible.
- Reuse `PrReviewWorkspace` with a platform option. UI copy, cache keys,
  credentials, progress, and publication links adapt to the selected platform.

## GitLab protocol

- `GET /projects/:id/merge_requests/:iid` pins `sha` and validates races.
- `GET /projects/:id/merge_requests/:iid/diffs` retrieves paginated changed-file
  patches. Collapsed, too-large, missing, and oversized patches are not sent to
  the model.
- `GET /projects/:id/repository/tree` and repository file raw endpoints provide
  bounded tools at the pinned SHA.
- `GET /projects/:id/merge_requests/:iid/versions` supplies base/start/head SHAs.
- `POST /projects/:id/merge_requests/:iid/discussions` publishes each confirmed
  line comment. A head recheck happens immediately before publication.

## Safety and failure semantics

- No GitLab mutation occurs during preflight or analysis.
- The user selects findings and edits comments before publication.
- A changed MR head rejects publication and requires a fresh analysis.
- GitLab and GitHub review caches and in-flight resource locks are isolated.
- A partial GitLab publication reports the created discussion count and does not
  retry automatically, preventing duplicate comments.

## Verification

- Wiremock tests cover URL encoding, paginated diffs, exact-SHA reads, head race,
  GitLab position mapping, rate limits, and partial publication.
- Command-service tests cover provider credential routing and resource isolation.
- React tests cover GitLab workspace launch, IPC selection, cache isolation,
  credential recovery, and explicit publication confirmation.
- Run formatter, lints, focused suites, then full Rust and frontend regressions.
