# Stable Settings Dialog Layout Design

## Goal

Keep the Settings dialog visually stable when users switch between DeepSeek, GitHub,
and GitLab, when credential status finishes loading, and when a credential changes from
unconfigured to configured. Only the provider content region may scroll; the dialog shell,
navigation, provider tabs, and action placement remain stable.

## Problem

The current shell specifies only a maximum height. Its actual height is therefore driven
by the active provider's content and available actions. DeepSeek includes service details
that the GitHub and GitLab panels do not, while configured providers expose extra actions.
Switching provider or credential state consequently resizes the whole dialog and moves the
primary action.

## Layout

On desktop, the dialog uses an explicit responsive size:

- Width: `min(960px, calc(100vw - 48px))`.
- Height: `min(680px, calc(100vh - 48px))`.
- The existing title bar remains a fixed-height top row.
- The left settings navigation fills the remaining shell height.
- The right pane is a three-row layout: integration header and provider tabs, scrollable
  provider content, and a fixed action bar.

The provider content is the only vertically scrolling region. The dialog shell and the
right pane use `min-height: 0` at every flex/grid boundary so overflow is contained rather
than expanding an ancestor.

## Action Bar

The action bar is outside the provider tab panel's scrolling content and always reserves
the same height.

- The primary save action stays at the far right.
- For configured credentials, Test Connection sits immediately before Save, while Remove
  Credential stays at the far left.
- For unconfigured credentials, the unused left and secondary action spaces remain empty;
  the bar itself does not collapse.
- Loading and active-operation states replace button content without changing button size.
- The action bar has a top divider and an opaque canvas background. It does not use sticky
  positioning because it is already a dedicated grid row.

## Provider Switching

DeepSeek, GitHub, and GitLab share the same panel frame. Switching providers changes only
the scrollable content and action availability. The content scroll position resets to the
top for the newly selected provider.

No height, width, padding, or layout-property animation is used. A subtle content opacity
transition of 150 milliseconds is permitted, with an instant transition under
`prefers-reduced-motion: reduce`.

## Responsive Behavior

At the existing small-screen breakpoint, settings category navigation becomes a horizontal
row above the right pane. The dialog continues to use the available viewport height minus
24px of total outer margin. Provider tabs remain horizontally scrollable, provider content
remains vertically scrollable, and the action bar wraps actions into full-width rows when
needed without changing the dialog's outer height.

If the viewport is too short to present the normal desktop header spacing, internal header
padding reduces at the existing responsive breakpoint. The dialog must never extend beyond
the viewport or require the backdrop itself to scroll.

## Accessibility

The existing dialog semantics, focus trap, Escape behavior, provider tab keyboard controls,
and focus restoration remain unchanged. Moving actions outside the tab panel must not break
their accessible relationship to the active provider. Disabled and loading states retain
visible labels and stable focus behavior.

## Testing

Component tests will verify:

- The shell has an explicit responsive height rather than only a maximum height.
- Provider content owns vertical scrolling while the shell and action bar do not.
- The action bar is rendered for every provider and credential state.
- Configured and unconfigured states expose the correct actions without replacing the bar.
- Provider switching resets the content scroll position.
- Existing keyboard, focus-restoration, busy-state, and localization tests continue to pass.

Manual acceptance will cover desktop and narrow viewports, all three providers, loading and
configured states, long translated copy, and a short-height window. Switching among these
states must not move or resize the dialog shell.

## Out of Scope

This change does not add settings categories, change credential behavior, redesign the
provider information architecture, or add new service configuration fields.
