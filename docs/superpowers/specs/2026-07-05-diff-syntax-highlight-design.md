# Diff Syntax Highlight Design

## Goal
Add lightweight syntax highlighting to text diffs and the three-pane conflict editor so code changes no longer render as plain monochrome text.

## Scope
- Highlight textual `DiffView` content in both unified and split modes.
- Highlight `ConflictEditor` CodeMirror panes for ours/result/theirs.
- Infer language from the file path extension.
- Keep binary, image, LFS, and too-large branches unchanged.
- Do not change Rust DTOs or GitBackend behavior.

## Approach
Use a small frontend language registry in `app/src/lib/syntax.ts`. It maps common file extensions to a stable language id and provides two APIs:

- `languageIdForPath(path)` for path-based detection.
- `highlightCodeLine(text, lang)` for tokenizing a single visible diff line into semantic spans.

`DiffView` already virtualizes rows and renders lines through `LineContent`. The highlighter will run only for visible rows, preserving virtualization. Word-level diff emphasis remains higher priority: changed segments keep their existing add/delete background, and syntax color is applied inside each segment.

`ConflictEditor` already uses CodeMirror. It will add a language extension derived from the file path when available, plus a theme for token colors using existing CSS tokens.

## Language Coverage
Initial coverage targets high-frequency repository files:

- TypeScript / JavaScript / TSX / JSX
- Rust
- JSON
- Markdown
- CSS / SCSS / LESS
- HTML
- Python
- YAML
- TOML
- Shell

Unknown extensions fall back to plain text.

## Styling
Colors must use existing theme CSS variables and Tailwind theme tokens. No hard-coded hex colors.

Token classes:

- `syn-keyword`
- `syn-string`
- `syn-number`
- `syn-comment`
- `syn-type`
- `syn-function`
- `syn-property`
- `syn-operator`

The palette should be restrained enough not to fight add/delete backgrounds.

## Testing
Add focused Vitest coverage for:

- Extension detection, including compound filenames such as `Dockerfile` and `.github/workflows/ci.yml`.
- Tokenization for representative TypeScript, Rust, JSON, CSS, Markdown, and shell lines.
- HTML escaping safety by rendering token text as React text, not HTML.

Manual verification after automated checks:

- Unified diff keeps row backgrounds, word-level emphasis, and line staging behavior.
- Split diff highlights both sides without horizontal/layout regressions.
- Conflict editor panes show CodeMirror syntax colors and preserve merge spacers/actions.
