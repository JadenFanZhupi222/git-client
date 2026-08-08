# PR Review Agent Handoff

## Implemented scope

The review-agent slice provides public GitHub PR and GitLab MR review workflows:

- DeepSeek through its OpenAI-compatible Chat Completions API, OpenAI through
  the Responses API, and Anthropic through the Messages API.
- Review-time selection across seven backend-allowlisted models, plus Simplified
  Chinese or English output selection.
- A Rust-owned credential flow for DeepSeek, OpenAI, Anthropic, GitHub, and GitLab keys.
- Review input pinned to the pull request or merge request head SHA.
- Read-only repository-tree and file-reading tools with enforced round, call, file,
  patch, line, and byte budgets.
- Versioned code-disclosure consent and explicit file selection for oversized pull requests.
- Select-all file controls with an indeterminate state and a clearly bounded first-request
  input-token estimate; tool reads and output usage remain actual-usage-only.
- Review results and edited comment drafts are cached locally per hosting provider,
  change request, and head SHA; they are restored after reopening, invalidated when the
  change updates, and cleared after publication.
- No-findings results use a compact completion state; verbose model summaries remain available
  behind an explicit disclosure instead of dominating the result view.
- Progress events, cancellation, editable findings, and a valid no-findings result.
- One GitHub `COMMENT` review containing the selected line comments, or selected GitLab
  diff discussions positioned with the current base/start/head SHA triplet.
- A second head-SHA check before publishing; stale results return `PR_UPDATED` and publish nothing.
- Sanitized rolling traces that exclude credentials, prompts, patches, source content,
  complete model output, and reasoning.

The review runtime is isolated in `crates/review-agent`. Tauri commands live in
`app/src-tauri/src/review_commands.rs`, public DTOs in `crates/ipc-types`, and the
frontend workspace in `app/src/components/PrReviewWorkspace.tsx`.

## Provider boundary

The provider contract now lives in the shared `agent-runtime` crate. `ModelProvider`
receives a normalized request and returns canonical final-text or tool-call turns plus
usage. Its descriptor includes stable provider/model IDs, structured-output and tool
capabilities, context/output limits, and usage support. Provider adapters own
HTTP/message-format details only. Review JSON decoding and its fallback to plain-text
summaries live in `ReviewOutputCodec`; Issue Triage uses its own domain codec over the
same provider response. Tool execution, unique-read caching, budgets, patch-line
validation, and traces remain in the PR orchestrator.

The production adapters are `DeepSeekProvider`, `OpenAiProvider`, and
`AnthropicProvider`, all shared by PR Review and Issue Triage. The aggregate catalog
and factory live in `providers.rs`; neither workflow contains provider-specific
branches. Future local or hosted adapters should implement `ModelProvider` and join
that registry instead of branching either orchestrator.

## Security boundary

The agent cannot execute commands, inspect the local checkout, modify files, or publish
without a separate user action. Repository reads are restricted to the recorded PR head
SHA. PR text and source code are treated as untrusted model input and cannot add tools or
increase budgets. Credentials stay in the Rust backend and are never returned to the WebView.

## Stable errors

The public workflow uses these codes: `AI_KEY_MISSING`, `OPENAI_KEY_MISSING`,
`ANTHROPIC_KEY_MISSING`, `GITHUB_TOKEN_MISSING`, `GITLAB_TOKEN_MISSING`,
`AUTH_FAILED`, `RATE_LIMITED`,
`NETWORK_ERROR`, `PR_UPDATED`,
`REVIEW_BUDGET_EXCEEDED`, `INVALID_MODEL_OUTPUT`, `CANCELLED`, and
`REVIEW_PUBLISH_FAILED`. `AGENT_RESOURCE_BUSY` is returned when another run is
already active for the same PR, even if the client generated a different run ID.

Model requests retry only provider network and rate-limit failures, with three total
attempts and bounded jittered exponential backoff. Authentication, truncated output,
and invalid responses are not retried. Successful results expose actual usage, elapsed
time, provider attempt count, and the same sanitized diagnostic ID stored in the trace.
Agent failures and cancellations return that diagnostic ID through an Agent-specific
IPC error contract; cancellation is shown as a terminal state rather than silently
returning to file selection.

## Milestone status and follow-up

The provider contract, multi-provider proof, GitHub Issue Triage workflow, confirmed
Issue publication, production hardening, and GitLab merge-request review expansion are
complete. The remaining larger direction is:

1. Design local development agents as a separate security milestone. Local commands and
   file writes must not be added to the PR review runtime's security boundary.

## Verification

Run the standard repository checks documented in `README.md`. Tests use fake providers
and HTTP GitHub/GitLab fixtures; they do not require real API keys or external network access.
