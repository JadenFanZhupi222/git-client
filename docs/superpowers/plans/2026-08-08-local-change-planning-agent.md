# Local Change Planning Agent

## Confirmed product scope

The Changes workspace gains a local change-planning agent that remains useful
without an API key. It analyzes staged, unstaged, and untracked changes, reports
risks, proposes conservative atomic commit groups, and drafts commit messages.
The user may explicitly confirm one group at a time for staging and commit.

The first release never pushes, resets, rebases, deletes files, switches branches,
or silently changes the index.

## Architecture

- `review-agent` owns the provider-neutral change-planning domain alongside the
  existing review and issue workflows. Deterministic planning is the authoritative
  source of file grouping and executability.
- The optional model pass may improve summaries, rationales, and commit messages,
  but cannot add, remove, or move files between groups.
- Tauri gathers status, bounded structured diffs, and recent commit-message style
  through `RepoRegistry`. Repository paths and credentials never cross into the
  WebView result contract.
- A stable snapshot ID covers HEAD, status entries, and diff evidence. Both analysis
  and execution use the same snapshot rules.

## Deterministic behavior

- Preserve all currently staged files as one group. Never silently unstage them.
- Group unstaged files conservatively by repository area, keeping source and nearby
  tests together where possible.
- Flag conflicts, partially staged paths, suspected secrets, generated artifacts,
  binary or oversized changes, large change sets, and source changes without tests.
- Produce a local fallback summary, rationale, and repository-style commit message.

## Confirmed execution

- Re-read the complete snapshot immediately before any write.
- A staged group is executable only when it exactly matches the current index.
- An unstaged group is executable only when the index is empty.
- Stage only the selected group's paths, then commit its confirmed non-empty message.
- If staging or the commit fails, unstage files added by this operation so the index
  returns to its previous clean state.
- After success, invalidate worktree and history queries; the remaining plan is stale
  and must be regenerated.

## Provider enhancement

- Local analysis is the default and performs no network calls.
- Users explicitly select an allowlisted model to enhance the plan.
- Only bounded textual diff evidence and recent commit-message examples are sent.
- Binary content, credentials, repository filesystem paths, and ignored files are
  never included.

## Verification

- Domain tests cover stable grouping, warning detection, message inference, model
  output validation, and attempts to mutate group membership.
- Command tests cover snapshot races, dirty-index refusal, exact staged-group
  matching, rollback after failed commit, and zero writes without confirmation.
- UI tests cover local mode, enhanced mode, group editing, explicit confirmation,
  stale-plan recovery, and disabled unsafe groups.
- Run formatting, strict Clippy, dependency boundaries, frontend build/tests, and
  the complete Rust workspace suite.
