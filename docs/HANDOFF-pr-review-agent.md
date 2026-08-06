# PR Review Agent Handoff

## Implemented scope

The `feat/pr-review-agent` branch adds a public GitHub PR review workflow:

- DeepSeek V4 through its OpenAI-compatible Chat Completions API.
- Review-time model selection between allowlisted DeepSeek V4 Flash and V4 Pro models,
  plus Simplified Chinese or English output selection.
- A Rust-owned credential flow for DeepSeek, GitHub, and GitLab keys.
- Review input pinned to the pull request head SHA.
- Read-only repository-tree and file-reading tools with enforced round, call, file,
  patch, line, and byte budgets.
- Versioned code-disclosure consent and explicit file selection for oversized pull requests.
- Select-all file controls with an indeterminate state and a clearly bounded first-request
  input-token estimate; tool reads and output usage remain actual-usage-only.
- Review results and edited comment drafts are cached locally per pull request and head SHA,
  restored after reopening, invalidated when the PR changes, and cleared after publication.
- No-findings results use a compact completion state; verbose model summaries remain available
  behind an explicit disclosure instead of dominating the result view.
- Progress events, cancellation, editable findings, and a valid no-findings result.
- One GitHub `COMMENT` review containing the selected line comments.
- A second head-SHA check before publishing; stale results return `PR_UPDATED` and publish nothing.
- Sanitized rolling traces that exclude credentials, prompts, patches, source content,
  complete model output, and reasoning.

The review runtime is isolated in `crates/review-agent`. Tauri commands live in
`app/src-tauri/src/review_commands.rs`, public DTOs in `crates/ipc-types`, and the
frontend workspace in `app/src/components/PrReviewWorkspace.tsx`.

## Provider boundary

The agent loop is provider-neutral. `ModelProvider` returns canonical final-text or
tool-call turns and exposes a descriptor with model capabilities. Provider adapters own
HTTP/message-format details only. Review JSON decoding and its fallback to plain-text
summaries live in the shared `ReviewOutputCodec`. Tool execution, unique-read caching,
budgets, patch-line validation, and traces remain in the orchestrator.

The current production adapter is `DeepSeekProvider`. Future OpenAI, Anthropic, or local
adapters should implement `ModelProvider` rather than adding provider branches to the
orchestrator.

## Security boundary

The agent cannot execute commands, inspect the local checkout, modify files, or publish
without a separate user action. Repository reads are restricted to the recorded PR head
SHA. PR text and source code are treated as untrusted model input and cannot add tools or
increase budgets. Credentials stay in the Rust backend and are never returned to the WebView.

## Stable errors

The public workflow uses these codes: `AI_KEY_MISSING`, `GITHUB_TOKEN_MISSING`,
`AUTH_FAILED`, `RATE_LIMITED`, `NETWORK_ERROR`, `PR_UPDATED`,
`REVIEW_BUDGET_EXCEEDED`, `INVALID_MODEL_OUTPUT`, `CANCELLED`, and
`REVIEW_PUBLISH_FAILED`.

## Follow-up milestones

The actionable next-stage plan is maintained in `AGENT-ROADMAP.md`. Its order is:

1. Stabilize the provider contract and expose a backend-owned model catalog.
2. Add a second provider adapter to prove that the runtime is genuinely provider-neutral.
3. Add GitHub Issue triage as a read-only workflow, followed by a separately approved
   label/comment publication slice.
4. Keep GitLab merge-request review as the next collaboration expansion after GitHub
   Issue triage and GitLab-specific diff-position rules are defined.
5. Design local development agents as a separate security milestone. Local commands and
   file writes must not be added to the PR review runtime's security boundary.

## Verification

Run the standard repository checks documented in `README.md`. Tests use fake providers
and HTTP/GitHub fixtures; they do not require real API keys or external network access.
