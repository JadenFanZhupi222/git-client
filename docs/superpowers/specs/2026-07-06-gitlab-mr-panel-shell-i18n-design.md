# GitLab MR Panel Shell i18n Design

## Scope

Translate only the shell copy in `GitlabMrPanel`: dialog label, title, remote and branch fallbacks, missing remote or branch errors, loading and empty states, list action buttons, and footer token/refresh/close actions.

The MR details body stays out of scope for this slice: approvals, notes, discussions, pipeline jobs, merge/comment controls, toast messages, and API payload behavior remain unchanged.

## Approach

Add a `gitlabMr.*` locale namespace beside `githubPr.*` in `app/src/lib/locales/en.ts` and `app/src/lib/locales/zh.ts`. `GitlabMrPanel` will call `useT()` and replace shell literals with dictionary keys.

Existing English behavior remains the default so current detail workflow tests keep asserting `Details`, `Refresh`, and other English detail strings. A new focused test switches to Chinese with `setLang("zh")` and verifies the empty panel shell.

## Acceptance

- Chinese mode renders the GitLab MR shell in Chinese.
- Existing GitLab MR detail, refresh, approval, comment, retry, and merge tests still pass in English.
- No Rust, IPC, DTO, or Git backend changes.
