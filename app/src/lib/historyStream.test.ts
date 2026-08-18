import { describe, expect, it } from "vitest";
import { createAgentStream, type AgentStreamState } from "./agentStream";
import { historyDraftFromStream } from "./historyStream";

describe("history stream draft", () => {
  it("assembles only backend-approved semantic answer fields", () => {
    const stream = withAttempts([
      attempt(1, [
        artifact("summary", null, "The guard was introduced"),
        artifact("finding_title", 0, "Startup guard"),
        artifact("finding_explanation", 0, "It prevents an empty graph."),
        { artifactType: "other_artifact", field: "summary", itemIndex: null, text: "Ignored" },
      ]),
    ]);

    expect(historyDraftFromStream(stream)).toEqual({
      summary: "The guard was introduced",
      findings: [{ title: "Startup guard", explanation: "It prevents an empty graph." }],
    });
  });

  it("does not infer display prose from raw JSON", () => {
    const raw = attempt(1, []);
    raw.text = '{"summary":"Raw model output","commit_ids":["secret-sha"]}';
    expect(historyDraftFromStream(withAttempts([raw]))).toBeNull();
  });

  it("uses only the latest retry attempt so rejected prose is cleared", () => {
    const stream = withAttempts([
      attempt(1, [artifact("summary", null, "Rejected answer")], "retrying"),
      attempt(2, [], "starting"),
    ]);

    expect(historyDraftFromStream(stream)).toBeNull();
    stream.attempts[1].artifactText.push(artifact("summary", null, "Replacement"));
    expect(historyDraftFromStream(stream)?.summary).toBe("Replacement");
  });
});

function withAttempts(attempts: AgentStreamState["attempts"]): AgentStreamState {
  return { ...createAgentStream("run-1"), attempts };
}

function artifact(field: string, itemIndex: number | null, text: string) {
  return { artifactType: "history_investigation", field, itemIndex, text };
}

function attempt(
  attemptId: number,
  artifactText: AgentStreamState["attempts"][number]["artifactText"],
  status: "starting" | "streaming" | "completed" | "retrying" | "failed" = "streaming",
): AgentStreamState["attempts"][number] {
  return {
    attemptId,
    providerId: "deepseek",
    modelId: "deepseek-chat",
    responseId: null,
    text: "",
    artifactText,
    tools: [],
    usage: null,
    errorCode: null,
    status,
  };
}
