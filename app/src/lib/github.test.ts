import { describe, expect, it, vi } from "vitest";
import {
  buildGithubCombinedStatusApiUrl,
  buildGithubCheckRunsApiUrl,
  buildGithubCreatePullApiUrl,
  buildGithubIssueCommentsApiUrl,
  buildGithubMergePullRequestApiUrl,
  buildGithubPullApiUrl,
  buildGithubPullReviewCommentsApiUrl,
  buildGithubPullsApiUrl,
  buildGithubReviewsApiUrl,
  createGithubPullRequestComment,
  createGithubPullRequest,
  fetchGithubPullRequestDetails,
  fetchGithubPullRequests,
  githubApiErrorMessage,
  mergeGithubPullRequest,
} from "./github";
import type { HostingRemote } from "./hosting";

const githubRemote: HostingRemote = {
  provider: "github",
  owner: "acme",
  repo: "project",
  webBaseUrl: "https://github.com/acme/project",
};

describe("buildGithubPullsApiUrl", () => {
  it("builds a GitHub pulls API URL filtered to the current branch", () => {
    expect(buildGithubPullsApiUrl(githubRemote, "feature/api")).toBe(
      "https://api.github.com/repos/acme/project/pulls?state=open&head=acme%3Afeature%2Fapi&per_page=20",
    );
  });

  it("returns null for non-GitHub remotes and lists all open PRs without a branch", () => {
    expect(
      buildGithubPullsApiUrl(
        { ...githubRemote, provider: "gitlab", webBaseUrl: "" },
        "feature/api",
      ),
    ).toBeNull();
    expect(buildGithubPullsApiUrl(githubRemote, null)).toBe(
      "https://api.github.com/repos/acme/project/pulls?state=open&per_page=50",
    );
  });
});

describe("GitHub pull request detail URLs", () => {
  it("builds pull request, review, and status URLs", () => {
    expect(buildGithubCreatePullApiUrl(githubRemote)).toBe(
      "https://api.github.com/repos/acme/project/pulls",
    );
    expect(buildGithubPullApiUrl(githubRemote, 7)).toBe(
      "https://api.github.com/repos/acme/project/pulls/7",
    );
    expect(buildGithubReviewsApiUrl(githubRemote, 7)).toBe(
      "https://api.github.com/repos/acme/project/pulls/7/reviews?per_page=30",
    );
    expect(buildGithubCombinedStatusApiUrl(githubRemote, "abc123")).toBe(
      "https://api.github.com/repos/acme/project/commits/abc123/status",
    );
    expect(buildGithubCheckRunsApiUrl(githubRemote, "abc123")).toBe(
      "https://api.github.com/repos/acme/project/commits/abc123/check-runs?per_page=20",
    );
    expect(buildGithubIssueCommentsApiUrl(githubRemote, 7)).toBe(
      "https://api.github.com/repos/acme/project/issues/7/comments",
    );
    expect(buildGithubPullReviewCommentsApiUrl(githubRemote, 7)).toBe(
      "https://api.github.com/repos/acme/project/pulls/7/comments?per_page=20",
    );
    expect(buildGithubMergePullRequestApiUrl(githubRemote, 7)).toBe(
      "https://api.github.com/repos/acme/project/pulls/7/merge",
    );
  });
});

describe("mergeGithubPullRequest", () => {
  it("merges a pull request with the selected method and expected head sha", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          sha: "merge123",
          merged: true,
          message: "Pull Request successfully merged",
        }),
        { status: 200 },
      ),
    );

    const result = await mergeGithubPullRequest(
      githubRemote,
      7,
      { method: "squash", headSha: "abc123" },
      "ghp_secret",
      fetchMock,
    );

    expect(fetchMock).toHaveBeenCalledWith(
      "https://api.github.com/repos/acme/project/pulls/7/merge",
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
    expect(result).toEqual({
      sha: "merge123",
      merged: true,
      message: "Pull Request successfully merged",
    });
  });

  it("requires token and head sha before merging", async () => {
    await expect(
      mergeGithubPullRequest(
        githubRemote,
        7,
        { method: "merge", headSha: "abc123" },
        " ",
      ),
    ).rejects.toThrow("GitHub token is required");
    await expect(
      mergeGithubPullRequest(
        githubRemote,
        7,
        { method: "merge", headSha: " " },
        "ghp_secret",
      ),
    ).rejects.toThrow("PR head SHA is required");
  });
});

describe("createGithubPullRequestComment", () => {
  it("posts a pull request issue comment and maps the created comment", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          id: 301,
          body: "Please re-run the failed check.",
          html_url: "https://github.com/acme/project/pull/7#issuecomment-301",
          user: { login: "me" },
          created_at: "2026-07-03T10:00:00Z",
          updated_at: "2026-07-03T10:00:00Z",
        }),
        { status: 201 },
      ),
    );

    const comment = await createGithubPullRequestComment(
      githubRemote,
      7,
      " Please re-run the failed check. ",
      "ghp_secret",
      fetchMock,
    );

    expect(fetchMock).toHaveBeenCalledWith(
      "https://api.github.com/repos/acme/project/issues/7/comments",
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
    expect(comment).toEqual({
      id: 301,
      body: "Please re-run the failed check.",
      url: "https://github.com/acme/project/pull/7#issuecomment-301",
      author: "me",
      createdAt: "2026-07-03T10:00:00Z",
      updatedAt: "2026-07-03T10:00:00Z",
    });
  });

  it("requires body and token", async () => {
    await expect(
      createGithubPullRequestComment(githubRemote, 7, " ", "ghp_secret"),
    ).rejects.toThrow("PR comment cannot be empty");
    await expect(
      createGithubPullRequestComment(
        githubRemote,
        7,
        "Please re-run the failed check.",
        " ",
      ),
    ).rejects.toThrow("GitHub token is required");
  });
});

describe("createGithubPullRequest", () => {
  it("posts a create pull request payload and returns the created summary", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          number: 8,
          title: "Add GitHub API creation",
          html_url: "https://github.com/acme/project/pull/8",
          draft: true,
          user: { login: "octo" },
          head: { ref: "feature/create-pr", sha: "def456" },
          base: { ref: "main" },
        }),
        { status: 201 },
      ),
    );

    const pr = await createGithubPullRequest(
      githubRemote,
      {
        title: " Add GitHub API creation ",
        body: "Created from the desktop client",
        head: "feature/create-pr",
        base: "main",
        draft: true,
      },
      "ghp_secret",
      fetchMock,
    );

    expect(fetchMock).toHaveBeenCalledWith(
      "https://api.github.com/repos/acme/project/pulls",
      {
        method: "POST",
        headers: {
          Accept: "application/vnd.github+json",
          Authorization: "Bearer ghp_secret",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          title: "Add GitHub API creation",
          body: "Created from the desktop client",
          head: "feature/create-pr",
          base: "main",
          draft: true,
        }),
      },
    );
    expect(pr).toEqual({
      number: 8,
      title: "Add GitHub API creation",
      url: "https://github.com/acme/project/pull/8",
      draft: true,
      author: "octo",
      headRef: "feature/create-pr",
      headSha: "def456",
      baseRef: "main",
    });
  });

  it("rejects create pull requests without a title, head, or base", async () => {
    await expect(
      createGithubPullRequest(
        githubRemote,
        { title: " ", head: "feature", base: "main", body: "", draft: false },
        "token",
      ),
    ).rejects.toThrow("PR 标题不能为空");
    await expect(
      createGithubPullRequest(
        githubRemote,
        { title: "Title", head: "", base: "main", body: "", draft: false },
        "token",
      ),
    ).rejects.toThrow("PR source branch 不能为空");
    await expect(
      createGithubPullRequest(
        githubRemote,
        { title: "Title", head: "feature", base: "", body: "", draft: false },
        "token",
      ),
    ).rejects.toThrow("PR base branch 不能为空");
  });
});

describe("fetchGithubPullRequests", () => {
  it("loads all open pull requests when no branch filter is supplied", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response(JSON.stringify([]), { status: 200 }));

    await fetchGithubPullRequests(githubRemote, null, null, fetchMock);

    expect(fetchMock).toHaveBeenCalledWith(
      "https://api.github.com/repos/acme/project/pulls?state=open&per_page=50",
      { headers: { Accept: "application/vnd.github+json" } },
    );
  });

  it("adds GitHub headers and bearer token when loading pull requests", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify([
          {
            number: 7,
            title: "Ship review panel",
            html_url: "https://github.com/acme/project/pull/7",
            draft: false,
            user: { login: "octo" },
            head: { ref: "feature/api", label: "acme:feature/api" },
            base: { ref: "main" },
          },
        ]),
        { status: 200 },
      ),
    );

    const prs = await fetchGithubPullRequests(
      githubRemote,
      "feature/api",
      "ghp_secret",
      fetchMock,
    );

    expect(fetchMock).toHaveBeenCalledWith(
      "https://api.github.com/repos/acme/project/pulls?state=open&head=acme%3Afeature%2Fapi&per_page=20",
      {
        headers: {
          Accept: "application/vnd.github+json",
          Authorization: "Bearer ghp_secret",
        },
      },
    );
    expect(prs).toEqual([
      {
        number: 7,
        title: "Ship review panel",
        url: "https://github.com/acme/project/pull/7",
        draft: false,
        author: "octo",
        headRef: "feature/api",
        headSha: "",
        baseRef: "main",
      },
    ]);
  });

  it("does not send authorization for public unauthenticated requests", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response(JSON.stringify([]), { status: 200 }));

    await fetchGithubPullRequests(githubRemote, "feature/api", null, fetchMock);

    expect(fetchMock).toHaveBeenCalledWith(expect.any(String), {
      headers: {
        Accept: "application/vnd.github+json",
      },
    });
  });
});

describe("fetchGithubPullRequestDetails", () => {
  it("loads pull request summary, reviews, combined status, and recent comments", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            number: 7,
            title: "Ship review panel",
            html_url: "https://github.com/acme/project/pull/7",
            draft: true,
            mergeable: false,
            mergeable_state: "dirty",
            comments: 2,
            review_comments: 3,
            commits: 4,
            changed_files: 5,
            additions: 120,
            deletions: 40,
            user: { login: "octo" },
            head: { ref: "feature/api", sha: "abc123" },
            base: { ref: "main" },
          }),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify([
            { state: "APPROVED", user: { login: "reviewer-a" } },
            { state: "CHANGES_REQUESTED", user: { login: "reviewer-b" } },
          ]),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            state: "failure",
            total_count: 2,
            statuses: [
              {
                context: "ci/test",
                state: "success",
                target_url: "https://ci",
              },
              { context: "lint", state: "failure", target_url: null },
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
                html_url: "https://github.com/acme/project/actions/runs/501",
                started_at: "2026-07-03T10:00:00Z",
                completed_at: "2026-07-03T10:05:00Z",
                app: { slug: "github-actions" },
              },
              {
                id: 502,
                name: "test / windows",
                status: "completed",
                conclusion: "failure",
                html_url: "https://github.com/acme/project/actions/runs/502",
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
              id: 301,
              body: "Please re-run the failed check.",
              html_url: "https://github.com/acme/project/pull/7#issuecomment-301",
              user: { login: "reviewer-a" },
              created_at: "2026-07-03T10:00:00Z",
              updated_at: "2026-07-03T10:01:00Z",
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
              html_url: "https://github.com/acme/project/pull/7#discussion_r401",
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
      );

    const detail = await fetchGithubPullRequestDetails(
      githubRemote,
      7,
      "ghp_secret",
      fetchMock,
    );

    expect(fetchMock).toHaveBeenNthCalledWith(
      1,
      "https://api.github.com/repos/acme/project/pulls/7",
      expect.any(Object),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      "https://api.github.com/repos/acme/project/pulls/7/reviews?per_page=30",
      expect.any(Object),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      3,
      "https://api.github.com/repos/acme/project/commits/abc123/status",
      expect.any(Object),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      4,
      "https://api.github.com/repos/acme/project/commits/abc123/check-runs?per_page=20",
      expect.any(Object),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      5,
      "https://api.github.com/repos/acme/project/issues/7/comments?per_page=20",
      expect.any(Object),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      6,
      "https://api.github.com/repos/acme/project/pulls/7/comments?per_page=20",
      expect.any(Object),
    );
    expect(detail).toEqual({
      number: 7,
      title: "Ship review panel",
      url: "https://github.com/acme/project/pull/7",
      draft: true,
      author: "octo",
      headRef: "feature/api",
      headSha: "abc123",
      baseRef: "main",
      mergeable: false,
      mergeableState: "dirty",
      comments: 2,
      reviewComments: 3,
      commits: 4,
      changedFiles: 5,
      additions: 120,
      deletions: 40,
      reviews: [
        { state: "APPROVED", author: "reviewer-a" },
        { state: "CHANGES_REQUESTED", author: "reviewer-b" },
      ],
      combinedStatus: {
        state: "failure",
        totalCount: 2,
        statuses: [
          { context: "ci/test", state: "success", targetUrl: "https://ci" },
          { context: "lint", state: "failure", targetUrl: null },
        ],
      },
      checkRuns: [
        {
          id: 501,
          name: "build / linux",
          status: "completed",
          conclusion: "success",
          url: "https://github.com/acme/project/actions/runs/501",
          app: "github-actions",
          startedAt: "2026-07-03T10:00:00Z",
          completedAt: "2026-07-03T10:05:00Z",
        },
        {
          id: 502,
          name: "test / windows",
          status: "completed",
          conclusion: "failure",
          url: "https://github.com/acme/project/actions/runs/502",
          app: "github-actions",
          startedAt: "2026-07-03T10:01:00Z",
          completedAt: "2026-07-03T10:06:00Z",
        },
      ],
      recentComments: [
        {
          id: 301,
          body: "Please re-run the failed check.",
          url: "https://github.com/acme/project/pull/7#issuecomment-301",
          author: "reviewer-a",
          createdAt: "2026-07-03T10:00:00Z",
          updatedAt: "2026-07-03T10:01:00Z",
        },
      ],
      reviewThreads: [
        {
          id: 401,
          body: "This branch should handle null refs.",
          url: "https://github.com/acme/project/pull/7#discussion_r401",
          author: "reviewer-b",
          path: "src/git.ts",
          line: 42,
          originalLine: 41,
          diffHunk: "@@ -39,7 +39,7 @@",
          createdAt: "2026-07-03T11:00:00Z",
          updatedAt: "2026-07-03T11:02:00Z",
        },
      ],
    });
  });

  it("keeps PR details available when fine-grained PAT cannot read check runs", async () => {
    const fetchMock = vi.fn(async (input: string | URL | Request) => {
      const url = String(input);
      if (url.endsWith("/pulls/7")) {
        return new Response(JSON.stringify({
          number: 7,
          title: "Review without checks",
          html_url: "https://github.com/acme/project/pull/7",
          user: { login: "octo" },
          head: { ref: "feature/api", sha: "abc123" },
          base: { ref: "main" },
        }));
      }
      if (url.includes("/check-runs")) {
        return new Response(JSON.stringify({ message: "Resource not accessible" }), { status: 403 });
      }
      if (url.endsWith("/status")) {
        return new Response(JSON.stringify({ state: "pending", total_count: 0, statuses: [] }));
      }
      return new Response(JSON.stringify([]));
    });

    const detail = await fetchGithubPullRequestDetails(
      githubRemote,
      7,
      "github_pat_secret",
      fetchMock,
    );

    expect(detail.title).toBe("Review without checks");
    expect(detail.checkRuns).toEqual([]);
    expect(detail.combinedStatus?.state).toBe("pending");
  });
});

describe("githubApiErrorMessage", () => {
  it("returns actionable messages for common GitHub API failures", () => {
    expect(githubApiErrorMessage(401)).toBe("GitHub token 无效或已过期");
    expect(githubApiErrorMessage(403)).toBe(
      "GitHub API 访问被拒绝，可能是权限不足或触发限流",
    );
    expect(githubApiErrorMessage(404)).toBe(
      "GitHub 仓库不存在，或当前 token 没有访问权限",
    );
  });
});
