# GitHub PR Panel Shell i18n Design

## Goal
Move the GitHub PR panel shell text onto the existing `useT()` i18n system.

## Scope
- Dialog aria label and title.
- Remote/branch fallback text.
- Missing remote / missing branch error messages.
- Loading and empty states.
- List row action buttons: open/details/loading.
- Footer buttons: token/refresh/close.

## Non-Goals
- PR detail metrics, check-runs, comments, review threads, merge controls, or comment controls.
- API behavior changes.
- GitLab MR panel changes.

## Approach
Add `githubPr.*` locale keys and use them in `GithubPrPanel`. Existing behavior tests continue to cover API calls; a new focused test locks Chinese visible text for the shell.

## Verification
- Focused `GithubPrPanel` tests.
- Full frontend Vitest suite.
- TypeScript and production build.
