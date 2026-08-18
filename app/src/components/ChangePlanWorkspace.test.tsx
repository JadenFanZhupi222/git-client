import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { setLang } from "../lib/i18n";
import { ChangePlanWorkspace } from "./ChangePlanWorkspace";

const ipc = vi.hoisted(() => ({
  analyzeChangePlan: vi.fn(),
  cancelChangePlan: vi.fn(),
  commitChangeGroup: vi.fn(),
  credentialStatus: vi.fn(),
  listReviewModels: vi.fn(),
  onAgentEvent: vi.fn(),
}));
vi.mock("../ipc", () => ipc);

const localPlan = {
  snapshot_id: "snapshot-1",
  summary: "2 changed files across 1 proposed commit group, with +8 and -2 lines.",
  warnings: [{
    code: "tests_not_changed",
    severity: "info" as const,
    message: "Source files changed without nearby test-file changes.",
    paths: [],
  }],
  groups: [{
    id: "area-app-frontend",
    title: "App frontend",
    rationale: "Keeps frontend changes together.",
    commit_message: "feat(changes): add planner",
    files: [
      { path: "app/src/App.tsx", state: "modified", staged: false, additions: 5, deletions: 1 },
      { path: "app/src/ipc.ts", state: "modified", staged: false, additions: 3, deletions: 1 },
    ],
    executable: true,
    blocked_reason: null,
  }],
  enhanced: false,
  usage: { input_tokens: 0, output_tokens: 0, tool_calls: 0 },
  model_id: "",
  provider_attempts: 0,
};

describe("ChangePlanWorkspace", () => {
  beforeEach(() => {
    localStorage.clear();
    setLang("en");
    vi.clearAllMocks();
    ipc.analyzeChangePlan.mockResolvedValue(localPlan);
    ipc.cancelChangePlan.mockResolvedValue(undefined);
    ipc.commitChangeGroup.mockResolvedValue({ sha: "abc123456789" });
    ipc.credentialStatus.mockResolvedValue(true);
    ipc.listReviewModels.mockResolvedValue([{
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
    }]);
    ipc.onAgentEvent.mockResolvedValue(vi.fn());
  });

  it("runs locally by default and requires explicit confirmation before committing", async () => {
    const user = userEvent.setup();
    const onCommitted = vi.fn();
    render(<ChangePlanWorkspace repo="D:/repo" onClose={vi.fn()} onCommitted={onCommitted} />);

    expect(await screen.findByText("App frontend")).toBeInTheDocument();
    expect(ipc.analyzeChangePlan).toHaveBeenCalledWith(expect.objectContaining({
      repo_path: "D:/repo",
      model_id: null,
    }));
    const commit = screen.getByRole("button", { name: "Stage and commit" });
    expect(commit).toBeDisabled();

    await user.click(screen.getByRole("checkbox", { name: /I reviewed these 2 files/ }));
    expect(commit).toBeEnabled();
    await user.click(commit);

    await waitFor(() => expect(ipc.commitChangeGroup).toHaveBeenCalledWith({
      run_id: expect.any(String),
      repo_path: "D:/repo",
      snapshot_id: "snapshot-1",
      group_id: "area-app-frontend",
      commit_message: "feat(changes): add planner",
      confirmed: true,
    }));
    expect(onCommitted).toHaveBeenCalledTimes(1);
    expect(ipc.analyzeChangePlan).toHaveBeenLastCalledWith(expect.objectContaining({ model_id: null }));
  });

  it("only sends the bounded diff to a model after the user opts in", async () => {
    const user = userEvent.setup();
    render(<ChangePlanWorkspace repo="D:/repo" onClose={vi.fn()} onCommitted={vi.fn()} onConfigureCredential={vi.fn()} />);
    await screen.findByText("App frontend");

    await user.click(screen.getByRole("button", { name: "AI enhance" }));
    const enhance = screen.getByRole("button", { name: "Replan with model" });
    expect(enhance).toBeDisabled();
    await user.click(screen.getByRole("checkbox", { name: /I agree to send/ }));
    expect(enhance).toBeEnabled();
    await user.click(enhance);

    await waitFor(() => expect(ipc.analyzeChangePlan).toHaveBeenCalledWith(expect.objectContaining({
      repo_path: "D:/repo",
      model_id: "gpt-5.4-mini",
    })));
  });
});
