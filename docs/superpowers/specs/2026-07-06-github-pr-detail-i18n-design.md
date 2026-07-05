# GitHub PR Detail i18n Design

## Scope

Translate the static detail-body copy inside `GithubPrPanel` after a user opens PR details: metric labels, count units, section headings, merge-method label/options, merge/comment buttons, comment label and placeholder, empty comment fallbacks, and small review-summary text.

Out of scope: API statuses such as `mergeable`, `success`, `failure`, CI context names, user names, dates, toast messages, merge blocking reasons, and GitLab MR details.

## Approach

Add a `githubPrDetail.*` locale namespace in `en.ts` and `zh.ts`. `PullRequestDetailsView` will use `useT()` directly, keeping parent props focused on data and actions.

The existing English workflow test remains unchanged. A new Chinese detail test opens a mocked PR detail response and asserts the translated labels and controls.

## Acceptance

- Chinese mode renders GitHub PR detail controls and headings in Chinese.
- Existing English GitHub PR detail, refresh, comment, and merge tests continue passing.
- No Rust, IPC, DTO, or Git backend changes.
