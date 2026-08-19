# Agent Platform v2: Durable Goal Runtime

**Status:** Implemented
**Date:** 2026-08-19
**Stage:** S6 of the Agent Platform v2 roadmap

## 1. Decision

S6 replaces the repository agent's atomic, in-memory turn with a durable Goal. A Goal is a repository-scoped unit of user intent that can span provider responses, tool batches, execution slices, application navigation, and process restarts. Model-round and tool-call counters remain only high runaway fuses; they never force synthesis or authorize a partial result.

The authoritative boundary remains host-owned. Stream events, checkpoints, and model final text are observational or provisional. Only a completion candidate that passes deterministic host checks and the applicable verifier is committed to session memory as the canonical result.

## 2. Ownership and dependency boundary

`agent-runtime` owns provider-neutral model, usage, tool, permission, intent, receipt, and safe event contracts. It does not know about Tauri, repositories, persistence formats, or React.

`agent-session` owns Goal state, state transitions, budget accounting, slice policy, progress detection, checkpoint working sets, completion candidates, verification decisions, and the persistence trait. It depends inward only on `agent-runtime`.

`agent-tools` implements resource-versioned observations and mutation/process/artifact receipts. Tool adapters never persist secrets or provider payloads.

The Tauri host owns the encrypted file store, installation key, repository identity, background run manager, provider/tool composition, crash reconciliation, IPC commands, and safe event projection. React reads authoritative snapshots and sends revision-checked commands.

## 3. Goal state machine

Every submitted repository-agent message creates a persistent `AgentGoal` unless the repository already has a nonterminal Goal. While a Goal is active, later messages are steering messages for that Goal and are injected at the next atomic provider-response or tool-effect boundary.

Statuses are `queued`, `running`, `awaiting_approval`, `pausing`, `paused`, `blocked`, `completed`, `failed`, and `cancelled`. Only the last three are terminal. Pause reasons are `user`, `app_restarted`, `budget`, and `provider_unavailable`. Block reasons are `workspace_conflict`, `ambiguous_tool_effect`, `no_progress`, `verifier_rejected`, `checkpoint_corrupt`, `storage_locked`, and `runaway_guard`.

Every mutation increments a monotonic revision. IPC mutations provide `expected_revision`; stale mutations fail with `AGENT_REVISION_CONFLICT`. One repository identity has at most one nonterminal Goal.

## 4. Execution slices

The default slice envelope is 120 seconds of active work, 250,000 input tokens, 16,000 output tokens, and 2 MiB of sanitized tool results. A boundary is checked only after a complete provider response or an atomic tool step. At a boundary the runtime compacts the working set, saves a checkpoint, and queues the next slice while the public Goal remains `running`.

For crash safety, a completed provider/tool batch may create an earlier atomic checkpoint before the maximum envelope is reached. This is an implementation checkpoint, not a completion or budget condition; context compaction is reserved for the pressure boundaries.

Slice exhaustion is not a budget error and does not request a final answer. A process restart changes persisted `running`, `pausing`, `awaiting_approval`, or `queued` state to `paused/app_restarted`; resumption requires an explicit user command and never performs an automatic provider request or write.

Tool/model count limits are deliberately high fuses. Crossing one checkpoints and changes the Goal to `blocked/runaway_guard` without committing a candidate.

## 5. Durable checkpoint

`GoalPersistence` is a host-implemented trait over complete versioned `AgentSessionState` snapshots. The production Tauri implementation stores `app_data/agent-sessions/v1/<opaque-session-id>.state` using XChaCha20-Poly1305 with a random 256-bit installation key held by the operating-system credential store. The header is versioned, every write has a random nonce, and the opaque session ID is authenticated as associated data.

Writes take an exclusive file lock, serialize to a sibling temporary file, call `sync_all`, atomically replace the destination, and sync the parent where supported. Missing keys, authentication failure, corruption, unsupported versions, or lock failure fail closed. Corrupt files are retained for explicit user recovery and never overwritten automatically. There is no plaintext fallback.

Encrypted state may contain Goal text, steering, sanitized and bounded transcript/evidence, provider-neutral summaries, usage, price snapshots, budgets, pending intents, and receipts. It must never contain API keys, raw provider request/response bodies, hidden reasoning, unsanitized tool results, provider protocol/DSML text, or frontend partial tool arguments. Terminal cleanup removes pending argument bodies and working evidence while retaining canonical conversation, usage, price snapshots, and content-free receipt metadata.

## 6. Tool intent and receipt protocol

`ToolHandlerOutput` contains `sanitized_content` and a `ToolReceipt`. Receipt variants are observations, mutations, artifacts, and processes. Every effectful call receives a stable `execution_id`.

Execution order is strict: complete provider parsing, schema validation, permission decision, durable/fsynced intent, effect execution, receipt verification, durable receipt, then sanitized model context. Partial arguments are never executable input.

Observations carry a resource and version digest. Mutation receipts carry execution ID, resource, before digest, and after digest. Artifacts carry an artifact and content digest. Process receipts carry a bounded program identifier, exit code, and replay policy.

On recovery, a mutation whose current digest equals `after_digest` is recovered as success without replay. A current digest equal to `before_digest` may be retried only after fresh approval. Any other digest blocks as `ambiguous_tool_effect`. Processes are never automatically replayed. Approval IDs expire on restart.

Filesystem reads provide a version digest. Write and patch intents bind an observed version. Pure-read workspace drift invalidates and refreshes evidence; any drift with a pending or completed mutation blocks for review.

## 7. Context and progress

A checkpoint working set contains objective and steering, an untrusted structured summary, the last two complete tool batches, evidence source/digest entries, mutation receipts, verifier gaps, and next actions. Old read bodies covered by the summary are removed first. Unresolved calls, pending intents, receipts, and current steering are never compacted away.

Progress is a change in evidence digest, repository state, successful receipt, verifier evidence, or verifier gaps. Four consecutive steps without progress trigger one recovery slice. Two further no-progress steps block with `no_progress`; they do not manufacture a completion.

## 8. Budget accounts

Usage distinguishes cached input, cache-miss input, output, and tool calls. Providers that omit cache details are charged as cache-miss. A Goal stores a price snapshot per model so catalog changes do not rewrite historical cost.

Default soft limits are DeepSeek Flash CNY 1, DeepSeek Pro CNY 2, GPT-5.6 Luna USD 0.25, Terra USD 0.50, Sol USD 1.00, Claude Sonnet 5 USD 0.50, and Claude Opus 5 USD 1.00. DeepSeek catalog prices use CNY: Flash CNY 0.02/M cached input, CNY 1/M cache-miss input and CNY 2/M output; Pro CNY 0.025/M, CNY 3/M and CNY 6/M.

Crossing a limit pauses with `budget`; it never fails or commits. An extension can only increase the limit. Resuming with a different model creates or reuses that model's independent account. An unpriced future model requires an explicit token budget.

Before provider I/O, every Goal model request is quoted against the active account using conservatively estimated cache-miss input plus the configured maximum output. Insufficient remaining budget checkpoints and pauses without issuing the request. Checkpoint compaction is skipped when its bounded quote does not fit, and an independent verifier reserves both its initial and single repair attempt before its first request. Provider-reported usage remains authoritative after completion; the preflight quote is only a no-large-overshoot guard.

## 9. Completion authority

`FinalText` creates a bounded completion candidate. Deterministic checks require no unresolved call or intent, valid receipts for claimed effects, receipt digests matching current resources, no provider protocol residue, no unsupported claims of writes/tests, and an empty `remaining_work` set.

A direct single-response answer that used no tools and received no steering may pass deterministic checks alone. A multi-response, steered, tool-using, or mutating Goal requires an independent tool-free verifier returning `{decision, gaps, evidence_ids}`. `accepted` commits the candidate; `continue` checkpoints its gaps and schedules another slice; `blocked` changes state to `blocked/verifier_rejected`. Invalid verifier output is retried once and then blocks. Candidate text never appears as a committed assistant message before acceptance.

## 10. IPC and event contract

The Goal IPC surface is `create_agent_goal`, `get_agent_session`, `get_agent_goal`, `steer_agent_goal`, `pause_agent_goal`, `resume_agent_goal`, `cancel_agent_goal`, `extend_agent_budget`, and the existing approval resolution command. Create returns after durable queueing, not after completion.

Snapshots are authoritative. Safe events include status changes, checkpoint saved, steering accepted, budget updated, completion candidate metadata, completion verified, and a `tool_call_ready` summary. Events/logs contain identifiers, safe tool/model names, statuses, counts, durations, sizes, stable errors, and receipt digests only. Raw `ToolArgumentsDelta` is not projected to frontend IPC.

## 11. React behavior

The Agent workspace restores the active Goal from `get_agent_session`. Navigation does not cancel it. The composer remains enabled for steering. Explicit Pause, Resume, Cancel, approvals, budget extension, conflict status, and restart-resume controls operate with snapshot revisions. Reset refuses while a Goal is nonterminal. Only a canonical committed result is rendered as an assistant message.

## 12. Acceptance criteria and test matrix

S6 is accepted when tests prove:

1. a Goal crosses the old 16/32 limits and slice boundaries without forced finalization;
2. restart recovery is paused and performs no provider/effect work until resume;
3. steering injection and one-active-Goal-per-repository semantics;
4. per-model cached/miss/output cost, soft pause, monotonic extension, and same-checkpoint resume;
5. verifier accepted/continue/blocked/invalid behavior and canonical-result authority;
6. intent/effect/receipt crash points, exact-once mutation recovery, process non-replay, and ambiguity blocking;
7. read drift refresh versus mutation drift blocking;
8. authenticated encryption, no plaintext markers, key/lock/auth/version fail-closed behavior;
9. no partial arguments, DSML, raw provider bodies, prompts, API keys, or unsanitized results in frontend DTOs/events/logs;
10. React navigation, restart, steering, pause/resume/cancel, budget extension, approvals, reset rejection, and final commit behavior;
11. all S1-S5 provider parsing, schema, permission, cancellation, result sanitization, and structured-result contracts remain green.

The release gate is `pnpm test`, `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --all -- --check`, and `pnpm build`.
