# Agent Platform v2: Sessions, Memory, Context Compaction, and RAG

**Status:** Implemented
**Date:** 2026-08-18
**Stage:** S5 of the Agent Platform v2 roadmap

## 1. Decision

S5 adds a provider-neutral orchestration layer in a new `agent-session` crate. It composes the S1 streaming protocol, S3 execution contract, and S4 tool adapters into bounded conversational turns while keeping workflow-specific structured results authoritative.

`agent-session` owns in-memory session state, atomic turn leases, context planning, deterministic memory compaction, retrieval injection, provider retry, and the model/tool loop. It depends only on `agent-runtime`. Provider HTTP adapters remain in `review-agent`; filesystem, process, and web adapters remain in `agent-tools`.

S5 also adds `TranscriptItem::AssistantText` so every provider can replay completed assistant turns. This is transcript compatibility, not a new provider parsing path. The application composes the generic engine into a repository-scoped Tauri service and an intentional React Agent workspace; neither layer owns provider parsing or tool execution.

## 2. Scope and non-goals

S5 includes:

- in-memory sessions with one active run per session;
- atomic commit of a user/assistant turn only after a canonical final model response;
- recent conversational memory plus a bounded compacted summary;
- context-window planning using a conservative provider-neutral token estimate;
- bounded RAG over explicitly supplied, sanitized text chunks;
- a generic multi-round model/tool loop using `ToolExecutor`;
- cancellation, transient retry, tool/model/usage budgets, and stable terminal errors;
- repository-scoped Tauri get/reset/start/cancel commands and generated TypeScript contracts;
- a React workspace for model selection, explicit provider consent, streaming status, write approval, cancellation, and session reset;
- tests proving provider neutrality, compaction, retrieval, loop bounds, and final-result authority.

S5 does not include durable persistence, crash recovery, background replay, repository-wide automatic indexing, embeddings, a vector database, MCP, or multi-agent orchestration. Durable sessions and resume belong to S6.

## 3. Dependency boundary

```text
review-agent provider adapters ─┐
                               ├─ host composition
agent-tools adapters ──────────┤
                               v
                     Tauri command layer ── generated IPC ── React workspace
                               |
                               v
                         agent-session
                               |
                               v
                         agent-runtime
```

`agent-runtime` must not depend on `agent-session`. `agent-session` must not depend on `review-agent`, `agent-tools`, Tauri, or React. Integration tests may use `agent-tools` as a development dependency.

The Tauri host selects a configured provider from the existing model allowlist, creates an S4 tool pack rooted at the canonical repository, supplies the S3 approval registry and S2 event sink, and passes only opaque session/run identifiers across IPC. The product run policy allows repository reads/list/search, asks for filesystem writes/patches/artifacts, and denies Shell and Web. Denied tools are omitted from model definitions instead of relying only on an execution-time denial.

## 4. Session contract

An `AgentSession` contains:

- opaque `session_id` and monotonic `revision`;
- a bounded system instruction owned by the host, never accepted from tool output;
- an optional bounded compacted memory summary;
- recent completed `User` and `Assistant` messages;
- no credentials, raw provider request/response, hidden reasoning, tool arguments, or tool results.

`SessionStore::begin_turn(session_id, run_id)` creates an exclusive lease from a snapshot. A second run for the same session is rejected as busy. A successful canonical final response commits the pending user and assistant messages and increments the revision. Cancellation, provider failure, invalid terminal output, or loop exhaustion releases the lease without mutating memory.

Sessions are capacity bounded. S5 returns a stable capacity error instead of silently evicting an active or user-visible session.

## 5. Memory and compaction

Recent completed messages are retained verbatim within per-message and aggregate limits. When recent memory exceeds the configured message or byte ceiling, the oldest complete user/assistant pair is passed to a `MemoryCompactor`.

The default compactor is deterministic and extractive: it records role-tagged, whitespace-normalized bounded excerpts and merges them into the existing summary. It never invokes a provider, follows instructions in memory, or retains tool results. The trait permits a future application-owned semantic compactor, but its output remains untrusted data and must obey the same size cap.

Compaction is atomic with turn commit. Failure to produce a bounded summary rejects the commit rather than leaving partially changed memory.

## 6. Context planning

Before every provider round, the engine builds this logical transcript:

1. host system instruction;
2. compacted-memory block marked as untrusted historical data;
3. recent user/assistant turns;
4. bounded RAG evidence marked as untrusted reference data;
5. current user message;
6. current-round assistant tool calls and sanitized tool results.

The available input budget is the provider context window minus configured output reservation and a safety margin. Tools and response schema count toward the estimate. ASCII is estimated at one token per four bytes; non-ASCII scalar values count as at least one token each. Unknown/zero provider windows fail closed unless the host supplies a lower explicit context limit.

If the request is too large, the planner first removes the oldest recent completed pair already represented in summary, then drops the lowest-ranked RAG chunks, then replaces the oldest current-turn tool-result content with a stable size-only compaction marker while preserving call/result protocol order. It never truncates JSON tool arguments or the current user message. If the irreducible request still does not fit, the turn fails with `context_exceeded` before provider I/O.

## 7. RAG contract

`RagRetriever` accepts the current user query and a result limit, returning `RagChunk { id, source, content, score }`. Retrieval implementations must return bounded UTF-8 data. The engine revalidates IDs, source labels, lengths, aggregate bytes, and result count before injection.

S5 provides:

- `NoopRagRetriever`, the production-safe default;
- `InMemoryRagIndex`, a deterministic lexical retriever for explicitly supplied chunks.

The lexical index normalizes alphanumeric tokens, scores unique query-term overlap, sorts by score then stable ID, and returns no zero-score chunks. It is useful for tests and small application-owned knowledge sets; it is not presented as semantic/vector retrieval.

The initial desktop composition deliberately uses `NoopRagRetriever`: no repository-wide index is built implicitly and no extra repository content is sent merely because an Agent session exists. A later application-owned index can be attached through `SessionEngine::with_retriever` without changing the session, provider, or IPC result contract.

RAG blocks contain no instructions from the host and are explicitly delimited as untrusted evidence. Retrieval content is sent only to the provider transcript, never to frontend events or logs.

## 8. Generic turn loop

For each turn:

1. acquire the session lease and build bounded context;
2. reserve a model round through `ToolRun`;
3. call `ModelProvider::respond_stream` with transient retry and run-scoped events;
4. accept only a fully reconstructed `ModelResponse`;
5. on tool calls, append the complete assistant call set, execute calls serially through `ToolExecutor`, and append sanitized `ToolResult` items;
6. on final text, validate bounds, atomically commit memory, and return the authoritative `AgentTurnResult`;
7. on cancellation or error, abort the lease and return a stable error.

Unknown tools and schema-invalid arguments become stable, non-sensitive tool-result errors so the model can recover. Duplicate/empty call IDs and cancellation remain terminal. Model-round, tool-call, cumulative token, result-byte, and wall-clock ceilings remain hard safety fuses, but they are not normal task-completion targets.

`AgentLoopPolicy` reserves model rounds, cumulative input/output tokens, and wall-clock time for tool-free final synthesis. Before a normal request would consume a reserve, or when the next model/tool call would reach a hard fuse, the engine stops advertising tools and requests the best final answer from the evidence already gathered. The repository product profile therefore uses high emergency fuses (64 model rounds and 128 tool calls), a 20-minute deadline, and separate synthesis reserves instead of treating the former 16/32 counters as business-level completion criteria.

The loop also fingerprints each complete tool batch from canonical tool names, complete arguments, and sanitized results while excluding provider call IDs. Three consecutive identical batches are classified as no progress and enter final synthesis. This is a loop detector, not an execution cache: each accepted call still passes schema, permission, cancellation, timeout, and budget enforcement before execution.

Final synthesis allows a small bounded number of model attempts. If a provider emits tool calls or recovered provider protocol after tools have been disabled, the calls are never executed or replayed as executable input; the engine adds one provider-neutral correction and retries within the reserved rounds. Continued protocol output fails with `loop_exhausted`. Final structured/text results remain authoritative and are committed only after normal validation.

Provider retries emit distinct attempt IDs on one monotonic run event sequence. Tool execution events are associated with the model attempt that produced the complete calls. Partial tool-argument stream deltas remain observational and never reach the executor.

## 9. Usage and result authority

The engine accumulates checked input/output usage and successful or attempted tool counts under configured ceilings. It estimates the next request before provider I/O so final-synthesis reserves can activate before the cumulative hard limit. Arithmetic overflow and actual hard-limit exhaustion still fail closed.

`AgentTurnResult` contains only session ID, run ID, committed revision, canonical final text, aggregate usage, model-round count, and retrieval count. Stream text, approval state, and tool events are not substituted for this result.

Existing PR review, issue triage, change plan, and history investigation continue to validate and return their existing structured result types. S5 does not route them through the generic session engine.

The React workspace treats `start_agent_turn` as the commit boundary. It may show transient stream deltas and tool/approval state while a run is active, but it appends an assistant message only from the successful command result and then refreshes the authoritative session snapshot. Cancellation or failure removes the optimistic user message.

## 10. Security and privacy

- Complete provider termination precedes any tool execution.
- Only S3-validated and approved calls reach handlers.
- Session memory contains no tool arguments/results, credentials, provider bodies, or hidden reasoning.
- RAG and memory are delimited as untrusted data, not executable instructions.
- Events contain no prompts, memory, RAG content, or raw tool results.
- Errors are stable categories without provider details or content echoes.
- Cancellation drops provider futures and active tool futures; S4 process kill-on-drop remains effective.
- Session IDs and run IDs are validated, bounded opaque identifiers.
- The application validates repository, run, model, and message input before credential access or provider construction.
- The frontend requires explicit consent before repository content may be sent to the selected provider.
- API keys are passed only to the provider factory and the runtime redactor; they are never returned in IPC DTOs.

## 11. Acceptance criteria

S5 is accepted when:

1. `agent-session` depends inward only on `agent-runtime` in production.
2. `AssistantText` round-trips and maps correctly in OpenAI, Anthropic, and DeepSeek requests.
3. Session creation, exclusive leases, atomic success commit, abort, revision, and capacity bounds are tested.
4. Compaction retains recent pairs, bounds the summary, and never retains tool results.
5. Context planning accounts for system, memory, history, RAG, tools, schema, current tool transcript, and output reservation.
6. Irreducible oversized input fails before provider I/O; tool results may be compacted without breaking call/result order.
7. RAG proves stable lexical ranking, no zero-score results, injection bounds, and prompt-injection delimiting.
8. The generic loop proves final-only commit, complete-call execution, schema failure recovery, approval denial recovery, cancellation, retry, duplicate IDs, high emergency fuses, synthesis reserves, repeated-batch no-progress detection, and tool-free finalization.
9. Event ordering remains monotonic and contains no memory, prompts, retrieval bodies, or raw tool results.
10. Workspace Rust tests, frontend tests, Clippy with warnings denied, rustfmt, dependency boundaries, and production build pass.
11. Tauri get/reset/start/cancel commands use opaque repository-scoped sessions, canonical repository roots, stable errors, and the existing run/approval registries.
12. The Agent workspace proves consent gating, tool-capable model filtering, final-result-only message commit, cancellation without memory commit, and backend session reset.

## 12. Test matrix

| Area | Cases |
|---|---|
| Session | invalid IDs, duplicate session, capacity, busy run, wrong lease, commit, abort, revision |
| Memory | pair retention, deterministic summary, Unicode bounds, compactor failure, no tool persistence |
| Transcript | assistant text serialization plus all three provider request mappings |
| Context | conservative estimate, unknown window, reserved output, history removal, RAG dropping, tool-result markers, irreducible overflow |
| RAG | tokenization, overlap ranking, tie order, limits, invalid chunks, untrusted delimiters |
| Loop | direct final, serial/parallel call sets, invalid/unknown tool recovery, synthesis reserves, repeated-batch no-progress detection, high loop ceiling |
| Safety | partial arguments never execute, duplicate IDs terminal, denied writes do not mutate, cancellation aborts memory |
| Retry/events | transient retry only, tool-free protocol correction, distinct attempts, monotonic sequence, sanitized event shapes and loop diagnostics |
| Integration | fake provider + S3 executor + S4 tools completes a read/patch/final turn under approval policy |
| IPC / host | DTO generation, input/model validation, opaque repository IDs, policy filtering, command registration in normal and E2E builds |
| React | loading/empty session, consent gate, authoritative completion, cancel rollback, reset, approval handoff |
| Regression | S1-S4 tests, Clippy, fmt, dependency boundaries, frontend tests/build |

## 13. Follow-on

S6 may serialize the content-only session contract, event journal, and run checkpoints for background execution, replay, and resume. It should preserve the S5 command/result authority boundary while adding durable repository-session metadata, crash-safe run state, explicit resume semantics, and an application-owned retrieval index lifecycle.
