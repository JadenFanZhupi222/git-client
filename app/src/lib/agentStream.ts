import type { AgentEventDto, ReviewUsageDto } from "../bindings";

export type AgentAttemptStatus = "starting" | "streaming" | "completed" | "retrying" | "failed";
export type AgentRunStatus = "active" | "completed" | "failed" | "cancelled";

export type AgentStreamTool = {
  callId: string;
  name: string;
  arguments: string;
  risk: string | null;
  approvalId: string | null;
  approvalSummary: string | null;
  permission: "none" | "pending" | "allowed" | "denied";
  execution: "pending" | "running" | "success" | "failed" | "denied";
  errorCode: string | null;
};

export type AgentStreamArtifactText = {
  artifactType: string;
  field: string;
  itemIndex: number | null;
  text: string;
};

export type AgentStreamAttempt = {
  attemptId: number;
  providerId: string | null;
  modelId: string | null;
  responseId: string | null;
  text: string;
  artifactText: AgentStreamArtifactText[];
  tools: AgentStreamTool[];
  usage: ReviewUsageDto | null;
  errorCode: string | null;
  status: AgentAttemptStatus;
};

export type AgentStreamState = {
  runId: string;
  runStatus: AgentRunStatus;
  lastSequence: number;
  attempts: AgentStreamAttempt[];
};

export function createAgentStream(runId: string): AgentStreamState {
  return { runId, runStatus: "active", lastSequence: 0, attempts: [] };
}

export function finishAgentStream(state: AgentStreamState, runStatus: Exclude<AgentRunStatus, "active">): AgentStreamState {
  return { ...state, runStatus };
}

function emptyAttempt(attemptId: number): AgentStreamAttempt {
  return {
    attemptId,
    providerId: null,
    modelId: null,
    responseId: null,
    text: "",
    artifactText: [],
    tools: [],
    usage: null,
    errorCode: null,
    status: "starting",
  };
}

function emptyTool(callId: string, name = "tool"): AgentStreamTool {
  return {
    callId,
    name,
    arguments: "",
    risk: null,
    approvalId: null,
    approvalSummary: null,
    permission: "none",
    execution: "pending",
    errorCode: null,
  };
}

function findOrCreateTool(attempt: AgentStreamAttempt, callId: string, name?: string): AgentStreamTool {
  let tool = attempt.tools.find((item) => item.callId === callId);
  if (!tool) {
    tool = emptyTool(callId, name);
    attempt.tools.push(tool);
  } else if (name) {
    tool.name = name;
  }
  return tool;
}

export function reduceAgentEvent(state: AgentStreamState, event: AgentEventDto): AgentStreamState {
  if (event.run_id !== state.runId || event.sequence <= state.lastSequence) return state;

  const attempts = state.attempts.map((attempt) => ({
    ...attempt,
    artifactText: attempt.artifactText.map((part) => ({ ...part })),
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
    case "artifact_text_delta": {
      if (!event.artifact_type || !event.artifact_field) break;
      let part = attempt.artifactText.find((candidate) => (
        candidate.artifactType === event.artifact_type
        && candidate.field === event.artifact_field
        && candidate.itemIndex === event.artifact_index
      ));
      if (!part) {
        part = {
          artifactType: event.artifact_type,
          field: event.artifact_field,
          itemIndex: event.artifact_index,
          text: "",
        };
        attempt.artifactText.push(part);
      }
      part.text += event.delta ?? "";
      attempt.status = "streaming";
      break;
    }
    case "artifact_text_reset": {
      const part = attempt.artifactText.find((candidate) => (
        candidate.artifactType === event.artifact_type
        && candidate.field === event.artifact_field
        && candidate.itemIndex === event.artifact_index
      ));
      if (part) part.text = "";
      break;
    }
    case "tool_call_started":
      if (event.call_id && !attempt.tools.some((tool) => tool.callId === event.call_id)) {
        attempt.tools.push(emptyTool(event.call_id, event.tool_name ?? "tool"));
      }
      attempt.status = "streaming";
      break;
    case "tool_arguments_delta": {
      if (!event.call_id) break;
      const tool = findOrCreateTool(attempt, event.call_id);
      tool.arguments += event.delta ?? "";
      attempt.status = "streaming";
      break;
    }
    case "tool_validation_failed": {
      if (!event.call_id) break;
      const tool = findOrCreateTool(attempt, event.call_id, event.tool_name ?? undefined);
      tool.execution = "failed";
      tool.errorCode = event.tool_error;
      break;
    }
    case "tool_approval_requested": {
      if (!event.call_id || !event.approval_id) break;
      const tool = findOrCreateTool(attempt, event.call_id, event.tool_name ?? undefined);
      tool.risk = event.risk;
      tool.approvalId = event.approval_id;
      tool.approvalSummary = event.approval_summary;
      tool.permission = "pending";
      break;
    }
    case "tool_approval_resolved": {
      if (!event.call_id) break;
      const tool = findOrCreateTool(attempt, event.call_id);
      tool.permission = event.decision === "allow" ? "allowed" : "denied";
      break;
    }
    case "tool_execution_started": {
      if (!event.call_id) break;
      const tool = findOrCreateTool(attempt, event.call_id, event.tool_name ?? undefined);
      tool.risk = event.risk;
      tool.execution = "running";
      break;
    }
    case "tool_execution_completed": {
      if (!event.call_id) break;
      const tool = findOrCreateTool(attempt, event.call_id, event.tool_name ?? undefined);
      tool.execution = event.tool_outcome === "success"
        ? "success"
        : event.tool_outcome === "denied" ? "denied" : "failed";
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
      attempt.artifactText = [];
      break;
  }

  attempts.sort((left, right) => left.attemptId - right.attemptId);
  return { ...state, lastSequence: event.sequence, attempts };
}
