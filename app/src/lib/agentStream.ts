import type { AgentEventDto, ReviewUsageDto } from "../bindings";

export type AgentAttemptStatus = "starting" | "streaming" | "completed" | "retrying" | "failed";

export type AgentStreamTool = {
  callId: string;
  name: string;
  arguments: string;
};

export type AgentStreamAttempt = {
  attemptId: number;
  providerId: string | null;
  modelId: string | null;
  responseId: string | null;
  text: string;
  tools: AgentStreamTool[];
  usage: ReviewUsageDto | null;
  errorCode: string | null;
  status: AgentAttemptStatus;
};

export type AgentStreamState = {
  runId: string;
  lastSequence: number;
  attempts: AgentStreamAttempt[];
};

export function createAgentStream(runId: string): AgentStreamState {
  return { runId, lastSequence: 0, attempts: [] };
}

function emptyAttempt(attemptId: number): AgentStreamAttempt {
  return {
    attemptId,
    providerId: null,
    modelId: null,
    responseId: null,
    text: "",
    tools: [],
    usage: null,
    errorCode: null,
    status: "starting",
  };
}

export function reduceAgentEvent(state: AgentStreamState, event: AgentEventDto): AgentStreamState {
  if (event.run_id !== state.runId || event.sequence <= state.lastSequence) return state;

  const attempts = state.attempts.map((attempt) => ({
    ...attempt,
    tools: attempt.tools.map((tool) => ({ ...tool })),
  }));
  let attempt = attempts.find((item) => item.attemptId === event.attempt_id);
  if (!attempt) {
    attempt = emptyAttempt(event.attempt_id);
    attempts.push(attempt);
  }

  switch (event.event_type) {
    case "model_attempt_started":
      attempt.providerId = event.provider_id;
      attempt.modelId = event.model_id;
      attempt.status = "starting";
      break;
    case "model_response_started":
      attempt.responseId = event.response_id;
      attempt.status = "streaming";
      break;
    case "output_text_delta":
      attempt.text += event.delta ?? "";
      attempt.status = "streaming";
      break;
    case "tool_call_started":
      if (event.call_id && !attempt.tools.some((tool) => tool.callId === event.call_id)) {
        attempt.tools.push({ callId: event.call_id, name: event.tool_name ?? "tool", arguments: "" });
      }
      attempt.status = "streaming";
      break;
    case "tool_arguments_delta": {
      if (!event.call_id) break;
      let tool = attempt.tools.find((item) => item.callId === event.call_id);
      if (!tool) {
        tool = { callId: event.call_id, name: "tool", arguments: "" };
        attempt.tools.push(tool);
      }
      tool.arguments += event.delta ?? "";
      attempt.status = "streaming";
      break;
    }
    case "usage_updated":
      attempt.usage = event.usage;
      break;
    case "model_response_completed":
      attempt.status = "completed";
      break;
    case "model_attempt_failed":
      attempt.errorCode = event.error_code;
      attempt.status = event.will_retry ? "retrying" : "failed";
      break;
  }

  attempts.sort((left, right) => left.attemptId - right.attemptId);
  return { ...state, lastSequence: event.sequence, attempts };
}
