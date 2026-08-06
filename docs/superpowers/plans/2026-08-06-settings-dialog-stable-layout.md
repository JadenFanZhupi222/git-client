# Stable Settings Dialog Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep the Settings dialog shell and action placement stable across provider, loading, and credential-state changes.

**Architecture:** Keep `SettingsPanel` as the single settings surface, but change its right pane into explicit header, scroll-body, and action-footer rows. The active provider owns one content scroll ref so provider switches can reset it without changing credential behavior or accessibility contracts.

**Tech Stack:** React 19, TypeScript, Tailwind CSS v4 utilities, Vitest, Testing Library.

---

## File Structure

- Modify `app/src/components/SettingsPanel.tsx`: responsive shell dimensions, three-row content layout, shared action footer, and provider-scroll reset.
- Modify `app/src/components/SettingsPanel.test.tsx`: structural, action-footer, and scroll-reset regression coverage.

### Task 1: Lock the shell and action-footer structure

**Files:**
- Modify: `app/src/components/SettingsPanel.test.tsx`
- Modify: `app/src/components/SettingsPanel.tsx`

- [ ] **Step 1: Write failing layout tests**

Update the existing responsive-layout assertion to require the stable shell classes and add a shared-footer assertion:

```tsx
const dialog = screen.getByRole("dialog", { name: "Settings" });
expect(dialog).toHaveClass(
  "h-[min(680px,calc(100vh-48px))]",
  "w-[min(960px,calc(100vw-48px))]",
);

const actionBar = screen.getByTestId("settings-action-bar");
expect(actionBar).toHaveClass("shrink-0", "border-t", "bg-canvas");
expect(dialog.querySelectorAll(".overflow-y-auto")).toHaveLength(1);
expect(actionBar.closest(".overflow-y-auto")).toBeNull();
```

Extend the configured/unconfigured tests so both states assert the same action-bar element remains mounted while only relevant buttons change.

- [ ] **Step 2: Run the focused tests and verify failure**

Run: `pnpm -C app test -- SettingsPanel.test.tsx`

Expected: FAIL because the dialog still uses `w-[780px] max-w-[94vw]`, and no `settings-action-bar` exists outside provider content.

- [ ] **Step 3: Implement the stable three-row layout**

In `SettingsPanel.tsx`:

```tsx
className="panel-in popover relative flex h-[min(680px,calc(100vh-48px))] w-[min(960px,calc(100vw-48px))] max-h-none max-w-none flex-col overflow-hidden rounded-lg border border-line-strong bg-canvas"
```

Keep the integration title and provider tabs in the first right-pane row. Keep provider panels in the single `min-h-0 flex-1 overflow-y-auto` body. Move the current action buttons out of each tabpanel into this sibling footer:

```tsx
<div
  data-testid="settings-action-bar"
  className="flex shrink-0 flex-col items-stretch gap-2 border-t border-line bg-canvas px-3 py-4 min-[441px]:flex-row min-[441px]:items-center sm:px-5"
>
  {configured && <Button variant="danger">...</Button>}
  <div className="min-[441px]:ml-auto flex flex-col gap-2 min-[441px]:flex-row">
    {configured && <Button>...</Button>}
    <Button variant="primary">...</Button>
  </div>
</div>
```

Preserve the existing handlers, labels, busy states, responsive full-width behavior, and button ordering.

- [ ] **Step 4: Run the focused tests and verify success**

Run: `pnpm -C app test -- SettingsPanel.test.tsx`

Expected: all SettingsPanel tests PASS.

- [ ] **Step 5: Commit the stable shell**

```bash
git add app/src/components/SettingsPanel.tsx app/src/components/SettingsPanel.test.tsx
git commit -m "fix: stabilize settings dialog layout"
```

### Task 2: Reset provider-content scroll without moving focus

**Files:**
- Modify: `app/src/components/SettingsPanel.test.tsx`
- Modify: `app/src/components/SettingsPanel.tsx`

- [ ] **Step 1: Write the failing scroll-reset test**

Add a test that assigns a non-zero scroll position to the single provider scroll owner, switches provider through the tab, and verifies the position resets:

```tsx
const user = userEvent.setup();
renderPanel({ initialSection: "deepseek" });
await screen.findByText("Not configured");
const scrollOwner = screen.getByTestId("settings-provider-scroll");
Object.defineProperty(scrollOwner, "scrollTop", { value: 240, writable: true });
await user.click(screen.getByRole("tab", { name: "GitHub" }));
expect(scrollOwner.scrollTop).toBe(0);
expect(screen.getByRole("tab", { name: "GitHub" })).toHaveFocus();
```

- [ ] **Step 2: Run the focused test and verify failure**

Run: `pnpm -C app test -- SettingsPanel.test.tsx -t "resets provider content scroll"`

Expected: FAIL because provider switching does not reset the scroll owner.

- [ ] **Step 3: Implement the scroll owner ref and reset**

Add `const providerScrollRef = useRef<HTMLDivElement>(null);`, attach it with `data-testid="settings-provider-scroll"` to the existing scroll owner, and reset it inside `selectProvider` before state changes:

```tsx
providerScrollRef.current?.scrollTo({ top: 0 });
```

In the jsdom test environment, mock `scrollTo` to update `scrollTop`; production continues to use the native DOM method. Do not change provider-tab focus handling.

- [ ] **Step 4: Run the focused suite**

Run: `pnpm -C app test -- SettingsPanel.test.tsx`

Expected: all SettingsPanel tests PASS, including existing keyboard, focus, busy-state, i18n, and secret-handling coverage.

- [ ] **Step 5: Commit scroll behavior**

```bash
git add app/src/components/SettingsPanel.tsx app/src/components/SettingsPanel.test.tsx
git commit -m "fix: reset settings provider scroll"
```

### Task 3: Full verification and visual acceptance

**Files:**
- Verify: `app/src/components/SettingsPanel.tsx`
- Verify: `app/src/components/SettingsPanel.test.tsx`

- [ ] **Step 1: Run the full frontend tests**

Run: `pnpm -C app test`

Expected: all Vitest suites PASS.

- [ ] **Step 2: Run production type-check and build**

Run: `pnpm -C app build`

Expected: TypeScript, Vite build, and bundle-size gate PASS.

- [ ] **Step 3: Inspect the final diff**

Run: `git diff HEAD~2 --check` and `git status --short`.

Expected: no whitespace errors; only the known pre-existing `app/src-tauri/Cargo.toml` line-ending noise may remain unstaged.

- [ ] **Step 4: Perform manual desktop acceptance**

Run: `pnpm -C app tauri dev`.

Verify that DeepSeek, GitHub, and GitLab switches do not resize the shell; configured-state actions do not move the footer; short windows constrain the dialog; narrow windows wrap actions; and only provider content scrolls.
