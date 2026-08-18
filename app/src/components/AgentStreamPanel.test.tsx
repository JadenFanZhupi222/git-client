import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { setLang } from "../lib/i18n";
import type { AgentStreamState } from "../lib/agentStream";
import { AgentStreamPanel } from "./AgentStreamPanel";

describe("AgentStreamPanel", () => {
  beforeEach(() => setLang("en"));

  it("renders streamed text, tools, usage, and retry state", () => {
    const stream: AgentStreamState = {
      runId: "run-1",
      lastSequence: 7,
      attempts: [{
        attemptId: 1,
        providerId: "openai",
        modelId: "gpt-5",
        responseId: "response-1",
        text: "Partial answer",
        tools: [{ callId: "call-1", name: "read_file", arguments: "{\"path\":\"src/a.ts\"}" }],
        usage: { input_tokens: 12, output_tokens: 3, tool_calls: 1 },
        errorCode: "rate_limited",
        status: "retrying",
      }],
    };

    render(<AgentStreamPanel stream={stream} />);

    expect(screen.getByRole("region", { name: "Live agent activity" })).toBeInTheDocument();
    expect(screen.getByText("Partial answer")).toBeInTheDocument();
    expect(screen.getByText("read_file")).toBeInTheDocument();
    expect(screen.getByText("Retrying")).toBeInTheDocument();
    expect(screen.getByText("12 input · 3 output tokens")).toBeInTheDocument();
  });
});
