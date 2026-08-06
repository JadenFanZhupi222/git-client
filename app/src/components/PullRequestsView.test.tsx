import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { setLang } from "../lib/i18n";
import { ToastProvider } from "./Toast";
import { PullRequestsView } from "./PullRequestsView";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("../ipc", () => ({
  hasGithubToken: vi.fn().mockResolvedValue(true),
  getGithubToken: vi.fn().mockResolvedValue("ghp_secret"),
}));
vi.mock("./PrReviewWorkspace", () => ({
  PrReviewWorkspace: () => <div role="dialog" aria-label="AI Review workspace" />,
}));

const remotes = [{ name: "origin", url: "https://github.com/acme/rocket.git" }];

function githubResponse(url: string): Response {
  if (url.includes("/pulls?")) {
    return new Response(JSON.stringify([
      { number: 12, title: "Agent review", html_url: "https://github.com/acme/rocket/pull/12", user: { login: "lin" }, head: { ref: "feature/agent", sha: "aaa" }, base: { ref: "main" } },
      { number: 8, title: "Polish history", html_url: "https://github.com/acme/rocket/pull/8", user: { login: "mei" }, head: { ref: "feature/history", sha: "bbb" }, base: { ref: "main" } },
    ]));
  }
  if (/\/pulls\/\d+$/.test(url)) {
    const number = Number(url.split("/").pop());
    return new Response(JSON.stringify({ number, title: number === 12 ? "Agent review" : "Polish history", html_url: `https://github.com/acme/rocket/pull/${number}`, mergeable: true, mergeable_state: "clean", comments: 0, review_comments: 0, commits: 2, changed_files: 3, additions: 8, deletions: 2, user: { login: "lin" }, head: { ref: number === 12 ? "feature/agent" : "feature/history", sha: number === 12 ? "aaa" : "bbb" }, base: { ref: "main" } }));
  }
  if (url.includes("/status")) return new Response(JSON.stringify({ state: "success", total_count: 0, statuses: [] }));
  if (url.includes("/check-runs")) return new Response(JSON.stringify({ total_count: 0, check_runs: [] }));
  return new Response(JSON.stringify([]));
}

describe("PullRequestsView", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.clearAllMocks();
    setLang("en");
  });

  it("shows all open PRs and can narrow the list to the current branch", async () => {
    const fetchMock = vi.fn((input: string | URL | Request) =>
      Promise.resolve(githubResponse(String(input))),
    );
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();

    render(
      <ToastProvider>
        <PullRequestsView
          remotes={remotes}
          branch="feature/agent"
          preferredRemote="origin"
          onCreatePullRequest={vi.fn()}
          onConfigureToken={vi.fn()}
        />
      </ToastProvider>,
    );

    expect(await screen.findByText("Agent review")).toBeInTheDocument();
    expect(screen.getByText("Polish history")).toBeInTheDocument();
    expect(fetchMock).toHaveBeenCalledWith(
      "https://api.github.com/repos/acme/rocket/pulls?state=open&per_page=50",
      expect.any(Object),
    );

    await user.click(screen.getByRole("button", { name: "Current branch" }));
    expect(screen.getAllByText("Agent review").length).toBeGreaterThan(0);
    expect(screen.queryByText("Polish history")).not.toBeInTheDocument();
    expect(screen.getByText("Current branch", { selector: "span" })).toBeInTheDocument();

    await waitFor(() => expect(screen.getByRole("button", { name: "AI Review" })).toBeEnabled());
  });
});
