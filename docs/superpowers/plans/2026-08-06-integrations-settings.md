# Integrations Settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace provider-level Settings root navigation with one Integrations category and a focused provider tab interface.

**Architecture:** Keep credential IPC and provider deep-link inputs unchanged. Separate the single category navigation from the existing `CredentialKindDto` provider selection inside `SettingsPanel`, then vary the credential actions by configured state. All new copy remains in the English and Chinese locale dictionaries.

**Tech Stack:** React 19, TypeScript, Vitest, Testing Library, Tailwind CSS v4 tokens, Tauri IPC wrappers

---

## File Map

- Modify `app/src/components/SettingsPanel.tsx`: render root category navigation, provider tabs, flat provider details, state-specific actions, and responsive layout.
- Modify `app/src/components/SettingsPanel.test.tsx`: specify the new information architecture, keyboard behavior, action visibility, copy, and existing lifecycle guarantees.
- Modify `app/src/lib/locales/en.ts`: add English Integrations hierarchy and credential-action copy.
- Modify `app/src/lib/locales/zh.ts`: add matching Chinese copy.
- Verify `app/src/App.settings.test.tsx`: preserve every provider-specific entry point and Settings lifecycle contract.

### Task 1: Specify the Integrations hierarchy and provider navigation

**Files:**
- Modify: `app/src/components/SettingsPanel.test.tsx`
- Modify: `app/src/components/SettingsPanel.tsx`

- [ ] **Step 1: Write failing information-architecture tests**

Add tests that render `initialSection="github"` and assert one category navigation item plus a separate provider tablist:

```tsx
renderPanel({ initialSection: "github" });

const categoryNav = screen.getByRole("navigation", { name: "Settings categories" });
expect(within(categoryNav).getByText("Integrations")).toHaveAttribute(
  "aria-current",
  "page",
);
expect(within(categoryNav).queryByText("GitHub")).not.toBeInTheDocument();

const providers = screen.getByRole("tablist", { name: "Integration" });
expect(within(providers).getByRole("tab", { name: "GitHub" })).toHaveAttribute(
  "aria-selected",
  "true",
);
```

Add a keyboard test that focuses GitHub and verifies ArrowRight selects GitLab, Home selects DeepSeek, and End selects GitLab. Preserve the existing assertion that `initialSection="gitlab"` deep-links directly to GitLab.

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```powershell
pnpm --dir app test -- SettingsPanel.test.tsx
```

Expected: FAIL because providers are still root sidebar tabs and no `Settings categories` navigation exists.

- [ ] **Step 3: Separate category and provider navigation**

In `SettingsPanel.tsx`, rename the provider collection and keep provider selection typed independently:

```tsx
const PROVIDERS: CredentialKindDto[] = ["deepseek", "github", "gitlab"];
type CredentialStatuses = Partial<Record<CredentialKindDto, boolean>>;

const [provider, setProvider] = useState<CredentialKindDto>(initialSection);
const providerRef = useRef<CredentialKindDto>(initialSection);
```

Render a normal category nav containing one selected Integrations button:

```tsx
<nav aria-label={t("settings.categories")} className="border-r border-line bg-elevated p-2">
  <button
    type="button"
    aria-current="page"
    className="w-full rounded-md bg-accent/15 px-3 py-2 text-left text-xs font-medium text-accent"
  >
    {t("settings.integrations.title")}
  </button>
</nav>
```

Move provider buttons into a horizontal `role="tablist"` in the content pane. Retain `tab`, `aria-controls`, `aria-selected`, roving `tabIndex`, and ArrowLeft/Right/Home/End behavior. Rename `selectSection` and `onTabKeyDown` to provider-oriented names so their responsibility is explicit.

- [ ] **Step 4: Run the focused test and verify GREEN**

Run:

```powershell
pnpm --dir app test -- SettingsPanel.test.tsx
```

Expected: all SettingsPanel tests pass.

- [ ] **Step 5: Commit the hierarchy slice**

```powershell
git add app/src/components/SettingsPanel.tsx app/src/components/SettingsPanel.test.tsx
git commit -m "refactor: add integrations settings hierarchy"
```

### Task 2: Implement provider detail and state-specific actions

**Files:**
- Modify: `app/src/components/SettingsPanel.test.tsx`
- Modify: `app/src/components/SettingsPanel.tsx`
- Modify: `app/src/lib/locales/en.ts`
- Modify: `app/src/lib/locales/zh.ts`

- [ ] **Step 1: Write failing configured-state tests**

For an unconfigured GitHub credential, assert only Save credential is visible:

```tsx
mockCredentialStatus.mockResolvedValue(false);
renderPanel({ initialSection: "github" });
await screen.findByText("Not configured");

expect(screen.getByRole("button", { name: "Save credential" })).toBeDisabled();
expect(screen.queryByRole("button", { name: "Test connection" })).not.toBeInTheDocument();
expect(screen.queryByRole("button", { name: "Remove credential" })).not.toBeInTheDocument();
```

For a configured GitHub credential, assert Test connection and Remove credential are visible,
Save replacement is disabled until a new value is entered, and the replacement placeholder
does not reveal the stored value. Add matching Chinese-label assertions using the existing
locale provider test harness.

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```powershell
pnpm --dir app test -- SettingsPanel.test.tsx
```

Expected: FAIL because all three generic actions currently remain rendered.

- [ ] **Step 3: Add bilingual hierarchy and action copy**

Add these keys to both locale dictionaries:

```ts
"settings.categories": "Settings categories",
"settings.integrations.title": "Integrations",
"settings.integrations.description": "Manage credentials for AI review and code hosting.",
"settings.integrations.providers": "Integration",
"settings.serviceDetails": "Service details",
"settings.credentialStored": "Stored securely in the system credential store.",
"settings.action.saveCredential": "Save credential",
"settings.action.saveReplacement": "Save replacement",
"settings.action.testConnection": "Test connection",
"settings.action.removeCredential": "Remove credential",
"settings.credentialReplacementPlaceholder": "Enter a new credential to replace the saved credential",
```

Use the Chinese values defined in the approved design specification. Update provider
descriptions and the DeepSeek disclosure to the approved concise wording.

- [ ] **Step 4: Render flat details and conditional actions**

Add the Integrations heading and description above the provider tablist. Render DeepSeek
endpoint/model under a localized Service details heading without a nested card. Replace the
uppercase credential label with a sentence-case 12-pixel label and add the credential-store
helper text.

Render actions from credential state:

```tsx
{configured ? (
  <>
    <Button variant="danger" onClick={() => void runOperation("clear")} disabled={busy}>
      {activeOperation === "clear" && <SpinnerIcon width={13} height={13} />}
      {t("settings.action.removeCredential")}
    </Button>
    <div className="ml-auto flex gap-2 max-[440px]:ml-0 max-[440px]:flex-col">
      <Button onClick={() => void runOperation("test")} disabled={busy}>
        {t("settings.action.testConnection")}
      </Button>
      <Button variant="primary" onClick={() => void runOperation("save")} disabled={busy || !secret.trim()}>
        {t("settings.action.saveReplacement")}
      </Button>
    </div>
  </>
) : (
  <Button
    variant="primary"
    className="ml-auto"
    onClick={() => void runOperation("save")}
    disabled={busy || !secret.trim()}
  >
    {t("settings.action.saveCredential")}
  </Button>
)}
```

Keep save/test/clear IPC calls, generation guards, toasts, secret clearing, status updates,
busy lockout, and status-failure save behavior unchanged.

- [ ] **Step 5: Run focused tests and verify GREEN**

Run:

```powershell
pnpm --dir app test -- SettingsPanel.test.tsx
```

Expected: all SettingsPanel tests pass, including existing lifecycle and secret-safety tests.

- [ ] **Step 6: Commit the provider-detail slice**

```powershell
git add app/src/components/SettingsPanel.tsx app/src/components/SettingsPanel.test.tsx app/src/lib/locales/en.ts app/src/lib/locales/zh.ts
git commit -m "feat: refine integrations credential settings"
```

### Task 3: Verify responsive, entry-point, and accessibility behavior

**Files:**
- Modify if a regression requires it: `app/src/components/SettingsPanel.tsx`
- Modify if a missing assertion requires it: `app/src/components/SettingsPanel.test.tsx`
- Verify: `app/src/App.settings.test.tsx`

- [ ] **Step 1: Add the final accessibility assertions**

Assert that the active provider panel has `aria-busy`, the credential input references the
helper text with `aria-describedby`, and focus returns to the invoking control after close.
Preserve tests for Tab containment, Escape/backdrop lockout while busy, and unmount safety.

- [ ] **Step 2: Run all Settings integration tests**

Run:

```powershell
pnpm --dir app test -- SettingsPanel.test.tsx App.settings.test.tsx
```

Expected: both files pass; all seven Settings entry paths still select the correct provider.

- [ ] **Step 3: Run complete frontend verification**

Run:

```powershell
pnpm --dir app test
pnpm --dir app exec tsc -p tsconfig.json --noEmit
pnpm --dir app build
```

Expected: every frontend test passes, TypeScript exits 0, and the initial JavaScript bundle
remains within the 500,000-byte budget.

- [ ] **Step 4: Review the rendered class structure against responsive requirements**

Confirm the modal uses approximately `w-[780px]`, the desktop body uses a 150-pixel category
rail, the rail moves above content below 640 pixels, provider tabs remain usable at narrow
widths, and actions stack below 440 pixels. Confirm every color uses existing semantic tokens.

- [ ] **Step 5: Commit any final test or responsive corrections**

If Step 1 or Step 4 changed files:

```powershell
git add app/src/components/SettingsPanel.tsx app/src/components/SettingsPanel.test.tsx
git commit -m "test: verify integrations settings behavior"
```

If no files changed, do not create an empty commit.
