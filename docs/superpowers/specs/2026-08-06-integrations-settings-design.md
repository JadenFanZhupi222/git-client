# Integrations Settings Information Architecture

## Goal

Correct the Settings hierarchy so credential providers are not represented as root-level
settings categories. Settings has one root category, **Integrations**, and the three
credential providers are focused views within that category.

This change is intentionally small. It does not add a settings dashboard, provider
catalog, marketplace, search, metrics, or new credential capabilities.

## Information Architecture

The Settings sidebar contains exactly one root item:

- `Integrations` / `集成`

The Integrations content pane contains a horizontal provider tablist:

- DeepSeek
- GitHub
- GitLab

Opening Settings from a generic entry point selects Integrations and DeepSeek. Existing
provider-specific entry points still open Integrations with the requested provider tab
selected. Provider identity remains the input to credential IPC; category navigation and
credential identity are separate UI concepts.

## Layout

Keep the existing modal, backdrop, header, token system, and restrained developer-tool
visual language. The desktop modal is approximately 780 pixels wide and uses a stable,
responsive height: 620 pixels when space permits, capped at the viewport height minus
48 pixels. Switching providers must never resize the outer dialog.

The dialog header remains fixed. The category rail and provider content occupy the remaining
height. The provider content area is the only vertical scroll owner, so longer DeepSeek
details scroll without moving the header, category navigation, provider tabs, or dialog
boundary. The action row remains part of the provider content rather than becoming a sticky
footer.

The desktop layout has a 150-pixel category rail and a content pane:

1. The category rail uses a normal navigation landmark. Integrations is marked with
   `aria-current="page"`; a one-item tablist is not used.
2. The content pane begins with the Integrations heading and a short description.
3. A horizontal provider tablist follows the heading. The active provider uses a compact
   accent underline, while inactive providers use muted text and a restrained hover state.
4. Provider details are rendered below the tabs as a flat form. Nested provider cards are
   avoided.

Below 640 pixels, the category rail becomes a compact row below the dialog header. Provider
tabs remain visible and may scroll horizontally. Content padding reduces to 16 pixels.
Provider headings and status may wrap. At narrow phone widths, actions stack into full-width
buttons while preserving logical focus order. On short viewports, the same 48-pixel outer
inset applies; the dialog never exceeds the available viewport height.

## Provider Detail

Every provider view contains:

- Provider name and one-line purpose.
- A factual credential-presence status: configured, not configured, checking, or status
  unavailable.
- A password input that never displays the stored credential.
- A short note stating that the credential is stored in the system credential store.
- Actions appropriate to the current state.

DeepSeek additionally contains a compact service-details definition list with the fixed
endpoint and model, followed by the existing disclosure about sending selected PR patches
and code excerpts to DeepSeek. GitHub and GitLab do not render empty service-detail shells.

Credential labels use sentence case without uppercase tracking. When configured, the input
placeholder explicitly explains that entering a value replaces the stored credential.

## Actions

When a credential is not configured, show only a right-aligned primary **Save credential**
action. Do not show disabled Test or Remove actions that cannot succeed.

When a credential is configured:

- Show **Remove credential** on the left.
- Show **Test connection** and **Save replacement** on the right.
- Disable Save replacement until the input contains a non-whitespace value.

The active operation retains an inline spinner. All conflicting actions, navigation, and
dialog closing remain disabled while an operation is running. A status lookup failure must
not prevent entering and saving a credential.

## Copy

New copy is added to both locale dictionaries and is never hard-coded in components.

| English | Chinese |
| --- | --- |
| Integrations | 集成 |
| Manage credentials for AI review and code hosting. | 管理 AI 评审与代码托管服务的凭据。 |
| Integration | 集成服务 |
| Service details | 服务详情 |
| Save credential | 保存凭据 |
| Save replacement | 保存新凭据 |
| Test connection | 测试连接 |
| Remove credential | 移除凭据 |
| Stored securely in the system credential store. | 安全存储于系统凭据库中。 |

Provider descriptions remain concise and task-specific. Status copy must say
**Configured**, not **Connected**, because the status command confirms presence rather than
network validity.

## Accessibility and Interaction

- Preserve the dialog focus trap, Escape/backdrop behavior, busy-state lockout, and focus
  restoration.
- The root category uses navigation semantics; providers use tab/tablist/tabpanel semantics.
- Provider tabs support Arrow Left/Right, Home, and End.
- Status remains a polite live region, and the provider detail exposes busy state.
- Associate the credential field with its helper and disclosure text using
  `aria-describedby`.
- Preserve the visible two-pixel accent focus indicator and WCAG AA contrast.
- After credential statuses load, focus the active provider input rather than the category
  navigation item.

## Testing

Update Settings tests to verify:

- One Integrations root category and a separate provider tablist.
- Provider-specific deep links select the expected tab.
- Provider tab keyboard navigation and panel associations.
- Configured and unconfigured action visibility.
- Replacement placeholder and stored-secret non-rendering.
- Partial status failures still allow credential saving.
- Busy lockout, late asynchronous results, unmount safety, focus containment, and focus
  restoration remain intact.
- English and Chinese labels render through locale dictionaries.

## Non-goals

- Settings overview or landing page.
- Integration catalog, provider cards, logos, metrics, search, or add-integration flow.
- New credential storage or IPC behavior.
- Changes to the PR review runtime or publishing flow.
