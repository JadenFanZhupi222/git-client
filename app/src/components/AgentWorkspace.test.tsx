import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentSessionSnapshotDto, AgentSessionTurnResultDto, ReviewModelOptionDto } from "../bindings";
import { setLang } from "../lib/i18n";
import { AgentWorkspace } from "./AgentWorkspace";

const ipc = vi.hoisted(() => ({
  cancelAgentTurn: vi.fn(),
  credentialStatus: vi.fn(),
  getAgentSession: vi.fn(),
  listReviewModels: vi.fn(),
  onAgentEvent: vi.fn(),
  resetAgentSession: vi.fn(),
  resolveToolApproval: vi.fn(),
  startAgentTurn: vi.fn(),
}));

vi.mock("../ipc", () => ipc);

const emptySession: AgentSessionSnapshotDto = {
  session_id: "repo-1234",
  revision: 0,
  memory_summary: null,
  recent_messages: [],
};

const model: ReviewModelOptionDto = {
  id: "deepseek-chat",
  label: "DeepSeek Chat",
  provider: "DeepSeek",
  provider_id: "deepseek",
  capabilities: {
    context_window_tokens: 64_000,
    max_output_tokens: 8_192,
    supports_structured_output: true,
    supports_tool_calling: true,
    reports_usage: true,
  },
  pricing: null,
};

const completed: AgentSessionTurnResultDto = {
  session_id: "repo-1234",
  run_id: "agent-test",
  revision: 1,
  final_text: "Implemented the focused fix.",
  usage: { input_tokens: 120, output_tokens: 24, tool_calls: 1 },
  model_rounds: 2,
  retrieval_count: 0,
};

describe("AgentWorkspace", () => {
  beforeEach(() => {
    localStorage.clear();
    setLang("en");
    Object.values(ipc).forEach((mock) => mock.mockReset());
    ipc.cancelAgentTurn.mockResolvedValue(undefined);
    ipc.credentialStatus.mockResolvedValue(true);
    ipc.getAgentSession.mockResolvedValue(emptySession);
    ipc.listReviewModels.mockResolvedValue([model]);
    ipc.onAgentEvent.mockResolvedValue(() => undefined);
    ipc.resetAgentSession.mockResolvedValue(emptySession);
  });

  it("commits only the authoritative turn result into the visible conversation", async () => {
    let finish!: (value: AgentSessionTurnResultDto) => void;
    ipc.startAgentTurn.mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    ipc.getAgentSession
      .mockResolvedValueOnce(emptySession)
      .mockResolvedValueOnce({
        ...emptySession,
        revision: 1,
        recent_messages: [
          { role: "user", content: "Fix the parser" },
          { role: "assistant", content: completed.final_text },
        ],
      });

    render(<AgentWorkspace repo={"D:\\repo"} />);
    await screen.findByText("Start with a concrete task");
    fireEvent.click(screen.getByLabelText(/I agree to send/));
    fireEvent.change(screen.getByLabelText(/Describe what you want/), { target: { value: "Fix the parser" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    await screen.findByText("Fix the parser");
    expect(screen.queryByText(completed.final_text)).not.toBeInTheDocument();
    expect(ipc.startAgentTurn).toHaveBeenCalledWith(expect.objectContaining({
      repo_path: "D:\\repo",
      model_id: "deepseek-chat",
      message: "Fix the parser",
    }));

    finish(completed);
    expect(await screen.findByText(completed.final_text)).toBeInTheDocument();
    expect(await screen.findByText(/120 input · 24 output tokens · 2 model rounds/)).toBeInTheDocument();
  });

  it("cancels the active run and does not commit its pending user message", async () => {
    let reject!: (reason: unknown) => void;
    ipc.startAgentTurn.mockReturnValue(new Promise((_, rejectPromise) => { reject = rejectPromise; }));

    render(<AgentWorkspace repo={"D:\\repo"} />);
    await screen.findByText("Start with a concrete task");
    fireEvent.click(screen.getByLabelText(/I agree to send/));
    fireEvent.change(screen.getByLabelText(/Describe what you want/), { target: { value: "Do not keep this" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    fireEvent.click(await screen.findByRole("button", { name: "Stop" }));

    await waitFor(() => expect(ipc.cancelAgentTurn).toHaveBeenCalledTimes(1));
    const runId = ipc.startAgentTurn.mock.calls[0][0].run_id;
    expect(ipc.cancelAgentTurn).toHaveBeenCalledWith(runId);
    reject({ code: "AGENT_CANCELLED", message: "cancelled", recoverable: true, diagnostic_id: "diag-1" });
    expect(await screen.findByText(/message was not committed/)).toBeInTheDocument();
    expect(screen.queryByText("Do not keep this")).not.toBeInTheDocument();
  });

  it("resets repository-scoped memory through the backend contract", async () => {
    ipc.getAgentSession.mockResolvedValue({
      ...emptySession,
      revision: 2,
      recent_messages: [{ role: "assistant", content: "Old response" }],
    });

    render(<AgentWorkspace repo={"D:\\repo"} />);
    await screen.findByText("Old response");
    fireEvent.click(screen.getByRole("button", { name: "New session" }));

    await waitFor(() => expect(ipc.resetAgentSession).toHaveBeenCalledWith("D:\\repo"));
    expect(await screen.findByText("Start with a concrete task")).toBeInTheDocument();
    expect(screen.queryByText("Old response")).not.toBeInTheDocument();
  });
});
