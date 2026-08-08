import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { setLang, translate } from "../lib/i18n";
import { GitlabMrPanel } from "./GitlabMrPanel";
import { ToastProvider } from "./Toast";

const { openUrl } = vi.hoisted(() => ({
  openUrl: vi.fn<() => Promise<void>>().mockResolvedValue(undefined),
}));
const reviewWorkspace = vi.hoisted(() => ({ props: null as Record<string, unknown> | null }));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl,
}));

vi.mock("../ipc", () => ({
  hasGitlabToken: vi.fn().mockResolvedValue(true),
  getGitlabToken: vi.fn().mockResolvedValue("glpat_secret"),
}));

vi.mock("./PrReviewWorkspace", () => ({
  PrReviewWorkspace: (props: Record<string, unknown>) => {
    reviewWorkspace.props = props;
    return <div data-testid="gitlab-ai-review-workspace" />;
  },
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
    reviewWorkspace.props = null;
    localStorage.clear();
    setLang("en");
  });

  it("provides complete Chinese copy for merge request details", () => {
    const t = (key: string, params?: Record<string, string | number>) =>
      translate("zh", key as never, params);

    expect(t("gitlabMrDetail.metricPipeline")).toBe("流水线");
    expect(t("gitlabMrDetail.metricApprovals")).toBe("批准");
    expect(t("gitlabMrDetail.approvalProgress", { approved: 1, required: 2 }))
      .toBe("已批准 1/2");
    expect(t("gitlabMrDetail.pipelineJobs")).toBe("流水线任务");
    expect(t("gitlabMrDetail.retryJob", { name: "test-windows" }))
      .toBe("重试 test-windows");
    expect(t("gitlabMrDetail.squash")).toBe("压缩提交");
    expect(t("gitlabMrDetail.commentPlaceholder")).toBe("写一条评论");
    expect(t("gitlabMrDetail.tokenRequired")).toBe("需要 GitLab token");
  });

  it("renders panel shell copy in Chinese", async () => {
    setLang("zh");
    const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify([]), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);

    render(
      <ToastProvider>
        <GitlabMrPanel
          remotes={remotes}
          branch="feature/empty"
          preferredRemote="origin"
          onClose={vi.fn()}
          onConfigureToken={vi.fn()}
        />
      </ToastProvider>,
    );

    expect(screen.getByRole("dialog", { name: "GitLab 合并请求" })).toBeInTheDocument();
    expect(screen.getByText("GitLab MR")).toBeInTheDocument();
    expect(await screen.findByText("当前分支没有打开的 MR")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "设置 token" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "刷新" })).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "关闭" })).toHaveLength(2);
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
          JSON.stringify([
            {
              id: 801,
              name: "build-linux",
              stage: "build",
              status: "success",
              duration: 125.4,
              web_url: "https://gitlab.com/team/project/-/jobs/801",
              started_at: "2026-07-03T10:00:00.000Z",
              finished_at: "2026-07-03T10:02:05.000Z",
            },
            {
              id: 802,
              name: "test-windows",
              stage: "test",
              status: "failed",
              duration: 89,
              web_url: "https://gitlab.com/team/project/-/jobs/802",
              started_at: "2026-07-03T10:01:00.000Z",
              finished_at: "2026-07-03T10:02:29.000Z",
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
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify([
            {
              id: 501,
              body: "Looks good after the pipeline fix.",
              author: { username: "reviewer-a" },
              created_at: "2026-07-01T10:00:00.000Z",
              updated_at: "2026-07-01T10:05:00.000Z",
              system: false,
              internal: false,
            },
          ]),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify([
            {
              id: "discussion-1",
              individual_note: false,
              notes: [
                {
                  id: 701,
                  type: "DiffNote",
                  body: "This branch should handle null refs.",
                  author: { username: "reviewer-b" },
                  created_at: "2026-07-01T11:00:00.000Z",
                  updated_at: "2026-07-01T11:05:00.000Z",
                  system: false,
                  resolvable: true,
                  resolved: false,
                  position: {
                    new_path: "src/git.ts",
                    old_path: "src/git.ts",
                    new_line: 42,
                    old_line: null,
                  },
                },
              ],
            },
          ]),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            id: 802,
            name: "test-windows",
            stage: "test",
            status: "pending",
            duration: null,
            web_url: "https://gitlab.com/team/project/-/jobs/802",
            started_at: null,
            finished_at: null,
          }),
          { status: 201 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            id: 503,
            body: "Please re-run the failed job.",
            author: { username: "me" },
            created_at: "2026-07-02T10:00:00.000Z",
            updated_at: "2026-07-02T10:00:00.000Z",
            system: false,
            internal: false,
          }),
          { status: 201 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            approvals_required: 2,
            approvals_left: 0,
            approved: true,
            approved_by: [
              { user: { username: "reviewer-a" } },
              { user: { username: "me" } },
            ],
            user_has_approved: true,
            user_can_approve: false,
          }),
          { status: 201 },
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
          { status: 201 },
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
    expect(screen.getAllByText("success").length).toBeGreaterThan(0);
    expect(screen.getByText("build-linux")).toBeInTheDocument();
    expect(screen.getByText("test-windows")).toBeInTheDocument();
    expect(screen.getByText("failed")).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Retry test-windows" }),
    );

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith(
        "https://gitlab.com/api/v4/projects/team%2Fproject/jobs/802/retry",
        {
          method: "POST",
          headers: {
            Accept: "application/json",
            "PRIVATE-TOKEN": "glpat_secret",
          },
        },
      );
    });
    expect(await screen.findByText("pending")).toBeInTheDocument();
    expect(screen.getByText("mergeable")).toBeInTheDocument();
    expect(screen.getByText("8 changes")).toBeInTheDocument();
    expect(screen.getByText("3 notes")).toBeInTheDocument();
    expect(screen.getByText("Approvals")).toBeInTheDocument();
    expect(screen.getByText("1/2 approved")).toBeInTheDocument();
    expect(screen.getAllByText("reviewer-a").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("Notes").length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("Looks good after the pipeline fix.")).toBeInTheDocument();
    expect(screen.getByText("src/git.ts:42")).toBeInTheDocument();
    expect(screen.getByText("unresolved")).toBeInTheDocument();
    expect(
      screen.getByText("This branch should handle null refs."),
    ).toBeInTheDocument();

    await userEvent.type(
      screen.getByLabelText("New merge request note"),
      "Please re-run the failed job.",
    );
    await userEvent.click(screen.getByRole("button", { name: "Comment" }));

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith(
        "https://gitlab.com/api/v4/projects/team%2Fproject/merge_requests/7/notes",
        {
          method: "POST",
          headers: {
            Accept: "application/json",
            "Content-Type": "application/json",
            "PRIVATE-TOKEN": "glpat_secret",
          },
          body: JSON.stringify({ body: "Please re-run the failed job." }),
        },
      );
    });
    expect(await screen.findByText("Please re-run the failed job.")).toBeInTheDocument();
    expect(screen.getByLabelText("New merge request note")).toHaveValue("");

    await userEvent.click(screen.getByRole("button", { name: "Approve" }));

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith(
        "https://gitlab.com/api/v4/projects/team%2Fproject/merge_requests/7/approve",
        {
          method: "POST",
          headers: {
            Accept: "application/json",
            "PRIVATE-TOKEN": "glpat_secret",
          },
        },
      );
    });
    expect(await screen.findByText("2/2 approved")).toBeInTheDocument();
    expect(screen.getByText("reviewer-a, me")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Unapprove" }));

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith(
        "https://gitlab.com/api/v4/projects/team%2Fproject/merge_requests/7/unapprove",
        {
          method: "POST",
          headers: {
            Accept: "application/json",
            "PRIVATE-TOKEN": "glpat_secret",
          },
        },
      );
    });
    expect(await screen.findByText("1/2 approved")).toBeInTheDocument();
    expect(screen.getAllByText("reviewer-a").length).toBeGreaterThanOrEqual(1);

    await userEvent.click(screen.getByRole("button", { name: "AI review" }));
    expect(screen.getByTestId("gitlab-ai-review-workspace")).toBeInTheDocument();
    expect(reviewWorkspace.props).toMatchObject({
      platform: "gitlab",
      target: { owner: "team", repo: "project", pull_number: 7 },
    });
  });

  it("refreshes merge request results", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify([
            {
              iid: 7,
              title: "Old MR title",
              web_url: "https://gitlab.com/team/project/-/merge_requests/7",
              author: { username: "dev-a" },
              source_branch: "feature/refresh",
              target_branch: "main",
            },
          ]),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify([
            {
              iid: 7,
              title: "Updated MR title",
              web_url: "https://gitlab.com/team/project/-/merge_requests/7",
              author: { username: "dev-a" },
              source_branch: "feature/refresh",
              target_branch: "main",
            },
          ]),
          { status: 200 },
        ),
      );
    vi.stubGlobal("fetch", fetchMock);

    render(
      <ToastProvider>
        <GitlabMrPanel
          remotes={remotes}
          branch="feature/refresh"
          preferredRemote="origin"
          onClose={vi.fn()}
          onConfigureToken={vi.fn()}
        />
      </ToastProvider>,
    );

    expect(await screen.findByText("!7 Old MR title")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));

    expect(await screen.findByText("!7 Updated MR title")).toBeInTheDocument();
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("merges a ready merge request with squash enabled", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify([
            {
              iid: 9,
              title: "Merge GitLab branch",
              web_url: "https://gitlab.com/team/project/-/merge_requests/9",
              author: { username: "dev-a" },
              source_branch: "feature/merge",
              target_branch: "main",
              detailed_merge_status: "mergeable",
            },
          ]),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            iid: 9,
            title: "Merge GitLab branch",
            web_url: "https://gitlab.com/team/project/-/merge_requests/9",
            author: { username: "dev-a" },
            source_branch: "feature/merge",
            target_branch: "main",
            merge_status: "can_be_merged",
            detailed_merge_status: "mergeable",
            changes_count: "2",
            user_notes_count: 0,
            blocking_discussions_resolved: true,
            has_conflicts: false,
            upvotes: 0,
            downvotes: 0,
            sha: "def456",
          }),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify([
            {
              id: 77,
              status: "success",
              ref: "refs/merge-requests/9/head",
              sha: "def456",
              web_url: "https://gitlab.com/team/project/-/pipelines/77",
            },
          ]),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(new Response(JSON.stringify([]), { status: 200 }))
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            approvals_required: 1,
            approvals_left: 0,
            approved: true,
            approved_by: [{ user: { username: "reviewer-a" } }],
            user_has_approved: false,
            user_can_approve: false,
          }),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(new Response(JSON.stringify([]), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify([]), { status: 200 }))
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            iid: 9,
            title: "Merge GitLab branch",
            web_url: "https://gitlab.com/team/project/-/merge_requests/9",
            author: { username: "dev-a" },
            source_branch: "feature/merge",
            target_branch: "main",
            merge_status: "can_be_merged",
            detailed_merge_status: "not_open",
          }),
          { status: 200 },
        ),
      );
    vi.stubGlobal("fetch", fetchMock);

    render(
      <ToastProvider>
        <GitlabMrPanel
          remotes={remotes}
          branch="feature/merge"
          preferredRemote="origin"
          onClose={vi.fn()}
          onConfigureToken={vi.fn()}
        />
      </ToastProvider>,
    );

    expect(await screen.findByText("!9 Merge GitLab branch")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Details" }));

    await waitFor(() => {
      expect(screen.getAllByText("mergeable").length).toBeGreaterThanOrEqual(1);
    });

    await userEvent.click(screen.getByLabelText("Squash commits"));
    await userEvent.click(screen.getByRole("button", { name: "Merge" }));

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith(
        "https://gitlab.com/api/v4/projects/team%2Fproject/merge_requests/9/merge",
        {
          method: "PUT",
          headers: {
            Accept: "application/json",
            "Content-Type": "application/json",
            "PRIVATE-TOKEN": "glpat_secret",
          },
          body: JSON.stringify({
            sha: "def456",
            squash: true,
          }),
        },
      );
    });
  });
});
