import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentEventDto } from "../bindings";
import { setLang } from "../lib/i18n";
import { HistoryInvestigationWorkspace } from "./HistoryInvestigationWorkspace";

const ipc = vi.hoisted(() => ({
  cancelHistoryInvestigation: vi.fn(),
  credentialStatus: vi.fn(),
  investigateRepositoryHistory: vi.fn(),
  listReviewModels: vi.fn(),
  onAgentEvent: vi.fn(),
}));
vi.mock("../ipc", () => ipc);

const model = {
  id: "gpt-5.4-mini",
  label: "GPT-5.4 mini",
  provider: "OpenAI",
  provider_id: "openai",
  capabilities: {
    supports_tool_calling: true,
    supports_structured_output: true,
    context_window_tokens: 400_000,
    max_output_tokens: 32_000,
    reports_usage: true,
  },
  pricing: null,
};

const result = {
  snapshot_id: "history-snapshot",
  summary: "The empty-repository guard was introduced to keep startup predictable.",
  confidence: "high",
  findings: [{
    title: "Startup guard",
    explanation: "The commit adds a fallback before the graph is loaded.",
    commit_ids: ["abc1234"],
    paths: ["src/history.rs"],
    evidence_links: [{ commit_id: "abc1234", path: "src/history.rs" }],
  }],
  caveats: ["Earlier history was outside the evidence window."],
  search_terms: ["startup guard"],
  evidence_sources: ["file_history", "pickaxe", "blame", "commit_diffs"],
  evidence_commit_count: 12,
  usage: { input_tokens: 120, output_tokens: 44, tool_calls: 0 },
  model_id: model.id,
  provider_attempts: 1,
};

let agentEventListener: ((event: AgentEventDto) => void) | null = null;

describe("HistoryInvestigationWorkspace", () => {
  beforeEach(() => {
    localStorage.clear();
    setLang("en");
    vi.clearAllMocks();
    ipc.cancelHistoryInvestigation.mockResolvedValue(undefined);
    ipc.credentialStatus.mockResolvedValue(true);
    ipc.listReviewModels.mockResolvedValue([model]);
    ipc.investigateRepositoryHistory.mockResolvedValue(result);
    agentEventListener = null;
    ipc.onAgentEvent.mockImplementation(async (listener: (event: AgentEventDto) => void) => {
      agentEventListener = listener;
      return vi.fn();
    });
  });

  it("requires consent and sends the selected file as bounded evidence scope", async () => {
    const user = userEvent.setup();
    const onOpenEvidence = vi.fn();
    render(
      <HistoryInvestigationWorkspace
        repo="D:/repo"
        selectedFile="src/history.rs"
        onClose={vi.fn()}
        onOpenEvidence={onOpenEvidence}
      />,
    );

    await screen.findByText("GPT-5.4 mini");
    const question = screen.getByLabelText("What code decision do you want to trace?");
    await user.type(question, "Why was the startup guard introduced?");
    const run = screen.getByRole("button", { name: "Find evidence and answer" });
    expect(run).toBeDisabled();

    await user.click(screen.getByRole("checkbox", { name: /I agree to send bounded commit metadata/ }));
    expect(run).toBeEnabled();
    await user.click(run);

    await waitFor(() => expect(ipc.investigateRepositoryHistory).toHaveBeenCalledWith({
      run_id: expect.any(String),
      repo_path: "D:/repo",
      question: "Why was the startup guard introduced?",
      file: "src/history.rs",
      model_id: model.id,
    }));
    expect(await screen.findByText("Startup guard")).toBeInTheDocument();
    expect(screen.getByText("abc1234")).toBeInTheDocument();
    expect(screen.getByText("High confidence")).toBeInTheDocument();
    expect(screen.getByText("Pickaxe search")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "src/history.rs" }));
    expect(onOpenEvidence).toHaveBeenCalledWith("abc1234", "src/history.rs");
  });

  it("can broaden the investigation from the selected file to recent repository history", async () => {
    const user = userEvent.setup();
    localStorage.setItem("history-investigation-model-consent-v1", "accepted");
    render(
      <HistoryInvestigationWorkspace
        repo="D:/repo"
        selectedFile="src/history.rs"
        onClose={vi.fn()}
        onOpenEvidence={vi.fn()}
      />,
    );

    await screen.findByText("GPT-5.4 mini");
    await user.type(screen.getByLabelText("What code decision do you want to trace?"), "How did this architecture evolve?");
    await user.click(screen.getByRole("checkbox", { name: /Limit evidence to the selected file/ }));
    await user.click(screen.getByRole("button", { name: "Find evidence and answer" }));

    await waitFor(() => expect(ipc.investigateRepositoryHistory).toHaveBeenCalledWith(
      expect.objectContaining({ file: null }),
    ));
  });

  it("streams answer prose before the validated command result is available", async () => {
    const user = userEvent.setup();
    localStorage.setItem("history-investigation-model-consent-v1", "accepted");
    let resolveInvestigation!: (value: typeof result) => void;
    ipc.investigateRepositoryHistory.mockImplementation(() => new Promise((resolve) => {
      resolveInvestigation = resolve;
    }));

    render(
      <HistoryInvestigationWorkspace
        repo="D:/repo"
        selectedFile={null}
        onClose={vi.fn()}
        onOpenEvidence={vi.fn()}
      />,
    );

    await screen.findByText("GPT-5.4 mini");
    await user.type(screen.getByLabelText("What code decision do you want to trace?"), "Why was the startup guard introduced?");
    await user.click(screen.getByRole("button", { name: "Find evidence and answer" }));
    await waitFor(() => expect(ipc.investigateRepositoryHistory).toHaveBeenCalled());
    const runId = ipc.investigateRepositoryHistory.mock.calls[0][0].run_id as string;

    act(() => {
      agentEventListener?.(agentEvent(runId, 1, "model_attempt_started", { model_id: model.id }));
      agentEventListener?.(agentEvent(runId, 2, "output_text_delta", {
        delta: '{"summary":"The guard was intro',
      }));
      agentEventListener?.(agentEvent(runId, 3, "artifact_text_delta", {
        artifact_type: "history_investigation",
        artifact_field: "summary",
        delta: "The guard was intro",
      }));
    });

    expect(screen.getByLabelText("Streaming history answer")).toHaveTextContent("The guard was intro");
    expect(screen.queryByText("High confidence")).not.toBeInTheDocument();
    expect(screen.queryByText("abc1234")).not.toBeInTheDocument();

    act(() => {
      agentEventListener?.(agentEvent(runId, 4, "output_text_delta", {
        delta: 'duced to keep startup predictable.","findings":[{"title":"Startup guard","explanation":"The commit adds a fall',
      }));
      agentEventListener?.(agentEvent(runId, 5, "artifact_text_delta", {
        artifact_type: "history_investigation",
        artifact_field: "summary",
        delta: "duced to keep startup predictable.",
      }));
      agentEventListener?.(agentEvent(runId, 6, "artifact_text_delta", {
        artifact_type: "history_investigation",
        artifact_field: "finding_title",
        artifact_index: 0,
        delta: "Startup guard",
      }));
      agentEventListener?.(agentEvent(runId, 7, "artifact_text_delta", {
        artifact_type: "history_investigation",
        artifact_field: "finding_explanation",
        artifact_index: 0,
        delta: "The commit adds a fall",
      }));
    });

    expect(screen.getByLabelText("Streaming history answer")).toHaveTextContent("The guard was introduced to keep startup predictable.");
    expect(screen.getByLabelText("Streaming history answer")).toHaveTextContent("The commit adds a fall");

    await act(async () => resolveInvestigation(result));
    expect(await screen.findByText("High confidence")).toBeInTheDocument();
    expect(screen.getByText("abc1234")).toBeInTheDocument();
  });

  it("shows a failed terminal state when evidence collection ends before model events", async () => {
    const user = userEvent.setup();
    localStorage.setItem("history-investigation-model-consent-v1", "accepted");
    ipc.investigateRepositoryHistory.mockRejectedValue({
      code: "HISTORY_EVIDENCE_FAILED",
      message: "Could not collect evidence",
      recoverable: true,
      diagnostic_id: "diag-evidence",
    });
    render(
      <HistoryInvestigationWorkspace
        repo="D:/repo"
        selectedFile={null}
        onClose={vi.fn()}
        onOpenEvidence={vi.fn()}
      />,
    );

    await screen.findByText("GPT-5.4 mini");
    await user.type(screen.getByLabelText("What code decision do you want to trace?"), "Why did evidence collection fail?");
    await user.click(screen.getByRole("button", { name: "Find evidence and answer" }));

    expect(await screen.findByText("Run ended without a validated answer")).toBeInTheDocument();
    expect(screen.queryByText("Gathering bounded repository evidence…")).not.toBeInTheDocument();
  });

  it("shows cancellation as a neutral terminal state without an error alert", async () => {
    const user = userEvent.setup();
    localStorage.setItem("history-investigation-model-consent-v1", "accepted");
    let rejectInvestigation!: (reason: unknown) => void;
    ipc.investigateRepositoryHistory.mockImplementation(() => new Promise((_, reject) => {
      rejectInvestigation = reject;
    }));
    ipc.cancelHistoryInvestigation.mockImplementation(async () => {
      rejectInvestigation({
        code: "CANCELLED",
        message: "Cancelled",
        recoverable: true,
        diagnostic_id: "diag-cancel",
      });
    });
    render(
      <HistoryInvestigationWorkspace
        repo="D:/repo"
        selectedFile={null}
        onClose={vi.fn()}
        onOpenEvidence={vi.fn()}
      />,
    );

    await screen.findByText("GPT-5.4 mini");
    await user.type(screen.getByLabelText("What code decision do you want to trace?"), "Why was this implementation introduced?");
    await user.click(screen.getByRole("button", { name: "Find evidence and answer" }));
    await user.click(await screen.findByRole("button", { name: "Cancel" }));

    expect(await screen.findByText("Run cancelled")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});

function agentEvent(
  runId: string,
  sequence: number,
  eventType: string,
  fields: Partial<AgentEventDto> = {},
): AgentEventDto {
  return {
    run_id: runId,
    sequence,
    attempt_id: 1,
    event_type: eventType,
    provider_id: "deepseek",
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
