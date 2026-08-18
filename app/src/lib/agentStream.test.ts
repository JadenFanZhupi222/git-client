import { describe, expect, it } from "vitest";
import type { AgentEventDto } from "../bindings";
import { createAgentStream, finishAgentStream, reduceAgentEvent } from "./agentStream";

function event(sequence: number, attemptId: number, eventType: string, fields: Partial<AgentEventDto> = {}): AgentEventDto {
  return {
    run_id: "run-1",
    sequence,
    attempt_id: attemptId,
    event_type: eventType,
    provider_id: null,
    model_id: null,
    response_id: null,
    delta: null,
    artifact_type: null,
    artifact_field: null,
    artifact_index: null,
    call_id: null,
    tool_name: null,
    usage: null,
    error_code: null,
    will_retry: null,
    ...fields,
  };
}

describe("agent stream reducer", () => {
  it("assembles text, tool arguments, and usage in sequence order", () => {
    let state = createAgentStream("run-1");
    state = reduceAgentEvent(state, event(1, 1, "model_attempt_started", { provider_id: "openai", model_id: "gpt-5" }));
    state = reduceAgentEvent(state, event(2, 1, "output_text_delta", { delta: "Hel" }));
    state = reduceAgentEvent(state, event(3, 1, "output_text_delta", { delta: "lo" }));
    state = reduceAgentEvent(state, event(4, 1, "tool_call_started", { call_id: "call-1", tool_name: "read_file" }));
    state = reduceAgentEvent(state, event(5, 1, "tool_arguments_delta", { call_id: "call-1", delta: "{\"path\":" }));
    state = reduceAgentEvent(state, event(6, 1, "tool_arguments_delta", { call_id: "call-1", delta: "\"src/a.ts\"}" }));
    state = reduceAgentEvent(state, event(7, 1, "usage_updated", { usage: { input_tokens: 12, output_tokens: 5, tool_calls: 1 } }));

    expect(state.attempts[0]).toMatchObject({ text: "Hello", providerId: "openai", modelId: "gpt-5", status: "streaming" });
    expect(state.attempts[0].tools).toEqual([{ callId: "call-1", name: "read_file", arguments: "{\"path\":\"src/a.ts\"}" }]);
    expect(state.attempts[0].usage?.output_tokens).toBe(5);
  });

  it("keeps failed retries and ignores duplicate or foreign events", () => {
    let state = createAgentStream("run-1");
    state = reduceAgentEvent(state, event(1, 1, "model_attempt_failed", { error_code: "rate_limited", will_retry: true }));
    state = reduceAgentEvent(state, event(2, 2, "model_attempt_started", { model_id: "gpt-5" }));
    const current = state;
    state = reduceAgentEvent(state, event(2, 2, "output_text_delta", { delta: "duplicate" }));
    state = reduceAgentEvent(state, { ...event(3, 2, "output_text_delta", { delta: "foreign" }), run_id: "run-2" });

    expect(state).toBe(current);
    expect(state.attempts.map((attempt) => attempt.status)).toEqual(["retrying", "starting"]);
  });

  it("assembles semantic artifact text separately from raw model output", () => {
    let state = createAgentStream("run-1");
    state = reduceAgentEvent(state, event(1, 1, "artifact_text_delta", {
      artifact_type: "history_investigation",
      artifact_field: "summary",
      artifact_index: null,
      delta: "The guard ",
    }));
    state = reduceAgentEvent(state, event(2, 1, "artifact_text_delta", {
      artifact_type: "history_investigation",
      artifact_field: "summary",
      artifact_index: null,
      delta: "was added",
    }));
    state = reduceAgentEvent(state, event(3, 1, "artifact_text_reset", {
      artifact_type: "history_investigation",
      artifact_field: "summary",
      artifact_index: null,
    }));
    state = reduceAgentEvent(state, event(4, 1, "artifact_text_delta", {
      artifact_type: "history_investigation",
      artifact_field: "summary",
      artifact_index: null,
      delta: "Replacement",
    }));

    expect(state.attempts[0].text).toBe("");
    expect(state.attempts[0].artifactText).toEqual([{
      artifactType: "history_investigation",
      field: "summary",
      itemIndex: null,
      text: "Replacement",
    }]);
  });

  it("clears rejected artifact prose during retry backoff and records the run terminal state", () => {
    let state = createAgentStream("run-1");
    state = reduceAgentEvent(state, event(1, 1, "artifact_text_delta", {
      artifact_type: "history_investigation",
      artifact_field: "summary",
      delta: "Rejected",
    }));
    state = reduceAgentEvent(state, event(2, 1, "model_attempt_failed", {
      error_code: "invalid_response",
      will_retry: true,
    }));

    expect(state.attempts[0].artifactText).toEqual([]);
    expect(state.attempts[0].status).toBe("retrying");
    expect(finishAgentStream(state, "failed").runStatus).toBe("failed");
  });
});
