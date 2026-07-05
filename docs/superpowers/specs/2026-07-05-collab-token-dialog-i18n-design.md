# Collaboration Token Dialog i18n Design

## Goal
Move GitHub and GitLab token dialogs off hard-coded mixed Chinese/English strings and onto the existing `useT()` i18n system.

## Scope
- GitHub token dialog.
- GitLab token dialog.
- Success toast copy, close labels, status labels, action buttons, token field label, and placeholders.

## Non-Goals
- Full PR/MR panel translation.
- Backend or IPC changes.
- Token storage behavior changes.

## Approach
Add provider-neutral keys for common token dialog structure plus provider-specific title and placeholder keys. The two dialogs keep separate components because their IPC functions differ, but share the same translation key shape.

Tests render the dialogs after setting `setLang("en")` and `setLang("zh")` to ensure visible copy follows the language store.

## Verification
- Focused component test for both token dialogs.
- Full frontend Vitest suite.
- TypeScript compile.
- Production build.
