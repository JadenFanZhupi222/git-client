# GitHub Token Input Copy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the ambiguous GitHub token-prefix placeholder with action-oriented localized copy and move prefix guidance into helper text.

**Architecture:** Keep the existing SettingsPanel rendering and localization keys. Change only GitHub locale values and lock them with component-level English and Chinese assertions.

**Tech Stack:** React 19, TypeScript locale dictionaries, Vitest, Testing Library.

---

### Task 1: Clarify GitHub token copy

**Files:**
- Modify: `app/src/components/SettingsPanel.test.tsx`
- Modify: `app/src/lib/locales/en.ts`
- Modify: `app/src/lib/locales/zh.ts`

- [ ] **Step 1: Write failing localized-copy assertions**

For an unconfigured GitHub provider, assert:

```tsx
expect(screen.getByPlaceholderText("Paste GitHub personal access token")).toBeInTheDocument();
expect(screen.getByText(
  "Supports tokens beginning with github_pat_ or ghp_. Stored securely in the system credential store.",
)).toBeInTheDocument();
```

Switch to Chinese and assert:

```tsx
expect(screen.getByPlaceholderText("粘贴 GitHub Personal Access Token")).toBeInTheDocument();
expect(screen.getByText(
  "支持以 github_pat_ 或 ghp_ 开头的令牌。凭据将安全存储于系统凭据库中。",
)).toBeInTheDocument();
```

- [ ] **Step 2: Verify the new test fails for the old prefix-only placeholder**

Run: `pnpm.cmd -C app exec vitest run src/components/SettingsPanel.test.tsx -t "explains GitHub token prefixes"`

Expected: FAIL because the current placeholder is `github_pat_... or ghp_...` and the helper contains only storage information.

- [ ] **Step 3: Update only GitHub locale values**

In `app/src/lib/locales/en.ts`:

```ts
"settings.github.placeholder": "Paste GitHub personal access token",
"settings.github.credentialHelper": "Supports tokens beginning with github_pat_ or ghp_. Stored securely in the system credential store.",
```

In `app/src/lib/locales/zh.ts`:

```ts
"settings.github.placeholder": "粘贴 GitHub Personal Access Token",
"settings.github.credentialHelper": "支持以 github_pat_ 或 ghp_ 开头的令牌。凭据将安全存储于系统凭据库中。",
```

Extend `ProviderMessageSuffix`, `providerMessageKey`, and the SettingsPanel helper lookup to use provider-specific `credentialHelper` keys. Add unchanged DeepSeek and GitLab helper values to both dictionaries so their UI copy remains identical.

- [ ] **Step 4: Run component and full frontend verification**

Run: `pnpm.cmd -C app exec vitest run src/components/SettingsPanel.test.tsx`

Expected: all SettingsPanel tests PASS.

Run: `pnpm.cmd -C app test`

Expected: all frontend tests PASS.

Run: `pnpm.cmd -C app build`

Expected: TypeScript, Vite, and bundle-size checks PASS.

- [ ] **Step 5: Commit the localized copy**

```bash
git add app/src/components/SettingsPanel.test.tsx app/src/components/SettingsPanel.tsx app/src/lib/locales/en.ts app/src/lib/locales/zh.ts
git commit -m "fix: clarify GitHub token input copy"
```
