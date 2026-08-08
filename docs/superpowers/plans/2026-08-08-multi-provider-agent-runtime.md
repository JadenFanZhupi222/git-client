# Multi-Provider Agent Runtime Implementation Plan

**Date:** 2026-08-08
**Status:** Implemented and automatically verified
**Scope:** GitHub PR Review and GitHub Issue Triage

## Outcome

The desktop app now has a provider-neutral vertical AI runtime in addition to its
two horizontal agent workflows. DeepSeek, OpenAI Responses, and Anthropic Messages
are selected through one backend-owned model catalog and one provider factory. PR
Review and Issue Triage receive only the normalized `ModelProvider` contract and do
not branch on a provider name.

The installed catalog contains seven allowlisted models:

- DeepSeek V4 Flash and V4 Pro
- OpenAI GPT-5.6 Sol, Terra, and Luna
- Anthropic Claude Opus 5 and Sonnet 5

Catalog metadata is descriptive rather than authoritative for billing. Pricing and
limits must be rechecked against provider documentation when models are changed.

## Architecture

```text
PR Review workflow ----+
                       +--> agent-runtime --> ModelProvider
Issue Triage workflow -+                         |
                                                 +--> DeepSeek adapter
                                                 +--> OpenAI Responses adapter
                                                 +--> Anthropic Messages adapter
```

`agent-runtime` owns normalized requests, transcript turns, tools, structured-output
schemas, responses, usage, errors, capabilities, cancellation, and model metadata.
Each adapter owns only its HTTP authentication and wire-format mapping. Domain output
decoding, budgets, snapshot validation, publication rules, and traces stay in their
workflow crates.

## Implementation decisions

### Structured output

`ModelRequest` carries an optional JSON Schema. PR Review and Issue Triage each expose
their own schema, so an adapter can request native structured output without learning
domain rules. Plain-text fallback behavior remains inside the domain codec.

### OpenAI Responses

The adapter maps normalized tools to Responses function tools and maps later tool
results to `function_call_output` items. It requests JSON Schema output through
`text.format`, disables response storage, and does not replay provider-specific
reasoning items into later normalized turns.

### Anthropic Messages

The adapter maps normalized calls and results to `tool_use` and `tool_result` content
blocks. It requests JSON Schema output through `output_config.format` and does not
replay provider-specific thinking blocks. The required API version header is set by
the backend.

### Credentials and selection

DeepSeek, OpenAI, and Anthropic use separate keyring entries. The WebView receives
credential status only, never a secret. The selected model deterministically selects
its credential and adapter; unknown model IDs fail before any provider request. Missing
keys use provider-specific error codes so both workflows can open the correct settings
tab.

### Safety invariants

- Model output is a proposal and never grants a permission.
- PR reads remain pinned to the selected head SHA.
- Issue analysis and publication remain pinned to an issue snapshot.
- Provider switching does not change tool budgets or publication validation.
- Only transient network and rate-limit failures are retried.
- Truncated, refused, malformed, or duplicate-call responses fail closed.
- Traces exclude credentials, prompts, source content, complete model output, and
  reasoning/thinking content.
- Automated tests use HTTP fixtures and consume no real model credentials or tokens.

## Main implementation locations

- `crates/agent-runtime/src/lib.rs`: normalized request and response contract.
- `crates/review-agent/src/deepseek.rs`: DeepSeek adapter.
- `crates/review-agent/src/openai.rs`: OpenAI Responses adapter.
- `crates/review-agent/src/anthropic.rs`: Anthropic Messages adapter.
- `crates/review-agent/src/providers.rs`: aggregate catalog, provider lookup, and factory.
- `crates/review-agent/src/review_output.rs`: PR Review JSON Schema and codec.
- `crates/review-agent/src/issue.rs`: Issue Triage JSON Schema and domain workflow.
- `app/src-tauri/src/credentials.rs`: keyring storage and credential validation.
- `app/src-tauri/src/review_commands.rs`: IPC catalog and production provider construction.
- `crates/ipc-types/src/lib.rs`: credential DTO contract.
- `app/src/components/SettingsPanel.tsx`: provider credential settings.

## Verification evidence

The implementation passed the following checks on 2026-08-08:

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-dependency-boundaries.ps1
pnpm.cmd -C app test
pnpm.cmd -C app build
node --test scripts/check-bundle-size.test.mjs scripts/release-preflight.test.mjs
pnpm.cmd -C app release:check -- --allow-unsigned
```

Rust workspace tests, 241 frontend tests, 18 release-script tests, dependency
boundaries, the unsigned artifact preflight, and the 500,000-byte initial JavaScript
budget all passed. The final initial entry chunk was approximately 432 KB.

## Remaining acceptance and future slices

OpenAI and Anthropic still need credential-backed desktop smoke tests by an operator;
that work intentionally remains outside automated verification. A failed smoke test
should produce the sanitized diagnostic ID needed for adapter-level investigation.

GitLab merge-request review is the next natural collaboration workflow. A local
code-editing agent is a separate security milestone because it requires workspace
isolation, command permissions, change previews, and recovery controls that must not be
added to the current read-only model runtime implicitly.
