import { describe, expect, it, vi } from "vitest";
import {
  buildGitlabMergeRequestsApiUrl,
  fetchGitlabMergeRequests,
  gitlabApiErrorMessage,
} from "./gitlab";
import type { HostingRemote } from "./hosting";

const gitlabRemote: HostingRemote = {
  provider: "gitlab",
  owner: "team/subgroup",
  repo: "project",
  webBaseUrl: "https://gitlab.com/team/subgroup/project",
};

describe("buildGitlabMergeRequestsApiUrl", () => {
  it("builds a GitLab merge requests API URL filtered to the current branch", () => {
    expect(buildGitlabMergeRequestsApiUrl(gitlabRemote, "feature/api")).toBe(
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/merge_requests?state=opened&source_branch=feature%2Fapi&per_page=20",
    );
  });

  it("returns null for non-GitLab remotes or missing branches", () => {
    expect(
      buildGitlabMergeRequestsApiUrl(
        { ...gitlabRemote, provider: "github" },
        "feature/api",
      ),
    ).toBeNull();
    expect(buildGitlabMergeRequestsApiUrl(gitlabRemote, null)).toBeNull();
  });
});

describe("fetchGitlabMergeRequests", () => {
  it("adds PRIVATE-TOKEN and maps merge request summaries", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify([
          {
            iid: 12,
            title: "Ship GitLab panel",
            web_url:
              "https://gitlab.com/team/subgroup/project/-/merge_requests/12",
            draft: false,
            work_in_progress: false,
            author: { username: "dev-a" },
            source_branch: "feature/api",
            target_branch: "main",
            merge_status: "can_be_merged",
            detailed_merge_status: "mergeable",
          },
        ]),
        { status: 200 },
      ),
    );

    const mrs = await fetchGitlabMergeRequests(
      gitlabRemote,
      "feature/api",
      "glpat_secret",
      fetchMock,
    );

    expect(fetchMock).toHaveBeenCalledWith(
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/merge_requests?state=opened&source_branch=feature%2Fapi&per_page=20",
      {
        headers: {
          Accept: "application/json",
          "PRIVATE-TOKEN": "glpat_secret",
        },
      },
    );
    expect(mrs).toEqual([
      {
        iid: 12,
        title: "Ship GitLab panel",
        url: "https://gitlab.com/team/subgroup/project/-/merge_requests/12",
        draft: false,
        author: "dev-a",
        sourceBranch: "feature/api",
        targetBranch: "main",
        mergeStatus: "can_be_merged",
        detailedMergeStatus: "mergeable",
      },
    ]);
  });

  it("omits PRIVATE-TOKEN for public unauthenticated requests", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response(JSON.stringify([]), { status: 200 }));

    await fetchGitlabMergeRequests(
      gitlabRemote,
      "feature/api",
      null,
      fetchMock,
    );

    expect(fetchMock).toHaveBeenCalledWith(expect.any(String), {
      headers: {
        Accept: "application/json",
      },
    });
  });
});

describe("gitlabApiErrorMessage", () => {
  it("returns actionable messages for common GitLab API failures", () => {
    expect(gitlabApiErrorMessage(401)).toBe("GitLab token 无效或已过期");
    expect(gitlabApiErrorMessage(403)).toBe(
      "GitLab API 访问被拒绝，可能是权限不足",
    );
    expect(gitlabApiErrorMessage(404)).toBe(
      "GitLab 项目不存在，或当前 token 没有访问权限",
    );
  });
});
