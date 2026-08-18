# Agent Platform v2: Streaming Foundation

**Status:** Accepted for implementation  
**Date:** 2026-08-18  
**Stage:** S1-S2 of the Agent Platform v2 roadmap

## 1. Decision

VersionArc will treat model streaming as a runtime protocol, not a UI effect. S1 introduces a provider-neutral event contract, consumes each provider's real server-sent event stream, and reconstructs the same validated `ModelResponse` used by existing workflows. The Tauri and React delivery bridge is deliberately deferred to S2.

This preserves the current workflow safety model while creating one stable foundation for token streaming, tool-call progress, tracing, cancellation, retries, background execution, and future durable replay.

## 2. Current baseline

The project already has:

- provider-neutral requests and final responses;
- OpenAI, Anthropic, and DeepSeek adapters;
- structured output and tool-call validation;
- bounded tool loops, cancellation, retry policy, usage accounting, and traces;
- workflow progress events for coarse stages.

The missing layer is the stream between a provider request and its final response. Every adapter currently buffers the complete HTTP body, so users cannot observe text or tool arguments while the model is producing them.

## 3. Goals

S1 must:

1. Define one serializable `AgentEvent` envelope with a run ID, monotonic sequence, retry attempt ID, and provider-neutral event payload.
2. Add a backwards-compatible streaming method to `ModelProvider`.
3. Parse SSE correctly across arbitrary network chunk boundaries, CRLF or LF delimiters, comments, and multi-line data fields.
4. Use real streaming requests for OpenAI Responses, Anthropic Messages, and DeepSeek Chat Completions.
5. Emit normalized text, tool-call, tool-argument, usage, and lifecycle events.
6. Reconstruct and validate a canonical `ModelResponse` before a workflow is allowed to continue.
7. Preserve cancellation and bounded transient retry behavior.
8. Prove the protocol with fixture and contract tests.

## 4. Non-goals

S1 does not:

- expose stream events through Tauri or render them in React (S2);
- persist event logs or resume streams after process restart;
- add new tools, MCP, memory, RAG, multi-agent orchestration, or background runs;
- expose provider reasoning or hidden chain-of-thought;
- make partial output authoritative.

## 5. Event contract

```text
AgentEvent
├── run_id: string
├── sequence: u64                 # strictly increasing within the publisher
├── attempt_id: u32               # changes when a transient request is retried
└── kind
    ├── model_attempt_started
    ├── model_response_started
    ├── output_text_delta
    ├── tool_call_started
    ├── tool_arguments_delta
    ├── usage_updated
    ├── model_response_completed
    └── model_attempt_failed
```

The envelope is provider-neutral and serializable. A shared publisher owns both the sequence and attempt counters so IDs remain unique across every model round in a run. A per-attempt emitter binds the current attempt ID and prevents adapters from manufacturing ordering metadata.

`model_attempt_failed` carries a stable error category and `will_retry`; it never includes credentials, prompts, response bodies, or provider error text.

Consumers must key speculative display state by `(run_id, attempt_id)`. When an attempt fails, deltas from that attempt may remain visible as failed diagnostic output, but they must never be concatenated into the next attempt's final answer.

## 6. Provider mapping

### OpenAI Responses

- Request with `stream: true`.
- `response.created` -> response started.
- `response.output_text.delta` -> text delta.
- function-call lifecycle and argument delta events -> tool events.
- usage from the completed response -> usage update.
- `response.completed.response` is parsed by the existing final-response validator.
- `response.incomplete`, `response.failed`, or stream `error` fails closed.

### Anthropic Messages

- Request with `stream: true`.
- `message_start` -> response started and input usage.
- `content_block_start` for `tool_use` -> tool-call started.
- `text_delta` -> text delta.
- `input_json_delta` -> tool-argument delta.
- `message_delta` -> stop reason and cumulative output usage.
- `message_stop` completes an in-memory Message object, which is parsed by the existing validator.
- `ping` and unknown future events are ignored; `error` fails closed.

### DeepSeek Chat Completions

- Request with `stream: true` and `stream_options.include_usage: true`.
- First chunk -> response started.
- `delta.content` -> text delta.
- indexed `delta.tool_calls` fragments -> tool-call and argument events.
- final usage-only chunk -> usage update.
- `[DONE]` closes the stream and the accumulated chat completion is parsed by the existing validator.
- terminal `finish_reason` values retain existing truncation and failure semantics.

## 7. SSE transport rules

The parser operates on bytes, because a UTF-8 character and an SSE frame may both span HTTP chunks. It:

- buffers until a complete blank-line-delimited event exists;
- accepts `\n\n` and `\r\n\r\n`;
- joins multiple `data:` lines with `\n`;
- ignores comment and keep-alive lines;
- rejects invalid UTF-8 and malformed JSON at the adapter boundary;
- caps a single buffered event at 1 MiB to prevent unbounded memory growth;
- treats an EOF with a non-empty partial event as an invalid response.

Dropping the response byte stream cancels the underlying request. Existing `tokio::select!` cancellation therefore remains the cancellation boundary.

## 8. Final response authority

Stream deltas are observational. Workflows may render them later, but they must only execute tools or publish a final result after the adapter has:

1. observed a valid provider terminal event;
2. reconstructed the full provider response;
3. passed it through the existing provider-specific validator;
4. emitted `model_response_completed`.

This prevents partial JSON, partial tool arguments, truncated output, or an interrupted attempt from becoming executable state.

## 9. Retry semantics

The retry policy remains limited to transient network and rate-limit failures. Each HTTP attempt receives a unique `attempt_id` and emits explicit start/failure events. Sequence numbers never reset within a publisher.

Authentication failures, invalid responses, malformed streams, truncation, and validation errors are terminal. Cancellation interrupts either streaming or backoff and does not emit a synthetic completion.

S1's existing workflow wrapper uses a no-op sink while still taking the streaming code path. S2 will provide a run-scoped publisher to the Tauri bridge so sequence numbers span every model round in a workflow.

## 10. Security and privacy

- Events contain only model-visible output deltas, tool names/IDs, aggregate usage, and stable error categories.
- Hidden reasoning deltas are ignored.
- Credentials, request headers, prompts, tool results, raw provider bodies, and detailed network errors are not emitted.
- Final tool arguments continue through the existing schema and domain validation before execution.

## 11. Verification

S1 is accepted when:

- the runtime event contract round-trips through Serde;
- the SSE parser passes fragmented UTF-8, CRLF, comments, multi-line data, overflow, and incomplete-frame tests;
- all three adapters send streaming requests;
- text and tool fixtures reconstruct the same canonical responses as non-streaming fixtures;
- normalized event order and usage are asserted;
- retry attempts have distinct IDs and a single monotonic sequence;
- cancellation still interrupts an in-flight provider future;
- formatting, Clippy, workspace Rust tests, dependency boundaries, frontend tests, and frontend production build all pass.

## 12. S2 delivery bridge

S2 exposes the S1 protocol without weakening final-response authority:

- one run-scoped publisher owns sequence and attempt counters across all model rounds;
- Tauri emits a flat, forward-compatible `agent-event` DTO with no credentials, prompts, tool results, or raw provider errors;
- listeners subscribe before invoking a workflow, filter by run ID and sequence, and are removed on completion, replacement, or unmount;
- the React reducer retains each attempt independently, appends text and tool-argument fragments, replaces cumulative usage, and ignores duplicate, stale, or foreign events;
- PR/MR review, issue triage, model-enhanced change planning, and history investigation share the same stream panel;
- the existing cancellation actions stop provider streaming and retry backoff, while a new run resets the speculative stream state;
- partial text remains observational and is never used as a workflow result or executable tool input.

The panel follows the existing dense VersionArc visual system: compact activity rows, bounded scroll areas, existing color tokens, restrained status motion, and an `aria-live` region for incremental text. It does not introduce a separate chatbot shell or display hidden reasoning.

S2 is accepted when the IPC DTO shape is contract-tested, the reducer proves ordering/retry/tool assembly, listener cleanup is tested, the shared panel is component-tested, and the complete Rust and frontend verification suite passes.

## 13. Follow-on stages

- **S2:** Tauri event bridge, React stream store, rendering, stop/retry UX.
- **S3:** generic tool registry and permission contract.
- **S4:** shell, patch, filesystem, search, web, and artifact tools.
- **S5:** sessions, memory, context compaction, and RAG.
- **S6:** durable/background runs, replay, and resume.
- **S7:** MCP client and server integration.
- **S8:** production traces, evals, budgets, and observability.
- **S9+:** computer use, multi-agent orchestration, and advanced governance.

## 14. Protocol references

- [OpenAI Responses streaming](https://developers.openai.com/api/docs/guides/streaming-responses)
- [Anthropic streaming Messages](https://platform.claude.com/docs/en/build-with-claude/streaming)
- [DeepSeek Chat Completions streaming](https://api-docs.deepseek.com/api/create-chat-completion)
