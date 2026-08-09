import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { setLang } from "../lib/i18n";
import { HistoryInvestigationWorkspace } from "./HistoryInvestigationWorkspace";

const ipc = vi.hoisted(() => ({
  cancelHistoryInvestigation: vi.fn(),
  credentialStatus: vi.fn(),
  investigateRepositoryHistory: vi.fn(),
  listReviewModels: vi.fn(),
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
  }],
  caveats: ["Earlier history was outside the evidence window."],
  usage: { input_tokens: 120, output_tokens: 44, tool_calls: 0 },
  model_id: model.id,
  provider_attempts: 1,
};

describe("HistoryInvestigationWorkspace", () => {
  beforeEach(() => {
    localStorage.clear();
    setLang("en");
    vi.clearAllMocks();
    ipc.cancelHistoryInvestigation.mockResolvedValue(undefined);
    ipc.credentialStatus.mockResolvedValue(true);
    ipc.listReviewModels.mockResolvedValue([model]);
    ipc.investigateRepositoryHistory.mockResolvedValue(result);
  });

  it("requires consent and sends the selected file as bounded evidence scope", async () => {
    const user = userEvent.setup();
    render(
      <HistoryInvestigationWorkspace
        repo="D:/repo"
        selectedFile="src/history.rs"
        onClose={vi.fn()}
      />,
    );

    await screen.findByText("GPT-5.4 mini");
    const question = screen.getByLabelText("What do you want to understand?");
    await user.type(question, "Why was the startup guard introduced?");
    const run = screen.getByRole("button", { name: "Investigate history" });
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
  });

  it("can broaden the investigation from the selected file to recent repository history", async () => {
    const user = userEvent.setup();
    localStorage.setItem("history-investigation-model-consent-v1", "accepted");
    render(
      <HistoryInvestigationWorkspace
        repo="D:/repo"
        selectedFile="src/history.rs"
        onClose={vi.fn()}
      />,
    );

    await screen.findByText("GPT-5.4 mini");
    await user.type(screen.getByLabelText("What do you want to understand?"), "How did this architecture evolve?");
    await user.click(screen.getByRole("checkbox", { name: /Limit evidence to the selected file/ }));
    await user.click(screen.getByRole("button", { name: "Investigate history" }));

    await waitFor(() => expect(ipc.investigateRepositoryHistory).toHaveBeenCalledWith(
      expect.objectContaining({ file: null }),
    ));
  });
});
