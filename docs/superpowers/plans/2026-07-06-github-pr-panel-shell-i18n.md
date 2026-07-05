# GitHub PR Panel Shell i18n Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Translate GitHub PR panel shell copy through locale dictionaries.

**Architecture:** Add `githubPr.*` keys to locales and wire only shell-level `GithubPrPanel` strings. Leave detail controls for a later dedicated slice.

**Tech Stack:** React, TypeScript, Vitest, Testing Library, existing `useT()`.

---

## Task 1: Failing Shell i18n Test

- [x] **Write failing test**
- [x] **Run red test**

## Task 2: Locale and Component Wiring

- [x] **Add locale keys**
- [x] **Wire panel shell strings**
- [x] **Run green focused tests**

## Task 3: Verification

- [x] **Run frontend tests**
- [x] **Run TypeScript**
- [x] **Run production build**
- [x] **Update handoff**
