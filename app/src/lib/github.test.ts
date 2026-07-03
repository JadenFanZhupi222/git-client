import { describe, expect, it, vi } from "vitest";
import {
  buildGithubCombinedStatusApiUrl,
  buildGithubCreatePullApiUrl,
  buildGithubIssueCommentsApiUrl,
  buildGithubPullApiUrl,
  buildGithubPullsApiUrl,
  buildGithubReviewsApiUrl,
  createGithubPullRequestComment,
  createGithubPullRequest,
  fetchGithubPullRequestDetails,
  fetchGithubPullRequests,
  githubApiErrorMessage,
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

  it("returns null for non-GitHub remotes or missing branches", () => {
    expect(
      buildGithubPullsApiUrl(
        { ...githubRemote, provider: "gitlab", webBaseUrl: "" },
        "feature/api",
      ),
    ).toBeNull();
    expect(buildGithubPullsApiUrl(githubRemote, null)).toBeNull();
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
    expect(buildGithubIssueCommentsApiUrl(githubRemote, 7)).toBe(
      "https://api.github.com/repos/acme/project/issues/7/comments",
    );
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
  it("loads pull request summary, reviews, and combined status", async () => {
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
    });
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
