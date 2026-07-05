# Collaboration Token Dialog i18n Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Translate GitHub/GitLab token dialogs through the existing i18n dictionary.

**Architecture:** Add locale keys to `zh.ts`/`en.ts`, then replace hard-coded strings in the two token dialogs with `useT()`. Tests drive the language-switch behavior.

**Tech Stack:** React, TypeScript, Vitest, Testing Library, existing `app/src/lib/i18n.ts`.

---

## Task 1: Token Dialog i18n Tests

- [x] **Write failing tests**

Create a component test that sets language to English/Chinese and renders GitHub/GitLab token dialogs.

- [x] **Run red test**

Run: `app/node_modules/.bin/vitest.cmd run src/components/TokenDialogs.test.tsx`

Expected: fail because dialog text is hard-coded.

## Task 2: Locale Keys and Component Wiring

- [x] **Add locale keys**

Add matching `collabToken.*` keys to `zh.ts` and `en.ts`.

- [x] **Wire GitHubTokenDialog**

Import `useT()` and replace all UI strings/toast strings.

- [x] **Wire GitLabTokenDialog**

Import `useT()` and replace all UI strings/toast strings.

- [x] **Run green test**

Run: `app/node_modules/.bin/vitest.cmd run src/components/TokenDialogs.test.tsx`

Expected: pass.

## Task 3: Verification

- [x] **Run frontend tests**

Run: `app/node_modules/.bin/vitest.cmd run`

- [x] **Run TypeScript**

Run: `app/node_modules/.bin/tsc.cmd -p tsconfig.json --noEmit`

- [x] **Run production build**

Run: `app/node_modules/.bin/vite.cmd build`

- [x] **Update handoff**

Record the i18n slice in `docs/HANDOFF.md`.
