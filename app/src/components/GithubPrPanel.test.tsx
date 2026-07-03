import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { GithubPrPanel } from "./GithubPrPanel";
import { ToastProvider } from "./Toast";

const { openUrl } = vi.hoisted(() => ({
  openUrl: vi.fn<() => Promise<void>>().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl,
}));

vi.mock("../ipc", () => ({
  hasGithubToken: vi.fn().mockResolvedValue(true),
  getGithubToken: vi.fn().mockResolvedValue("ghp_secret"),
}));

const remotes = [
  {
    name: "origin",
    url: "https://github.com/team/project.git",
  },
];

describe("GithubPrPanel", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it("loads pull request details and creates a conversation comment", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify([
            {
              number: 7,
              title: "Ship GitHub comments",
              html_url: "https://github.com/team/project/pull/7",
              user: { login: "dev-a" },
              head: { ref: "feature/comments", sha: "abc123" },
              base: { ref: "main" },
            },
          ]),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            number: 7,
            title: "Ship GitHub comments",
            html_url: "https://github.com/team/project/pull/7",
            mergeable: true,
            mergeable_state: "clean",
            comments: 2,
            review_comments: 1,
            commits: 3,
            changed_files: 4,
            additions: 24,
            deletions: 8,
            user: { login: "dev-a" },
            head: { ref: "feature/comments", sha: "abc123" },
            base: { ref: "main" },
          }),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify([{ state: "APPROVED", user: { login: "reviewer-a" } }]),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            state: "success",
            total_count: 1,
            statuses: [
              {
                context: "ci/test",
                state: "success",
                target_url: "https://ci",
              },
            ],
          }),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            id: 301,
            body: "Please re-run the failed check.",
            html_url: "https://github.com/team/project/pull/7#issuecomment-301",
            user: { login: "me" },
            created_at: "2026-07-03T10:00:00Z",
            updated_at: "2026-07-03T10:00:00Z",
          }),
          { status: 201 },
        ),
      );
    vi.stubGlobal("fetch", fetchMock);

    render(
      <ToastProvider>
        <GithubPrPanel
          remotes={remotes}
          branch="feature/comments"
          preferredRemote="origin"
          onClose={vi.fn()}
          onConfigureToken={vi.fn()}
        />
      </ToastProvider>,
    );

    expect(await screen.findByText("#7 Ship GitHub comments")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Details" }));

    expect(await screen.findByText("mergeable")).toBeInTheDocument();
    expect(screen.getByText("success")).toBeInTheDocument();

    await userEvent.type(
      screen.getByLabelText("New pull request comment"),
      "Please re-run the failed check.",
    );
    await userEvent.click(screen.getByRole("button", { name: "Comment" }));

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith(
        "https://api.github.com/repos/team/project/issues/7/comments",
        {
          method: "POST",
          headers: {
            Accept: "application/vnd.github+json",
            Authorization: "Bearer ghp_secret",
            "Content-Type": "application/json",
          },
          body: JSON.stringify({ body: "Please re-run the failed check." }),
        },
      );
    });
    expect(screen.getByLabelText("New pull request comment")).toHaveValue("");
  });

  it("refreshes pull request results", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify([
            {
              number: 1,
              title: "Old PR title",
              html_url: "https://github.com/team/project/pull/1",
              user: { login: "dev-a" },
              head: { ref: "feature/refresh", sha: "abc123" },
              base: { ref: "main" },
            },
          ]),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify([
            {
              number: 1,
              title: "Updated PR title",
              html_url: "https://github.com/team/project/pull/1",
              user: { login: "dev-a" },
              head: { ref: "feature/refresh", sha: "abc123" },
              base: { ref: "main" },
            },
          ]),
          { status: 200 },
        ),
      );
    vi.stubGlobal("fetch", fetchMock);

    render(
      <ToastProvider>
        <GithubPrPanel
          remotes={remotes}
          branch="feature/refresh"
          preferredRemote="origin"
          onClose={vi.fn()}
          onConfigureToken={vi.fn()}
        />
      </ToastProvider>,
    );

    expect(await screen.findByText("#1 Old PR title")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));

    expect(await screen.findByText("#1 Updated PR title")).toBeInTheDocument();
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });
});
