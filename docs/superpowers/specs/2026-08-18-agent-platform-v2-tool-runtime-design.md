# Agent Platform v2: Generic Tool Runtime and Permission Contract

**Status:** Accepted for implementation
**Date:** 2026-08-18
**Stage:** S3 of the Agent Platform v2 roadmap

## 1. Decision

VersionArc will execute model-requested tools through one provider-neutral runtime boundary. Provider adapters remain responsible only for translating complete provider responses into canonical `ToolCall` values. A tool call is not executable until the provider response has reached its valid terminal event, the complete arguments have parsed as JSON, the registered input schema has validated them, run limits allow another call, and the permission decision is `allow` (directly or after an explicit approval).

The authoritative workflow result remains the validated structured result returned by the workflow. Tool and stream events are observational and cannot substitute for either a canonical model response or a tool result.

## 2. Current baseline and gaps

The current runtime already provides provider-neutral model requests/responses, `ToolDefinition`, `ToolCall`, real streaming for OpenAI/Anthropic/DeepSeek, cancellation around provider calls, bounded workflow loops, usage accounting, and monotonic `AgentEvent` delivery. Provider adapters already reconstruct complete tool arguments and reject malformed/truncated responses; S3 does not duplicate that parsing.

The PR review workflow currently owns two read-only tools directly. It validates arguments with workflow-specific functions, stores the provider call ID inside a private `_call_id` argument, executes by matching tool names, and applies workflow-local call/output/round budgets. Other workflows use their own orchestration. There is no shared registration, schema compiler, permission evaluator, approval channel, timeout policy, output sanitizer, or tool-execution event contract.

S3 closes the shared-contract gap. S4 supplies production shell, patch, filesystem, search, web, and artifact implementations; S7 supplies MCP adapters.

## 3. Goals

S3 must:

1. Make tool IDs first-class and define provider-neutral `ToolDefinition`, `ToolCall`, and `ToolResult` contracts.
2. Provide a duplicate-safe `ToolRegistry` and asynchronous `ToolHandler` execution port.
3. Compile and validate a controlled JSON Schema subset before policy evaluation or handler invocation.
4. Model risk independently from provider-visible tool metadata.
5. Support deterministic `allow`, `deny`, and `ask` decisions, with `deny` as the failure default.
6. Enforce run-scoped cancellation, per-call timeout, call/output budgets, and maximum model-loop count.
7. Bound and sanitize tool results before they enter transcripts, events, traces, or UI-visible state.
8. Emit metadata-only lifecycle events for validation, approval, execution, completion, denial, timeout, cancellation, and failure.
9. Define a Tauri/React approval protocol that cannot execute a call and never transports secrets or raw tool output.
10. Leave stable extension ports for S4 tools and S7 MCP tools.

## 4. Non-goals

S3 does not add production shell, patch, filesystem, search, web, artifact, or MCP tools; persist approval grants; resume approvals after restart; execute tools in parallel; expose hidden reasoning; or make stream fragments executable. It does not replace existing provider tool-call parsers.

## 5. Canonical contracts

```text
ToolDefinition
├── name: stable namespaced identifier
├── description: provider-visible description
├── input_schema: controlled JSON Schema
├── risk: read_only | write | destructive | external
├── timeout_ms: per-tool ceiling (further capped by run policy)
└── max_result_bytes: per-tool sanitized-result ceiling

ToolCall
├── call_id: non-empty provider-issued ID
├── name: registered tool name
└── arguments: complete parsed JSON value

ToolResult
├── call_id / name
├── outcome: success | denied | invalid_input | timeout | cancelled | failed
├── content: sanitized bounded text intended for the model transcript
├── truncated: bool
└── content_bytes: bytes after sanitization and truncation
```

`call_id` is metadata, never an input-schema property. Adapters translate provider IDs into this field and translate it back when replaying assistant tool calls. Compatibility constructors may remain, but `_call_id` is removed from executable arguments.

Tool names use a conservative identifier grammar and are unique in a registry. Future built-ins should use stable namespaces such as `filesystem.read`, `search.text`, `shell.exec`, `patch.apply`, `web.fetch`, and `artifact.write`; MCP adapters use a collision-safe server namespace.

## 6. Registry and handler boundary

`ToolRegistry` owns immutable registered entries after construction. Registration:

- validates name, description, timeout, and result limits;
- compiles the input schema once;
- rejects duplicate names;
- stores an `Arc<dyn ToolHandler>` and optional argument summarizer/result sanitizer ports;
- exposes only provider-visible definitions to model adapters.

`ToolHandler::execute` receives a `ToolExecutionContext` plus already validated arguments. The context contains run/call identity and a cancellation view, not API credentials or the full model prompt. A handler returns an in-process raw result; the executor sanitizes and bounds it before constructing `ToolResult`. Handler errors are typed, mapped to stable public categories, and must not be copied verbatim into events.

Registry lookup failure and schema failure never invoke permission code or a handler.

## 7. JSON Schema validation

S3 supports an explicit, recursively compiled subset of JSON Schema 2020-12:

- `type`: object, array, string, integer, number, boolean, null;
- object `properties`, `required`, `additionalProperties` (boolean only), `minProperties`, `maxProperties`;
- array `items`, `minItems`, `maxItems`, `uniqueItems`;
- string `minLength`, `maxLength`, `pattern` using the runtime's documented safe matcher;
- numeric `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf`;
- `enum`, `const`, and `oneOf`;
- annotation-only `$schema`, `title`, and `description`.

Schemas containing unsupported validation keywords, invalid keyword types, invalid regexes, contradictory limits, external references, or remote resolution fail registration. Validation returns bounded path/code diagnostics and never echoes the rejected value. `additionalProperties` defaults to false for tool object schemas even though generic JSON Schema defaults it to true; tool authors must opt in explicitly. This is a deliberate execution-safety profile.

Only `ToolCall.arguments: Value` is accepted by the executor. There is no executor API for argument-delta strings. Providers may emit observational deltas, but only the canonical terminal `ModelResponse::ToolCalls` can reach execution.

## 8. Risk and permission policy

Risk levels are ordered for policy matching but retain distinct semantics:

| Risk | Examples | Default |
|---|---|---|
| `read_only` | bounded repository reads and local search | allow only when explicitly registered in the workflow policy |
| `write` | create/edit artifact or working-tree file | ask |
| `destructive` | delete, reset, overwrite, force operation | deny |
| `external` | network request, remote mutation, MCP side effect | ask |

`PermissionPolicy` evaluates ordered rules over exact tool name or namespace prefix plus risk. A rule returns `allow`, `deny`, or `ask`. The first matching rule wins; no match is `deny`. A run may further restrict, but never broaden, the application policy.

An `ask` decision creates an opaque run-unique approval ID and a `ToolApprovalRequest` containing only run ID, approval ID, call ID, tool name, risk, and an optional bounded sanitized summary supplied by the tool. Raw arguments are absent by default. The approval resolver returns one-shot `allow` or `deny`; cancellation, timeout, unknown/late/duplicate response, app shutdown, or dropped UI listener resolve as deny/cancel. Approval applies to exactly one `(run_id, call_id, approval_id)` and cannot become a persistent grant in S3.

## 9. Executor pipeline

For each canonical call, the executor performs these phases in order:

```text
complete ToolCall
  -> identity and registry lookup
  -> compiled schema validation
  -> run cancellation and budget reservation
  -> policy evaluation
  -> optional one-shot approval
  -> per-call timeout + cancellation race
  -> handler execution
  -> result sanitization
  -> per-tool and remaining-run size cap
  -> canonical ToolResult + metadata-only events
```

No handler receives arguments before the first five phases succeed. Budget reservation is atomic within a run so later parallel execution can be added without oversubscription. S3 executes calls serially.

`ToolRunLimits` includes maximum model rounds, maximum attempted tool calls, maximum cumulative sanitized result bytes, maximum per-call timeout, and optional run deadline. A run guard owns counters and the cancellation view. `begin_model_round` fails once before a model call would exceed the loop limit. Calls denied or invalid consume an attempt slot (to prevent adversarial free retries) but no output budget. Cached results may be represented explicitly by a future cache adapter; S3 does not silently bypass permission or accounting.

Timeout and cancellation do not imply that an uncooperative external side effect can be rolled back. Tool implementations must honor cancellation and, for S4 mutating tools, use transactional/preflight patterns where possible.

## 10. Result limits and sensitive-data filtering

Raw handler output remains process-local. Sanitization precedes byte accounting and any transcript conversion. The default sanitizer:

- replaces registered literal secrets without reading environment variables itself;
- redacts common authorization/header/token/key/value patterns;
- removes control characters except tab/newline;
- truncates on a valid UTF-8 boundary;
- emits only sanitized size/truncation metadata.

Tools handling structured secrets must provide a domain sanitizer rather than relying only on pattern matching. The effective result cap is the minimum of tool, run-remaining, and application hard ceilings. Truncation uses a fixed non-sensitive marker. Events/logs never contain result content, arguments, prompt text, API keys, provider bodies, handler error strings, or raw byte counts from before sanitization.

## 11. Observable events

The existing run-scoped monotonic `AgentEvent` stream gains metadata-only kinds:

- `tool_validation_failed(call_id, tool_name?, code)`;
- `tool_approval_requested(approval_id, call_id, tool_name, risk, summary?)`;
- `tool_approval_resolved(approval_id, call_id, decision)`;
- `tool_execution_started(call_id, tool_name, risk)`;
- `tool_execution_completed(call_id, tool_name, outcome, duration_ms, content_bytes, truncated)`.

Stable error/outcome enums are safe for UI and traces. Detailed local diagnostics may be recorded only through a separately sanitized backend trace sink. Tool events are observational; the returned `ToolResult` is authoritative for the next model transcript.

## 12. Tauri and React approval boundary

Tauri owns a `ToolApprovalRegistry` keyed by approval ID. The runtime-facing resolver registers a one-shot sender, emits the sanitized `agent-event`, then waits under the run's cancellation/deadline. React renders pending requests from the same filtered run stream and invokes:

```text
resolve_tool_approval({ run_id, approval_id, decision: allow | deny })
```

The command only resolves a registered waiter after exact run/approval matching. It cannot name a tool, alter arguments, submit result content, or execute anything itself. Unknown, expired, mismatched, or already resolved IDs return a stable IPC error. Run cancellation removes and denies all its pending approvals. Frontend state is replaceable observational state; losing it cannot accidentally allow a call.

Generated TS DTOs remain the contract source. React reducers ignore stale/duplicate/foreign sequence numbers as in S2, retain only sanitized summaries, and clear pending approval state on resolution or terminal run state.

## 13. Extension points

- **shell:** executable/argv/cwd/env policy, no implicit shell parsing, sandbox profile, output streaming adapter.
- **patch:** explicit target root, preimage hash, dry-run/preview, atomic apply and rollback metadata.
- **filesystem:** canonical root capabilities, symlink/traversal defense, read/write size quotas.
- **search:** bounded roots/results/time with no write capability.
- **web:** URL/method/domain policy, redirect and private-network controls, credential scopes.
- **artifact:** typed artifact store, MIME/size policy, opaque handles instead of arbitrary paths.
- **MCP:** server-qualified names, server trust/risk mapping, schema compilation at discovery, per-server result sanitizer and cancellation adapter.

These are adapters behind `ToolHandler`; none may bypass the registry, validator, policy, or executor.

## 14. Acceptance criteria

S3 is accepted when:

1. Canonical tool contracts round-trip through Serde and `call_id` is separate from arguments.
2. Provider adapter fixtures still reconstruct and replay tool calls without changing provider parsing semantics.
3. Registry registration rejects invalid/duplicate definitions and unsupported schemas.
4. Valid and invalid inputs across every supported schema keyword are contract-tested; invalid inputs never reach a handler.
5. Policy precedence, default deny, and all `allow`/`deny`/`ask` paths are tested.
6. Approval IDs are one-shot, run-bound, cancellable, timeout-bounded, and cannot be replayed.
7. Cancellation, per-tool timeout, run deadline, call budget, output budget, and model-round budget fail closed.
8. Raw results are redacted and UTF-8 safely truncated before transcript use; events contain metadata only.
9. Tauri DTO conversion and React reducer/IPC contracts handle approval request/resolution without raw arguments or results.
10. Existing structured workflow results remain authoritative and existing S1/S2 event ordering tests pass.
11. Workspace tests, dependency-boundary check, Clippy with warnings denied, rustfmt check, frontend tests, and production build pass.

## 15. Test matrix

| Layer | Cases |
|---|---|
| Contracts | Serde round-trip, first-class call ID, stable enum names, no secret-bearing fields |
| Schema compiler | valid nested schemas; unsupported keywords; invalid keyword types; contradictory limits; bad patterns; external refs |
| Schema validator | types, required/extra properties, arrays, bounds, enum/const/oneOf, bounded error paths, no value echo |
| Registry | valid registration, duplicate names, invalid names, immutable definitions, concurrent lookups |
| Policy | exact/prefix precedence, risk matching, default deny, run policy cannot broaden app policy |
| Approval | allow, deny, timeout, cancellation, run mismatch, unknown ID, duplicate/replayed response, shutdown |
| Executor | validation-before-policy, policy-before-handler, serial execution, handler error mapping, timeout/cancel race |
| Budgets | zero/edge limits, attempted-call accounting, cumulative post-sanitize bytes, per-tool/run timeout minimum, max rounds |
| Sanitization | literal secrets, header/token patterns, control characters, UTF-8 boundary truncation, fixed marker |
| Events | monotonic sequence, safe metadata, every terminal outcome, absence of arguments/results/prompts/provider bodies |
| Provider regression | OpenAI/Anthropic/DeepSeek complete tool-call fixtures and transcript replay |
| IPC/React | generated DTO shape, exact run/approval resolution, stale event rejection, pending-state cleanup, no execution IPC |
| Full regression | Rust workspace, Clippy, fmt, dependency boundaries, frontend Vitest, frontend production build |

## 16. Rollout

S3 first lands the contracts, registry, validator, executor, approval broker, and tests. Existing read-only review tools may be adapted incrementally without changing their provider parsing or final structured output. S4 registers production tool adapters and moves remaining workflow-local execution paths through this boundary. Until a workflow is migrated, its current restrictive tool list remains in force; no new capability is enabled merely by landing S3.
