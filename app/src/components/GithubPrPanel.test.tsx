import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { setLang } from "../lib/i18n";
import { GithubPrPanel } from "./GithubPrPanel";
import { ToastProvider } from "./Toast";

const { openUrl } = vi.hoisted(() => ({
  openUrl: vi.fn<() => Promise<void>>().mockResolvedValue(undefined),
}));
const reviewWorkspace = vi.hoisted(() => ({
  props: null as null | Record<string, unknown>,
  parentHiddenWhenFocused: null as boolean | null,
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl,
}));

vi.mock("../ipc", () => ({
  hasGithubToken: vi.fn().mockResolvedValue(true),
  getGithubToken: vi.fn().mockResolvedValue("ghp_secret"),
}));
vi.mock("./PrReviewWorkspace", async () => {
  const { useLayoutEffect, useRef } = await import("react");
  return {
    PrReviewWorkspace: (props: Record<string, unknown>) => {
      const dialogRef = useRef<HTMLDivElement>(null);
      useLayoutEffect(() => {
        const previous = document.activeElement as HTMLElement | null;
        dialogRef.current?.focus();
        reviewWorkspace.parentHiddenWhenFocused = previous?.closest('[role="dialog"]')?.getAttribute("aria-hidden") === "true";
        (props.onFocusReady as (() => void) | undefined)?.();
        return () => previous?.focus();
      }, []);
      reviewWorkspace.props = props;
      return <div ref={dialogRef} role="dialog" aria-label="AI Review workspace" tabIndex={-1}><button onClick={() => (props.onClose as () => void)()}>Close AI workspace</button></div>;
    },
  };
});

const remotes = [
  {
    name: "origin",
    url: "https://github.com/team/project.git",
  },
];

describe("GithubPrPanel", () => {
  afterEach(() => {
    reviewWorkspace.props = null;
    reviewWorkspace.parentHiddenWhenFocused = null;
    vi.unstubAllGlobals();
    vi.clearAllMocks();
    localStorage.clear();
    setLang("en");
  });

  it("opens AI Review for the exact remote PR and forwards credential routing", async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify([{ number: 17, title: "Review me", html_url: "https://github.com/team/project/pull/17", user: { login: "dev" }, head: { ref: "feature", sha: "abc" }, base: { ref: "main" } }])))
      .mockResolvedValueOnce(new Response(JSON.stringify({ number: 17, title: "Review me", html_url: "https://github.com/team/project/pull/17", mergeable: true, mergeable_state: "clean", comments: 0, review_comments: 0, commits: 1, changed_files: 1, additions: 1, deletions: 0, user: { login: "dev" }, head: { ref: "feature", sha: "abc" }, base: { ref: "main" } })))
      .mockResolvedValueOnce(new Response(JSON.stringify([])))
      .mockResolvedValueOnce(new Response(JSON.stringify({ state: "success", total_count: 0, statuses: [] })))
      .mockResolvedValueOnce(new Response(JSON.stringify({ total_count: 0, check_runs: [] })))
      .mockResolvedValueOnce(new Response(JSON.stringify([])))
      .mockResolvedValueOnce(new Response(JSON.stringify([])));
    vi.stubGlobal("fetch", fetchMock);
    const onConfigureCredential = vi.fn();
    const user = userEvent.setup();
    render(<ToastProvider><GithubPrPanel remotes={remotes} branch="feature" preferredRemote="origin" onClose={vi.fn()} onConfigureToken={vi.fn()} onConfigureCredential={onConfigureCredential} /></ToastProvider>);
    await user.click(await screen.findByRole("button", { name: "Details" }));
    expect(screen.getByRole("dialog", { name: "GitHub pull requests" })).toBeInTheDocument();
    const trigger = await screen.findByRole("button", { name: "AI Review" });
    await user.click(trigger);
    const workspace = await screen.findByRole("dialog", { name: "AI Review workspace" });
    expect(workspace).toHaveFocus();
    expect(reviewWorkspace.parentHiddenWhenFocused).toBe(false);
    expect(screen.queryByRole("dialog", { name: "GitHub pull requests" })).not.toBeInTheDocument();
    expect(document.querySelector('[role="dialog"][aria-label="GitHub pull requests"]')).toHaveAttribute("inert");
    expect(reviewWorkspace.props?.target).toEqual({ owner: "team", repo: "project", pull_number: 17 });
    act(() => {
      (reviewWorkspace.props?.onConfigureCredential as (kind: string) => void)("deepseek");
    });
    expect(onConfigureCredential).toHaveBeenCalledWith("deepseek");
    expect(screen.queryByRole("dialog", { name: "AI Review workspace" })).not.toBeInTheDocument();
    expect(await screen.findByRole("dialog", { name: "GitHub pull requests" })).toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });

  it("renders panel shell copy in Chinese", async () => {
    setLang("zh");
    const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify([]), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);

    render(
      <ToastProvider>
        <GithubPrPanel
          remotes={remotes}
          branch="feature/empty"
          preferredRemote="origin"
          onClose={vi.fn()}
          onConfigureToken={vi.fn()}
        />
      </ToastProvider>,
    );

    expect(screen.getByRole("dialog", { name: "GitHub 拉取请求" })).toBeInTheDocument();
    expect(screen.getByText("GitHub PR")).toBeInTheDocument();
    expect(await screen.findByText("当前分支没有打开的 PR")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "设置 token" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "刷新" })).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "关闭" })).toHaveLength(2);
  });

  it("renders pull request detail copy in Chinese", async () => {
    setLang("zh");
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify([
            {
              number: 7,
              title: "Ship GitHub details",
              html_url: "https://github.com/team/project/pull/7",
              user: { login: "dev-a" },
              head: { ref: "feature/details", sha: "abc123" },
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
            title: "Ship GitHub details",
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
            head: { ref: "feature/details", sha: "abc123" },
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
            statuses: [{ context: "ci/test", state: "success" }],
          }),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            total_count: 1,
            check_runs: [
              {
                id: 501,
                name: "build / linux",
                status: "completed",
                conclusion: "success",
                html_url: "https://github.com/team/project/actions/runs/501",
                app: { slug: "github-actions" },
              },
            ],
          }),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify([
            {
              id: 201,
              body: "Looks good after the retry.",
              html_url: "https://github.com/team/project/pull/7#issuecomment-201",
              user: { login: "reviewer-a" },
              created_at: "2026-07-03T09:00:00Z",
              updated_at: "2026-07-03T09:00:00Z",
            },
          ]),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify([
            {
              id: 401,
              body: "This branch should handle null refs.",
              html_url: "https://github.com/team/project/pull/7#discussion_r401",
              path: "src/git.ts",
              line: 42,
              user: { login: "reviewer-b" },
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
          branch="feature/details"
          preferredRemote="origin"
          onClose={vi.fn()}
          onConfigureToken={vi.fn()}
        />
      </ToastProvider>,
    );

    expect(await screen.findByText("#7 Ship GitHub details")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "详情" }));

    expect(await screen.findByText("合并状态")).toBeInTheDocument();
    expect(screen.getByText("状态")).toBeInTheDocument();
    expect(screen.getByText("评审")).toBeInTheDocument();
    expect(screen.getByText("变更")).toBeInTheDocument();
    expect(screen.getByText("4 个文件")).toBeInTheDocument();
    expect(screen.getByText("3 个提交")).toBeInTheDocument();
    expect(screen.getByText("2 条评论")).toBeInTheDocument();
    expect(screen.getByText("1 条评审评论")).toBeInTheDocument();
    expect(screen.getByText("检查运行")).toBeInTheDocument();
    expect(screen.getByText("最近评论")).toBeInTheDocument();
    expect(screen.getByText("评审讨论")).toBeInTheDocument();
    expect(screen.getByLabelText("合并方式")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "合并" })).toBeInTheDocument();
    expect(screen.getByLabelText("新建拉取请求评论")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("写一条评论")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "评论" })).toBeInTheDocument();
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
            total_count: 2,
            check_runs: [
              {
                id: 501,
                name: "build / linux",
                status: "completed",
                conclusion: "success",
                html_url: "https://github.com/team/project/actions/runs/501",
                started_at: "2026-07-03T10:00:00Z",
                completed_at: "2026-07-03T10:05:00Z",
                app: { slug: "github-actions" },
              },
              {
                id: 502,
                name: "test / windows",
                status: "completed",
                conclusion: "failure",
                html_url: "https://github.com/team/project/actions/runs/502",
                started_at: "2026-07-03T10:01:00Z",
                completed_at: "2026-07-03T10:06:00Z",
                app: { slug: "github-actions" },
              },
            ],
          }),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify([
            {
              id: 201,
              body: "Looks good after the retry.",
              html_url: "https://github.com/team/project/pull/7#issuecomment-201",
              user: { login: "reviewer-a" },
              created_at: "2026-07-03T09:00:00Z",
              updated_at: "2026-07-03T09:00:00Z",
            },
          ]),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify([
            {
              id: 401,
              body: "This branch should handle null refs.",
              html_url: "https://github.com/team/project/pull/7#discussion_r401",
              path: "src/git.ts",
              line: 42,
              original_line: 41,
              diff_hunk: "@@ -39,7 +39,7 @@",
              user: { login: "reviewer-b" },
              created_at: "2026-07-03T11:00:00Z",
              updated_at: "2026-07-03T11:02:00Z",
            },
          ]),
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
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            sha: "merge123",
            merged: true,
            message: "Pull Request successfully merged",
          }),
          { status: 200 },
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
    expect(screen.getAllByText("success").length).toBeGreaterThan(0);
    expect(screen.getByText("build / linux")).toBeInTheDocument();
    expect(screen.getByText("test / windows")).toBeInTheDocument();
    expect(screen.getByText("failure")).toBeInTheDocument();
    expect(screen.getByText("Looks good after the retry.")).toBeInTheDocument();
    expect(screen.getByText("reviewer-a")).toBeInTheDocument();
    expect(screen.getByText("src/git.ts:42")).toBeInTheDocument();
    expect(
      screen.getByText("This branch should handle null refs."),
    ).toBeInTheDocument();

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

    await userEvent.selectOptions(screen.getByLabelText("Merge method"), "squash");
    await userEvent.click(screen.getByRole("button", { name: "Merge" }));

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith(
        "https://api.github.com/repos/team/project/pulls/7/merge",
        {
          method: "PUT",
          headers: {
            Accept: "application/vnd.github+json",
            Authorization: "Bearer ghp_secret",
            "Content-Type": "application/json",
          },
          body: JSON.stringify({
            merge_method: "squash",
            sha: "abc123",
          }),
        },
      );
    });
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
