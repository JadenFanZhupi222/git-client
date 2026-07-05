# Collaboration Create Dialog i18n Design

## Goal
Move GitHub PR and GitLab MR creation dialogs onto the existing `useT()` i18n system.

## Scope
- GitHub create PR dialog visible copy.
- GitLab create MR dialog visible copy.
- Missing-remote warning, labels, draft checkbox, token/cancel/create buttons, required-token toast, and success toast.

## Non-Goals
- PR/MR list panels.
- API payload changes.
- Branch/default-base behavior changes.
- Backend or IPC changes.

## Approach
Add shared `collabCreate.*` keys for common labels and provider-specific keys for titles, no-remote messages, required-token messages, draft wording, and success toasts. Existing tests keep payload behavior covered; new tests lock language-specific visible text.

## Verification
- Focused component i18n tests for English GitHub and Chinese GitLab.
- Existing create PR/MR behavior tests.
- Full Vitest, TypeScript, and production build.
