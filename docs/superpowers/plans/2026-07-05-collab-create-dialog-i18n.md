# Collaboration Create Dialog i18n Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Translate GitHub PR and GitLab MR create dialogs through locale dictionaries.

**Architecture:** Extend locale dictionaries with `collabCreate.*`, then replace hard-coded strings in `GithubCreatePrDialog` and `GitlabCreateMrDialog`. Tests assert visible language changes without changing API behavior.

**Tech Stack:** React, TypeScript, Vitest, Testing Library, existing `useT()`.

---

## Task 1: Failing i18n Tests

- [x] **Write failing tests**

Add language-specific assertions to existing create-dialog component tests.

- [x] **Run red tests**

Run focused dialog tests and confirm failure on hard-coded text.

## Task 2: Locale and Component Wiring

- [x] **Add locale keys**

Add matching `collabCreate.*` keys to `zh.ts` and `en.ts`.

- [x] **Wire GitHub dialog**

Replace visible strings and toasts with `useT()`.

- [x] **Wire GitLab dialog**

Replace visible strings and toasts with `useT()`.

- [x] **Run green focused tests**

Run GitHub/GitLab create dialog tests.

## Task 3: Verification

- [x] **Run frontend tests**
- [x] **Run TypeScript**
- [x] **Run production build**
- [x] **Update handoff**
