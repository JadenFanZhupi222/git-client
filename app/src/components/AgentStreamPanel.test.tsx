import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { setLang } from "../lib/i18n";
import type { AgentStreamState } from "../lib/agentStream";
import { AgentStreamPanel } from "./AgentStreamPanel";

describe("AgentStreamPanel", () => {
  beforeEach(() => setLang("en"));

  it("renders semantic activity and keeps raw model data collapsed", async () => {
    const user = userEvent.setup();
    const stream: AgentStreamState = {
      runId: "run-1",
      runStatus: "active",
      lastSequence: 7,
      attempts: [{
        attemptId: 1,
        providerId: "openai",
        modelId: "gpt-5",
        responseId: "response-1",
        text: "Partial answer",
        artifactText: [],
        tools: [{ callId: "call-1", name: "read_file", arguments: "{\"path\":\"src/a.ts\"}" }],
        usage: { input_tokens: 12, output_tokens: 3, tool_calls: 1 },
        errorCode: "rate_limited",
        status: "retrying",
      }],
    };

    render(<AgentStreamPanel stream={stream} />);

    expect(screen.getByRole("region", { name: "Live agent activity" })).toBeInTheDocument();
    expect(screen.getByText("Response incomplete; retrying")).toBeInTheDocument();
    expect(screen.getByText("Called read_file")).toBeInTheDocument();
    expect(screen.getByText("Retrying")).toBeInTheDocument();
    expect(screen.getByText(/12 input.*3 output tokens/)).toBeInTheDocument();
    const debugDetails = screen.getByText("Debug details").closest("details");
    expect(debugDetails).not.toHaveAttribute("open");

    await user.click(screen.getByText("Debug details"));
    expect(debugDetails).toHaveAttribute("open");
    expect(screen.getByText("Partial answer")).toBeInTheDocument();
    expect(screen.getByText('{"path":"src/a.ts"}')).toBeInTheDocument();
  });

  it("uses a workflow-specific preparation label before model events arrive", () => {
    render(
      <AgentStreamPanel
        stream={{ runId: "run-2", runStatus: "active", lastSequence: 0, attempts: [] }}
        preparingLabel="Gathering repository evidence"
      />,
    );

    expect(screen.getByText("Gathering repository evidence")).toBeInTheDocument();
  });

  it("shows explicit failed and cancelled terminal states instead of implying success", () => {
    const failed = {
      runId: "run-failed",
      runStatus: "failed" as const,
      lastSequence: 0,
      attempts: [],
    };
    const view = render(<AgentStreamPanel stream={failed} />);
    expect(screen.getByText("Run ended without a validated answer")).toBeInTheDocument();
    expect(screen.queryByText("Generating structured answer")).not.toBeInTheDocument();

    view.rerender(<AgentStreamPanel stream={{ ...failed, runStatus: "cancelled" }} />);
    expect(screen.getByText("Run cancelled")).toBeInTheDocument();
  });
});
