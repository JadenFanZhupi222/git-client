# Diff Syntax Highlight Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add syntax highlighting to text diffs and the three-pane conflict editor without changing Rust backend contracts.

**Architecture:** Add a small frontend syntax utility for path detection and visible-line tokenization, then wire it into `DiffView` and `ConflictEditor`. The diff view keeps virtualization and word-level emphasis; CodeMirror panes get a language extension and token theme.

**Tech Stack:** React 19, TypeScript, Vitest, CodeMirror 6, existing Tailwind/CSS tokens.

---

## Files
- Create: `app/src/lib/syntax.ts`
- Create: `app/src/lib/syntax.test.ts`
- Modify: `app/src/components/DiffView.tsx`
- Modify: `app/src/components/ConflictEditor.tsx`
- Modify: `app/src/index.css`
- Update: `docs/HANDOFF.md`

## Task 1: Syntax Utility

- [x] **Step 1: Write failing tests**

Create `app/src/lib/syntax.test.ts` with tests for language detection and representative tokenization.

- [x] **Step 2: Run red test**

Run: `pnpm --dir app vitest run src/lib/syntax.test.ts`

Expected: fail because `./syntax` does not exist.

- [x] **Step 3: Implement syntax utility**

Create `app/src/lib/syntax.ts` with `languageIdForPath`, `highlightCodeLine`, and token types.

- [x] **Step 4: Run green test**

Run: `pnpm --dir app vitest run src/lib/syntax.test.ts`

Expected: pass.

## Task 2: DiffView Integration

- [x] **Step 1: Write failing rendering test**

Add a focused test that renders `DiffView` with a `.ts` file diff and expects keyword/string token spans to exist.

- [x] **Step 2: Run red test**

Run: `pnpm --dir app vitest run src/components/DiffView.test.tsx`

Expected: fail because syntax spans are not rendered.

- [x] **Step 3: Integrate highlighter**

Pass `diff.path` language into `LineContent`, render syntax spans for plain and emphasized segments, and preserve add/delete emphasis classes.

- [x] **Step 4: Run green test**

Run: `pnpm --dir app vitest run src/components/DiffView.test.tsx src/lib/syntax.test.ts`

Expected: pass.

## Task 3: ConflictEditor Integration

- [x] **Step 1: Write failing utility-level test or component smoke test**

Cover that `ConflictEditor` derives a language from `file` and includes syntax support for known extensions.

- [x] **Step 2: Run red test**

Run the focused test command from Step 1.

Expected: fail before CodeMirror language support is wired.

- [x] **Step 3: Add CodeMirror language extension**

Install required CodeMirror language packages if missing with `pnpm --dir app add`, import the selected extensions, and add a token theme to `baseTheme`.

- [x] **Step 4: Run green test**

Run the focused test command from Step 1.

Expected: pass.

## Task 4: Verification

- [x] **Run frontend tests**

Run: `pnpm --dir app test`

- [x] **Run TypeScript**

Run: `pnpm --dir app exec tsc -p tsconfig.json --noEmit`

- [x] **Run production build**

Run: `pnpm --dir app build`

- [x] **Update handoff**

Record diff syntax highlighting as complete in `docs/HANDOFF.md`, including any known limitations.
