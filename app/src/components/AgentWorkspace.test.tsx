import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentEventDto, AgentGoalEventDto, AgentGoalSnapshotDto, AgentSessionSnapshotDto, ReviewModelOptionDto } from "../bindings";
import { setLang } from "../lib/i18n";
import { AgentWorkspace } from "./AgentWorkspace";

const ipc = vi.hoisted(() => ({
  cancelAgentGoal: vi.fn(),
  createAgentGoal: vi.fn(),
  credentialStatus: vi.fn(),
  extendAgentBudget: vi.fn(),
  getAgentSession: vi.fn(),
  listenAgentGoalEvents: vi.fn(),
  listReviewModels: vi.fn(),
  onAgentEvent: vi.fn(),
  pauseAgentGoal: vi.fn(),
  resetAgentSession: vi.fn(),
  resolveToolApproval: vi.fn(),
  resumeAgentGoal: vi.fn(),
  steerAgentGoal: vi.fn(),
}));

vi.mock("../ipc", () => ipc);

const emptySession: AgentSessionSnapshotDto = {
  session_id: "repo-1234",
  revision: 0,
  memory_summary: null,
  recent_messages: [],
  active_goal: null,
};

const model: ReviewModelOptionDto = {
  id: "deepseek-v4-flash",
  label: "DeepSeek V4 Flash",
  provider: "DeepSeek",
  provider_id: "deepseek",
  capabilities: {
    context_window_tokens: 1_000_000,
    max_output_tokens: 384_000,
    supports_structured_output: true,
    supports_tool_calling: true,
    reports_usage: true,
  },
  pricing: null,
};

function goal(overrides: Partial<AgentGoalSnapshotDto> = {}): AgentGoalSnapshotDto {
  return {
    goal_id: "goal-test",
    session_id: "repo-1234",
    revision: 0,
    objective: "Fix the parser",
    model_id: model.id,
    status: "queued",
    pause_reason: null,
    block_reason: null,
    usage_by_model: [{
      model_id: model.id,
      currency: "CNY",
      input_tokens: 0,
      cached_input_tokens: 0,
      output_tokens: 0,
      tool_calls: 0,
      spent_micros: 0,
      limit_micros: 1_000_000,
      limit_tokens: null,
    }],
    slice_index: 0,
    steering_count: 0,
    completion_candidate_pending: false,
    final_text: null,
    ...overrides,
  };
}

describe("AgentWorkspace durable Goals", () => {
  let goalEvent: ((event: AgentGoalEventDto) => void) | null;
  let agentEvent: ((event: AgentEventDto) => void) | null;

  beforeEach(() => {
    localStorage.clear();
    setLang("en");
    goalEvent = null;
    agentEvent = null;
    Object.values(ipc).forEach((mock) => mock.mockReset());
    ipc.cancelAgentGoal.mockImplementation(async ({ expected_revision }) => goal({ status: "cancelled", revision: expected_revision + 1 }));
    ipc.createAgentGoal.mockResolvedValue(goal());
    ipc.credentialStatus.mockResolvedValue(true);
    ipc.extendAgentBudget.mockImplementation(async () => goal({ status: "queued", revision: 2 }));
    ipc.getAgentSession.mockResolvedValue(emptySession);
    ipc.listenAgentGoalEvents.mockImplementation(async (handler) => {
      goalEvent = handler;
      return () => undefined;
    });
    ipc.listReviewModels.mockResolvedValue([model]);
    ipc.onAgentEvent.mockImplementation(async (handler) => {
      agentEvent = handler;
      return () => undefined;
    });
    ipc.pauseAgentGoal.mockImplementation(async ({ expected_revision }) => goal({ status: "paused", pause_reason: "user", revision: expected_revision + 1 }));
    ipc.resetAgentSession.mockResolvedValue(emptySession);
    ipc.resumeAgentGoal.mockImplementation(async ({ expected_revision }) => goal({ status: "queued", revision: expected_revision + 1 }));
    ipc.steerAgentGoal.mockImplementation(async ({ expected_revision }) => goal({ status: "running", revision: expected_revision + 1, steering_count: 1 }));
  });

  it("shows only the canonical result after completion verification", async () => {
    render(<AgentWorkspace repo={"D:\\repo"} />);
    await screen.findByText("Start with a concrete task");
    fireEvent.click(screen.getByLabelText(/I agree to send/));
    fireEvent.change(screen.getByLabelText(/Describe what you want/), { target: { value: "Fix the parser" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    expect(await screen.findByText("Fix the parser")).toBeInTheDocument();
    expect(screen.queryByText("Implemented the focused fix.")).not.toBeInTheDocument();
    expect(ipc.createAgentGoal).toHaveBeenCalledWith(expect.objectContaining({
      repo_path: "D:\\repo",
      model_id: model.id,
      message: "Fix the parser",
    }));
    expect(ipc.onAgentEvent).toHaveBeenCalledOnce();
    expect(ipc.onAgentEvent.mock.invocationCallOrder[0]).toBeLessThan(
      ipc.createAgentGoal.mock.invocationCallOrder[0],
    );

    await waitFor(() => expect(agentEvent).not.toBeNull());
    act(() => agentEvent!({
      run_id: ipc.createAgentGoal.mock.calls[0][0].goal_id,
      sequence: 1,
      attempt_id: 1,
      event_type: "output_text_delta",
      provider_id: null,
      model_id: model.id,
      response_id: null,
      delta: "A final answer is being streamed",
      artifact_type: null,
      artifact_field: null,
      artifact_index: null,
      call_id: null,
      tool_name: null,
      usage: null,
      error_code: null,
      will_retry: null,
      approval_id: null,
      risk: null,
      approval_summary: null,
      decision: null,
      tool_outcome: null,
      duration_ms: null,
      content_bytes: null,
      truncated: null,
      tool_error: null,
    }));
    expect(screen.getByLabelText("Streaming agent answer")).toHaveTextContent("A final answer is being streamed");

    const completed = goal({ status: "completed", revision: 4, final_text: "Implemented the focused fix." });
    ipc.getAgentSession.mockResolvedValue({
      ...emptySession,
      revision: 2,
      recent_messages: [
        { role: "user", content: "Fix the parser" },
        { role: "assistant", content: "Implemented the focused fix." },
      ],
      active_goal: completed,
    });
    await waitFor(() => expect(goalEvent).not.toBeNull());
    goalEvent!({
      goal_id: "goal-test",
      revision: 4,
      event_type: "completion_verified",
      status: "completed",
      reason: null,
      model_id: model.id,
      spent_micros: null,
      limit_micros: null,
      receipt_digest: null,
      size_bytes: 28,
    });
    expect(await screen.findByText("Implemented the focused fix.")).toBeInTheDocument();
  });

  it("renders canonical assistant Markdown but keeps user input as plain text", async () => {
    ipc.getAgentSession.mockResolvedValue({
      ...emptySession,
      revision: 2,
      recent_messages: [
        { role: "user", content: "# Keep this literal" },
        { role: "assistant", content: "# Architecture\n\nUse **Goal** checkpoints.\n\n- durable\n- resumable\n\n<script>unsafe()</script>" },
      ],
      active_goal: goal({ status: "completed", revision: 4, final_text: "# Architecture" }),
    });

    const view = render(<AgentWorkspace repo={"D:\\repo"} />);
    expect(await screen.findByRole("heading", { name: "Architecture" })).toBeInTheDocument();
    expect(screen.getByText("Goal").tagName).toBe("STRONG");
    expect(screen.getByText("# Keep this literal")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Keep this literal" })).not.toBeInTheDocument();
    expect(view.container.querySelector("script")).toBeNull();
    expect(screen.queryByText("unsafe()")).not.toBeInTheDocument();
  });

  it("does not cancel background work when the view unmounts", async () => {
    const view = render(<AgentWorkspace repo={"D:\\repo"} />);
    await screen.findByText("Start with a concrete task");
    fireEvent.click(screen.getByLabelText(/I agree to send/));
    fireEvent.change(screen.getByLabelText(/Describe what you want/), { target: { value: "Keep running" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await screen.findByText("Keep running");
    view.unmount();
    expect(ipc.cancelAgentGoal).not.toHaveBeenCalled();
  });

  it("sends new input as steering for the same active Goal", async () => {
    ipc.getAgentSession.mockResolvedValue({ ...emptySession, active_goal: goal({ status: "running", revision: 3 }) });
    render(<AgentWorkspace repo={"D:\\repo"} />);
    await screen.findByText("Fix the parser");
    fireEvent.click(screen.getByLabelText(/I agree to send/));
    fireEvent.change(screen.getByLabelText(/Describe what you want/), { target: { value: "Also inspect tests" } });
    fireEvent.click(screen.getByRole("button", { name: "Steer" }));
    await waitFor(() => expect(ipc.steerAgentGoal).toHaveBeenCalledWith({
      repo_path: "D:\\repo",
      goal_id: "goal-test",
      expected_revision: 3,
      message: "Also inspect tests",
    }));
    expect(ipc.createAgentGoal).not.toHaveBeenCalled();
    expect(await screen.findByText("Also inspect tests")).toBeInTheDocument();
  });

  it("requires explicit resume after restart and exposes budget extension", async () => {
    ipc.getAgentSession.mockResolvedValue({
      ...emptySession,
      active_goal: goal({ status: "paused", pause_reason: "app_restarted", revision: 5 }),
    });
    const restartView = render(<AgentWorkspace repo={"D:\\repo"} />);
    expect(await screen.findByText(/Checkpoint restored/)).toBeInTheDocument();
    expect(ipc.onAgentEvent).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Resume" }));
    await waitFor(() => expect(ipc.resumeAgentGoal).toHaveBeenCalledWith(expect.objectContaining({
      goal_id: "goal-test",
      expected_revision: 5,
    })));
    const latestSubscription = ipc.onAgentEvent.mock.invocationCallOrder[
      ipc.onAgentEvent.mock.invocationCallOrder.length - 1
    ];
    expect(latestSubscription).toBeLessThan(
      ipc.resumeAgentGoal.mock.invocationCallOrder[0],
    );
    restartView.unmount();

    ipc.getAgentSession.mockResolvedValue({
      ...emptySession,
      active_goal: goal({ status: "paused", pause_reason: "budget", revision: 7 }),
    });
    const budgetView = render(<AgentWorkspace repo={"D:\\budget-repo"} />);
    fireEvent.click(await screen.findByRole("button", { name: "Extend budget" }));
    expect(screen.getByRole("dialog", { name: "Extend Goal budget" })).toBeInTheDocument();
    expect(screen.getByText("¥0.0000")).toBeInTheDocument();
    expect(screen.getByText("¥1.0000")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText(/New limit/), { target: { value: "2.5" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply extension" }));
    await waitFor(() => expect(ipc.extendAgentBudget).toHaveBeenCalledWith(expect.objectContaining({
      goal_id: "goal-test",
      expected_revision: 7,
      new_limit_micros: 2_500_000,
    })));
    budgetView.unmount();
  });

  it("explains that provider pauses happen only after automatic retries are exhausted", async () => {
    ipc.getAgentSession.mockResolvedValue({
      ...emptySession,
      active_goal: goal({ status: "paused", pause_reason: "provider_unavailable", revision: 6 }),
    });

    render(<AgentWorkspace repo={"D:\\repo"} />);

    expect(await screen.findByText(/provider remained unavailable after automatic retries/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Resume" })).toBeEnabled();
  });

  it("does not show a stale live stream for a blocked Goal", async () => {
    const queuedSession = {
      ...emptySession,
      active_goal: goal({ status: "queued", revision: 7 }),
    };
    const blockedSession = {
      ...emptySession,
      active_goal: goal({
        status: "blocked",
        block_reason: "completion_candidate_invalid",
        revision: 8,
      }),
    };
    ipc.getAgentSession.mockResolvedValueOnce(queuedSession).mockResolvedValue(blockedSession);
    render(<AgentWorkspace repo={"D:\\repo"} />);
    expect(await screen.findByText(/Connecting to model output/)).toBeInTheDocument();
    await waitFor(() => expect(goalEvent).not.toBeNull());
    goalEvent!({
      goal_id: "goal-test",
      revision: 8,
      event_type: "goal_status_changed",
      status: "blocked",
      reason: "completion_candidate_invalid",
      model_id: model.id,
      spent_micros: null,
      limit_micros: null,
      receipt_digest: null,
      size_bytes: null,
    });
    expect(await screen.findByText(/Blocked: completion candidate invalid/)).toBeInTheDocument();
    expect(screen.queryByText(/Connecting to model output/)).not.toBeInTheDocument();
  });

  it("refuses session reset while a Goal is nonterminal", async () => {
    ipc.getAgentSession.mockResolvedValue({ ...emptySession, active_goal: goal({ status: "running" }) });
    render(<AgentWorkspace repo={"D:\\repo"} />);
    const reset = await screen.findByRole("button", { name: "New session" });
    expect(reset).toBeDisabled();
    fireEvent.click(reset);
    expect(ipc.resetAgentSession).not.toHaveBeenCalled();
  });
});
