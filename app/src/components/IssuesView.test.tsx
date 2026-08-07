import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { setLang } from "../lib/i18n";
import { IssuesView } from "./IssuesView";

const ipc = vi.hoisted(() => ({
  listGithubIssues: vi.fn(),
  getGithubIssueContext: vi.fn(),
}));

vi.mock("../ipc", () => ipc);
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("./IssueTriageWorkspace", () => ({
  IssueTriageWorkspace: (props: { onConfigureCredential: (kind: "deepseek") => void }) => (
    <div role="dialog" aria-label="AI Issue Triage test">
      <button onClick={() => props.onConfigureCredential("deepseek")}>Configure model</button>
    </div>
  ),
}));

const issues = [
  { number: 7, title: "App crashes on launch", url: "https://github.com/acme/rocket/issues/7", author: "lin", updated_at: "2026-08-07T08:00:00Z", comments: 1, labels: [{ name: "bug", color: "d73a4a" }] },
  { number: 5, title: "Document setup", url: "https://github.com/acme/rocket/issues/5", author: "mei", updated_at: "2026-08-06T08:00:00Z", comments: 0, labels: [{ name: "docs", color: "0075ca" }] },
];

describe("IssuesView", () => {
  beforeEach(() => {
    setLang("en");
    vi.clearAllMocks();
    ipc.listGithubIssues.mockResolvedValue(issues);
    ipc.getGithubIssueContext.mockImplementation(async (target) => ({
      issue: issues.find((issue) => issue.number === target.issue_number),
      body: "Steps to reproduce",
      comments: [{ author: "mei", body: "Confirmed", created_at: "2026-08-07T09:00:00Z", updated_at: "2026-08-07T09:00:00Z" }],
      comments_truncated: false,
      available_labels: issues.flatMap((issue) => issue.labels),
      similar_issues: [],
      snapshot: { updated_at: "2026-08-07T08:00:00Z", comments: 1 },
    }));
  });

  it("loads issues through secure IPC, filters them, and opens read-only triage", async () => {
    const user = userEvent.setup();
    const onConfigureCredential = vi.fn();
    render(<IssuesView remotes={[{ name: "origin", url: "https://github.com/acme/rocket.git" }]} preferredRemote="origin" onConfigureCredential={onConfigureCredential} />);

    expect(await screen.findByRole("heading", { name: "App crashes on launch" })).toBeInTheDocument();
    expect(ipc.listGithubIssues).toHaveBeenCalledWith({ owner: "acme", repo: "rocket" });
    expect(screen.getByText("Steps to reproduce")).toBeInTheDocument();

    await user.type(screen.getByPlaceholderText("Search open issues"), "document");
    expect(screen.queryByText("App crashes on launch", { selector: "button *" })).not.toBeInTheDocument();
    expect(screen.getAllByText("Document setup").length).toBeGreaterThan(0);

    await user.clear(screen.getByPlaceholderText("Search open issues"));
    const contextReadsBeforeTriage = ipc.getGithubIssueContext.mock.calls.length;
    await user.click(await screen.findByRole("button", { name: "AI Triage" }));
    expect(ipc.getGithubIssueContext).toHaveBeenCalledTimes(contextReadsBeforeTriage + 1);
    await user.click(screen.getByRole("button", { name: "Configure model" }));
    expect(onConfigureCredential).toHaveBeenCalledWith("deepseek");
    expect(screen.queryByRole("dialog", { name: "AI Issue Triage test" })).not.toBeInTheDocument();
  });

  it("routes a missing GitHub token to settings", async () => {
    ipc.listGithubIssues.mockRejectedValue({ code: "GITHUB_TOKEN_MISSING", message: "missing", recoverable: true });
    const user = userEvent.setup();
    const onConfigureCredential = vi.fn();
    render(<IssuesView remotes={[{ name: "origin", url: "https://github.com/acme/rocket.git" }]} preferredRemote="origin" onConfigureCredential={onConfigureCredential} />);

    await user.click(await screen.findByRole("button", { name: "Configure GitHub" }));
    expect(onConfigureCredential).toHaveBeenCalledWith("github");
  });
});
