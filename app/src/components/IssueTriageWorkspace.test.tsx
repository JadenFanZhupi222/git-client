import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { setLang } from "../lib/i18n";
import { IssueTriageWorkspace } from "./IssueTriageWorkspace";

const ipc = vi.hoisted(() => ({
  listReviewModels: vi.fn(),
  onReviewProgress: vi.fn(),
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
});
