import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { GitlabMrPanel } from "./GitlabMrPanel";
import { ToastProvider } from "./Toast";

const { openUrl } = vi.hoisted(() => ({
  openUrl: vi.fn<() => Promise<void>>().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl,
}));

vi.mock("../ipc", () => ({
  hasGitlabToken: vi.fn().mockResolvedValue(true),
  getGitlabToken: vi.fn().mockResolvedValue("glpat_secret"),
}));

const remotes = [
  {
    name: "origin",
    url: "https://gitlab.com/team/project.git",
  },
];

describe("GitlabMrPanel", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it("loads and displays merge request details", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify([
            {
              iid: 7,
              title: "Ship GitLab details",
              web_url: "https://gitlab.com/team/project/-/merge_requests/7",
              author: { username: "dev-a" },
              source_branch: "feature/gitlab-details",
              target_branch: "main",
              detailed_merge_status: "checking",
            },
          ]),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            iid: 7,
            title: "Ship GitLab details",
            web_url: "https://gitlab.com/team/project/-/merge_requests/7",
            author: { username: "dev-a" },
            source_branch: "feature/gitlab-details",
            target_branch: "main",
            merge_status: "can_be_merged",
            detailed_merge_status: "mergeable",
            changes_count: "8",
            user_notes_count: 3,
            blocking_discussions_resolved: true,
            has_conflicts: false,
            upvotes: 1,
            downvotes: 0,
          }),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify([
            {
              id: 51,
              status: "success",
              ref: "refs/merge-requests/7/head",
              sha: "def456",
              web_url: "https://gitlab.com/team/project/-/pipelines/51",
            },
          ]),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            approvals_required: 2,
            approvals_left: 1,
            approved: false,
            approved_by: [{ user: { username: "reviewer-a" } }],
            user_has_approved: false,
            user_can_approve: true,
          }),
          { status: 200 },
        ),
      );
    vi.stubGlobal("fetch", fetchMock);

    render(
      <ToastProvider>
        <GitlabMrPanel
          remotes={remotes}
          branch="feature/gitlab-details"
          preferredRemote="origin"
          onClose={vi.fn()}
          onConfigureToken={vi.fn()}
        />
      </ToastProvider>,
    );

    expect(await screen.findByText("!7 Ship GitLab details")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Details" }));

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith(
        "https://gitlab.com/api/v4/projects/team%2Fproject/merge_requests/7",
        expect.any(Object),
      );
    });
    expect(
      await screen.findByText("Pipeline"),
    ).toBeInTheDocument();
    expect(screen.getByText("success")).toBeInTheDocument();
    expect(screen.getByText("mergeable")).toBeInTheDocument();
    expect(screen.getByText("8 changes")).toBeInTheDocument();
    expect(screen.getByText("3 notes")).toBeInTheDocument();
    expect(screen.getByText("Approvals")).toBeInTheDocument();
    expect(screen.getByText("1/2 approved")).toBeInTheDocument();
    expect(screen.getByText("reviewer-a")).toBeInTheDocument();
  });
});
