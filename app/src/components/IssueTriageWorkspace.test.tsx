import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { setLang } from "../lib/i18n";
import { IssueTriageWorkspace } from "./IssueTriageWorkspace";

const ipc = vi.hoisted(() => ({
  listReviewModels: vi.fn(),
  onReviewProgress: vi.fn(),
  publishIssueTriage: vi.fn(),
  startIssueTriage: vi.fn(),
  cancelIssueTriage: vi.fn(),
}));
vi.mock("../ipc", () => ipc);

const target = { owner: "acme", repo: "rocket", issue_number: 7 };
const context = {
  issue: { number: 7, title: "App crashes", url: "https://github.com/acme/rocket/issues/7", author: "lin", updated_at: "2026-08-07T08:00:00Z", comments: 1, labels: [{ name: "bug", color: "d73a4a" }] },
  body: "The app crashes after clicking Open.",
  comments: [{ author: "mei", body: "Confirmed", created_at: "2026-08-07T09:00:00Z", updated_at: "2026-08-07T09:00:00Z" }],
  comments_truncated: false,
  available_labels: [{ name: "bug", color: "d73a4a" }, { name: "priority:high", color: "b60205" }],
  similar_issues: [{ number: 3, title: "Startup crash", url: "https://github.com/acme/rocket/issues/3", author: "mei", updated_at: "2026-08-01T08:00:00Z", comments: 2, labels: [{ name: "bug", color: "d73a4a" }] }],
  snapshot: { updated_at: "2026-08-07T08:00:00Z", comments: 1 },
};
const result = {
  run_id: "server-run",
  snapshot: context.snapshot,
  comments_analyzed: 1,
  comments_truncated: false,
  proposal: {
    summary: "A reproducible launch crash.",
    category: "bug",
    priority: "high",
    confidence: 0.91,
    suggested_labels: ["bug", "priority:high"],
    suspected_duplicate_numbers: [3],
    suggested_reply: "Thanks. Could you share the app version?",
    rationale: ["The report includes reproduction steps."],
  },
  usage: { input_tokens: 420, output_tokens: 90, tool_calls: 0 },
  model_id: "deepseek-v4-flash",
  duration_ms: 780,
  diagnostic_id: "diag-fedcba9876543210",
  provider_attempts: 1,
};

describe("IssueTriageWorkspace", () => {
  beforeEach(() => {
    localStorage.clear();
    setLang("en");
    vi.clearAllMocks();
    ipc.listReviewModels.mockResolvedValue([{ id: "deepseek-v4-flash", label: "DeepSeek V4 Flash", provider: "DeepSeek" }]);
    ipc.onReviewProgress.mockResolvedValue(vi.fn());
    ipc.startIssueTriage.mockResolvedValue(result);
    ipc.cancelIssueTriage.mockResolvedValue(undefined);
    ipc.publishIssueTriage.mockResolvedValue({
      publish_id: "publish-1",
      snapshot: { updated_at: "2026-08-07T10:00:00Z", comments: 2 },
      actions: [
        { action_id: "label:priority:high", kind: "label", label: "priority:high", status: "applied", error_code: null },
        { action_id: "comment", kind: "comment", label: null, status: "applied", error_code: null },
      ],
    });
  });

  it("requires consent, pins the issue snapshot, and renders suggestions without a write action", async () => {
    const user = userEvent.setup();
    render(<IssueTriageWorkspace target={target} context={context} onClose={vi.fn()} onConfigureCredential={vi.fn()} />);

    const start = screen.getByRole("button", { name: "Start triage" });
    expect(start).toBeDisabled();
    await user.click(screen.getByRole("checkbox", { name: /I understand/ }));
    await waitFor(() => expect(start).toBeEnabled());
    await user.click(start);

    expect(await screen.findByText("A reproducible launch crash.")).toBeInTheDocument();
    expect(ipc.startIssueTriage).toHaveBeenCalledWith(expect.objectContaining({
      target,
      expected_updated_at: context.snapshot.updated_at,
      expected_comments: context.snapshot.comments,
      model_id: "deepseek-v4-flash",
    }));
    expect(screen.getByText("91%")).toBeInTheDocument();
    expect(screen.getByText("#3")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /submit|publish|apply/i })).not.toBeInTheDocument();
    expect(localStorage.getItem("issue-triage-result-v1:acme/rocket#7")).toContain("A reproducible launch crash");
  });

  it("routes missing model credentials to settings", async () => {
    ipc.startIssueTriage.mockRejectedValue({ code: "AI_KEY_MISSING", message: "missing", recoverable: true });
    const user = userEvent.setup();
    const onConfigureCredential = vi.fn();
    localStorage.setItem("issue-triage-consent-v1", "accepted");
    render(<IssueTriageWorkspace target={target} context={context} onClose={vi.fn()} onConfigureCredential={onConfigureCredential} />);

    await waitFor(() => expect(screen.getByRole("button", { name: "Start triage" })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "Start triage" }));
    await user.click(await screen.findByRole("button", { name: "Open settings" }));
    expect(onConfigureCredential).toHaveBeenCalledWith("deepseek");
  });

  it("shows cancellation as a terminal state with a diagnostic id", async () => {
    let rejectRun!: (reason: unknown) => void;
    ipc.startIssueTriage.mockReturnValue(new Promise((_, reject) => { rejectRun = reject; }));
    const user = userEvent.setup();
    localStorage.setItem("issue-triage-consent-v1", "accepted");
    render(<IssueTriageWorkspace target={target} context={context} onClose={vi.fn()} onConfigureCredential={vi.fn()} />);
    await user.click(await screen.findByRole("button", { name: "Start triage" }));
    await user.click(screen.getByRole("button", { name: "Cancel triage" }));
    rejectRun({ code: "CANCELLED", message: "cancelled", recoverable: true, diagnostic_id: "diag-3333333333333333" });

    expect(await screen.findByText("Triage was cancelled. No result was saved and no GitHub changes were made.")).toBeInTheDocument();
    expect(screen.getByText("Diagnostic diag-3333333333333333")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start triage" })).toBeEnabled();
  });

  it("shows the diagnostic id returned with a triage failure", async () => {
    ipc.startIssueTriage.mockRejectedValue({ code: "RATE_LIMITED", message: "limited", recoverable: true, diagnostic_id: "diag-4444444444444444" });
    const user = userEvent.setup();
    localStorage.setItem("issue-triage-consent-v1", "accepted");
    render(<IssueTriageWorkspace target={target} context={context} onClose={vi.fn()} onConfigureCredential={vi.fn()} />);
    await user.click(await screen.findByRole("button", { name: "Start triage" }));

    expect(await screen.findByText("Diagnostic diag-4444444444444444")).toBeInTheDocument();
  });

  it("discards a cached result when the freshly loaded issue snapshot changed", async () => {
    localStorage.setItem("issue-triage-result-v1:acme/rocket#7", JSON.stringify({
      ...result,
      snapshot: { updated_at: "2026-08-06T08:00:00Z", comments: 0 },
    }));
    localStorage.setItem("issue-triage-consent-v1", "accepted");

    render(<IssueTriageWorkspace target={target} context={context} onClose={vi.fn()} onConfigureCredential={vi.fn()} />);

    expect(await screen.findByText("The issue changed since the saved result was created. The old result was discarded; run triage again.")).toBeInTheDocument();
    expect(screen.queryByText("A reproducible launch crash.")).not.toBeInTheDocument();
    expect(localStorage.getItem("issue-triage-result-v1:acme/rocket#7")).toBeNull();
    expect(screen.getByRole("button", { name: "Start triage" })).toBeEnabled();
  });

  it("restores a pre-diagnostics cache entry without showing invented metadata", async () => {
    const { model_id: _model, duration_ms: _duration, diagnostic_id: _diagnostic, provider_attempts: _attempts, ...legacyResult } = result;
    localStorage.setItem("issue-triage-result-v1:acme/rocket#7", JSON.stringify(legacyResult));

    render(<IssueTriageWorkspace target={target} context={context} onClose={vi.fn()} onConfigureCredential={vi.fn()} />);

    expect(await screen.findByText("A reproducible launch crash.")).toBeInTheDocument();
    expect(screen.queryByText(/Diagnostic diag-/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Estimated cost/)).not.toBeInTheDocument();
  });

  it("publishes only explicitly selected actions after an exact confirmation", async () => {
    const user = userEvent.setup();
    const onPublished = vi.fn();
    localStorage.setItem("issue-triage-consent-v1", "accepted");
    render(<IssueTriageWorkspace target={target} context={context} onClose={vi.fn()} onConfigureCredential={vi.fn()} onPublished={onPublished} />);

    await user.click(await screen.findByRole("button", { name: "Start triage" }));
    await screen.findByText("A reproducible launch crash.");
    const review = screen.getByRole("button", { name: "Review selected actions" });
    expect(review).toBeDisabled();

    await user.click(screen.getByRole("checkbox", { name: "Add label: priority:high" }));
    await user.click(screen.getByRole("checkbox", { name: "Post the reply draft" }));
    await user.click(review);

    expect(screen.getByRole("heading", { name: "Confirm GitHub changes" })).toBeInTheDocument();
    expect(ipc.publishIssueTriage).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Publish selected actions" }));

    expect(await screen.findByRole("heading", { name: "Selected actions published" })).toBeInTheDocument();
    expect(ipc.publishIssueTriage).toHaveBeenCalledWith(expect.objectContaining({
      confirmed: true,
      target,
      expected_snapshot: result.snapshot,
      labels: ["priority:high"],
      reply: result.proposal.suggested_reply,
      publish_id: expect.any(String),
    }));
    expect(onPublished).toHaveBeenCalledTimes(1);
  });

  it("retries a partial result with the same batch id and returned snapshot", async () => {
    const partialSnapshot = { updated_at: "2026-08-07T10:00:00Z", comments: 2 };
    ipc.publishIssueTriage
      .mockResolvedValueOnce({
        publish_id: "ignored-by-client",
        snapshot: partialSnapshot,
        actions: [
          { action_id: "label:priority:high", kind: "label", label: "priority:high", status: "failed", error_code: "AUTH_FAILED" },
          { action_id: "comment", kind: "comment", label: null, status: "applied", error_code: null },
        ],
      })
      .mockResolvedValueOnce({
        publish_id: "ignored-by-client",
        snapshot: { updated_at: "2026-08-07T11:00:00Z", comments: 2 },
        actions: [
          { action_id: "label:priority:high", kind: "label", label: "priority:high", status: "applied", error_code: null },
          { action_id: "comment", kind: "comment", label: null, status: "already_applied", error_code: null },
        ],
      });
    const user = userEvent.setup();
    const onConfigureCredential = vi.fn();
    localStorage.setItem("issue-triage-consent-v1", "accepted");
    render(<IssueTriageWorkspace target={target} context={context} onClose={vi.fn()} onConfigureCredential={onConfigureCredential} />);

    await user.click(await screen.findByRole("button", { name: "Start triage" }));
    await user.click(await screen.findByRole("checkbox", { name: "Add label: priority:high" }));
    await user.click(screen.getByRole("checkbox", { name: "Post the reply draft" }));
    await user.click(screen.getByRole("button", { name: "Review selected actions" }));
    await user.click(screen.getByRole("button", { name: "Publish selected actions" }));
    await screen.findByRole("heading", { name: "Some actions need attention" });
    expect(screen.getByText(/Issues is set to Read and write/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Open GitHub settings" }));
    expect(onConfigureCredential).toHaveBeenCalledWith("github");

    const firstInput = ipc.publishIssueTriage.mock.calls[0][0];
    await user.click(screen.getByRole("button", { name: "Retry failed actions" }));
    await screen.findByRole("heading", { name: "Selected actions published" });
    const secondInput = ipc.publishIssueTriage.mock.calls[1][0];
    expect(secondInput.publish_id).toBe(firstInput.publish_id);
    expect(secondInput.expected_snapshot).toEqual(partialSnapshot);
  });
});
