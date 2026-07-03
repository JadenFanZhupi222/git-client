import { describe, expect, it, vi } from "vitest";
import {
  approveGitlabMergeRequest,
  buildGitlabCreateMergeRequestApiUrl,
  buildGitlabMergeRequestApproveApiUrl,
  buildGitlabMergeRequestApprovalsApiUrl,
  buildGitlabMergeRequestApiUrl,
  buildGitlabMergeRequestNoteCreateApiUrl,
  buildGitlabMergeRequestNotesApiUrl,
  buildGitlabPipelineJobsApiUrl,
  buildGitlabRetryJobApiUrl,
  buildGitlabMergeRequestPipelinesApiUrl,
  buildGitlabMergeRequestDiscussionsApiUrl,
  buildGitlabMergeRequestMergeApiUrl,
  buildGitlabMergeRequestUnapproveApiUrl,
  buildGitlabMergeRequestsApiUrl,
  createGitlabMergeRequest,
  createGitlabMergeRequestNote,
  fetchGitlabMergeRequestDetails,
  fetchGitlabMergeRequests,
  gitlabApiErrorMessage,
  mergeGitlabMergeRequest,
  retryGitlabJob,
  unapproveGitlabMergeRequest,
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

describe("buildGitlabCreateMergeRequestApiUrl", () => {
  it("builds a GitLab create merge request API URL", () => {
    expect(buildGitlabCreateMergeRequestApiUrl(gitlabRemote)).toBe(
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/merge_requests",
    );
  });
});

describe("GitLab merge request detail URLs", () => {
  it("builds GitLab detail and pipeline API URLs", () => {
    expect(buildGitlabMergeRequestApiUrl(gitlabRemote, 18)).toBe(
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/merge_requests/18",
    );
    expect(buildGitlabMergeRequestPipelinesApiUrl(gitlabRemote, 18)).toBe(
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/merge_requests/18/pipelines?per_page=1",
    );
    expect(buildGitlabPipelineJobsApiUrl(gitlabRemote, 99)).toBe(
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/pipelines/99/jobs?per_page=20",
    );
    expect(buildGitlabRetryJobApiUrl(gitlabRemote, 802)).toBe(
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/jobs/802/retry",
    );
    expect(buildGitlabMergeRequestApprovalsApiUrl(gitlabRemote, 18)).toBe(
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/merge_requests/18/approvals",
    );
    expect(buildGitlabMergeRequestNotesApiUrl(gitlabRemote, 18)).toBe(
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/merge_requests/18/notes?sort=desc&order_by=updated_at&per_page=5",
    );
    expect(buildGitlabMergeRequestDiscussionsApiUrl(gitlabRemote, 18)).toBe(
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/merge_requests/18/discussions?per_page=20",
    );
    expect(buildGitlabMergeRequestNoteCreateApiUrl(gitlabRemote, 18)).toBe(
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/merge_requests/18/notes",
    );
    expect(buildGitlabMergeRequestApproveApiUrl(gitlabRemote, 18)).toBe(
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/merge_requests/18/approve",
    );
    expect(buildGitlabMergeRequestUnapproveApiUrl(gitlabRemote, 18)).toBe(
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/merge_requests/18/unapprove",
    );
    expect(buildGitlabMergeRequestMergeApiUrl(gitlabRemote, 18)).toBe(
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/merge_requests/18/merge",
    );
  });
});

describe("retryGitlabJob", () => {
  it("retries a GitLab job and maps the updated job", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          id: 802,
          name: "test-windows",
          stage: "test",
          status: "pending",
          duration: null,
          web_url: "https://gitlab.com/team/subgroup/project/-/jobs/802",
          started_at: null,
          finished_at: null,
        }),
        { status: 201 },
      ),
    );

    const job = await retryGitlabJob(
      gitlabRemote,
      802,
      "glpat_secret",
      fetchMock,
    );

    expect(fetchMock).toHaveBeenCalledWith(
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/jobs/802/retry",
      {
        method: "POST",
        headers: {
          Accept: "application/json",
          "PRIVATE-TOKEN": "glpat_secret",
        },
      },
    );
    expect(job).toEqual({
      id: 802,
      name: "test-windows",
      stage: "test",
      status: "pending",
      duration: null,
      url: "https://gitlab.com/team/subgroup/project/-/jobs/802",
      startedAt: "",
      finishedAt: "",
    });
  });

  it("requires a GitLab token before retrying a job", async () => {
    await expect(
      retryGitlabJob(gitlabRemote, 802, " "),
    ).rejects.toThrow("GitLab token is required");
  });
});

describe("mergeGitlabMergeRequest", () => {
  it("merges a merge request with squash and the expected source sha", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          iid: 18,
          title: "Add GitLab merge",
          web_url:
            "https://gitlab.com/team/subgroup/project/-/merge_requests/18",
          state: "merged",
          author: { username: "dev-a" },
          source_branch: "feature/gitlab-merge",
          target_branch: "main",
          merge_status: "can_be_merged",
          detailed_merge_status: "not_open",
          sha: "def456",
        }),
        { status: 200 },
      ),
    );

    const result = await mergeGitlabMergeRequest(
      gitlabRemote,
      18,
      { squash: true, headSha: "def456" },
      "glpat_secret",
      fetchMock,
    );

    expect(fetchMock).toHaveBeenCalledWith(
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/merge_requests/18/merge",
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
    expect(result).toEqual({
      iid: 18,
      title: "Add GitLab merge",
      url: "https://gitlab.com/team/subgroup/project/-/merge_requests/18",
      draft: false,
      author: "dev-a",
      sourceBranch: "feature/gitlab-merge",
      targetBranch: "main",
      mergeStatus: "can_be_merged",
      detailedMergeStatus: "not_open",
    });
  });

  it("requires token and head sha before merging", async () => {
    await expect(
      mergeGitlabMergeRequest(
        gitlabRemote,
        18,
        { squash: false, headSha: "def456" },
        " ",
      ),
    ).rejects.toThrow("GitLab token is required");
    await expect(
      mergeGitlabMergeRequest(
        gitlabRemote,
        18,
        { squash: false, headSha: " " },
        "glpat_secret",
      ),
    ).rejects.toThrow("MR head SHA is required");
  });
});

describe("createGitlabMergeRequestNote", () => {
  it("posts a merge request note and maps the created note", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
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
    );

    const note = await createGitlabMergeRequestNote(
      gitlabRemote,
      18,
      " Please re-run the failed job. ",
      "glpat_secret",
      fetchMock,
    );

    expect(fetchMock).toHaveBeenCalledWith(
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/merge_requests/18/notes",
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
    expect(note).toEqual({
      id: 503,
      body: "Please re-run the failed job.",
      author: "me",
      createdAt: "2026-07-02T10:00:00.000Z",
      updatedAt: "2026-07-02T10:00:00.000Z",
      system: false,
      internal: false,
    });
  });

  it("requires body and token", async () => {
    await expect(
      createGitlabMergeRequestNote(gitlabRemote, 18, " ", "glpat_secret"),
    ).rejects.toThrow("MR note cannot be empty");

    await expect(
      createGitlabMergeRequestNote(
        gitlabRemote,
        18,
        "Please re-run the failed job.",
        " ",
      ),
    ).rejects.toThrow("GitLab token is required");
  });
});

describe("approveGitlabMergeRequest", () => {
  it("posts an approval and maps the updated approval summary", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
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
    );

    const approval = await approveGitlabMergeRequest(
      gitlabRemote,
      18,
      "glpat_secret",
      fetchMock,
    );

    expect(fetchMock).toHaveBeenCalledWith(
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/merge_requests/18/approve",
      {
        method: "POST",
        headers: {
          Accept: "application/json",
          "PRIVATE-TOKEN": "glpat_secret",
        },
      },
    );
    expect(approval).toEqual({
      approvalsRequired: 2,
      approvalsLeft: 0,
      approved: true,
      approvedBy: ["reviewer-a", "me"],
      userHasApproved: true,
      userCanApprove: false,
    });
  });
});

describe("unapproveGitlabMergeRequest", () => {
  it("posts an unapproval and maps the updated approval summary", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
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

    const approval = await unapproveGitlabMergeRequest(
      gitlabRemote,
      18,
      "glpat_secret",
      fetchMock,
    );

    expect(fetchMock).toHaveBeenCalledWith(
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/merge_requests/18/unapprove",
      {
        method: "POST",
        headers: {
          Accept: "application/json",
          "PRIVATE-TOKEN": "glpat_secret",
        },
      },
    );
    expect(approval).toEqual({
      approvalsRequired: 2,
      approvalsLeft: 1,
      approved: false,
      approvedBy: ["reviewer-a"],
      userHasApproved: false,
      userCanApprove: true,
    });
  });
});

describe("fetchGitlabMergeRequestDetails", () => {
  it("fetches merge request details and the latest pipeline", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            iid: 18,
            title: "Add GitLab details",
            web_url:
              "https://gitlab.com/team/subgroup/project/-/merge_requests/18",
            draft: false,
            author: { username: "dev-a" },
            source_branch: "feature/gitlab-details",
            target_branch: "main",
            merge_status: "can_be_merged",
            detailed_merge_status: "mergeable",
            changes_count: "12",
            user_notes_count: 5,
            blocking_discussions_resolved: false,
            has_conflicts: false,
            upvotes: 2,
            downvotes: 1,
            sha: "def456",
          }),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify([
            {
              id: 99,
              status: "success",
              ref: "refs/merge-requests/18/head",
              sha: "abc123",
              web_url: "https://gitlab.com/team/subgroup/project/-/pipelines/99",
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
              web_url:
                "https://gitlab.com/team/subgroup/project/-/jobs/801",
              started_at: "2026-07-03T10:00:00.000Z",
              finished_at: "2026-07-03T10:02:05.000Z",
            },
            {
              id: 802,
              name: "test-windows",
              stage: "test",
              status: "failed",
              duration: 89,
              web_url:
                "https://gitlab.com/team/subgroup/project/-/jobs/802",
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
            approved_by: [
              {
                user: {
                  username: "reviewer-a",
                },
              },
            ],
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
            {
              id: 502,
              body: "changed the title",
              author: { username: "gitlab-bot" },
              created_at: "2026-07-01T09:00:00.000Z",
              updated_at: "2026-07-01T09:00:00.000Z",
              system: true,
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
      );

    const detail = await fetchGitlabMergeRequestDetails(
      gitlabRemote,
      18,
      "glpat_secret",
      fetchMock,
    );

    expect(fetchMock).toHaveBeenNthCalledWith(
      1,
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/merge_requests/18",
      {
        headers: {
          Accept: "application/json",
          "PRIVATE-TOKEN": "glpat_secret",
        },
      },
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/merge_requests/18/pipelines?per_page=1",
      {
        headers: {
          Accept: "application/json",
          "PRIVATE-TOKEN": "glpat_secret",
        },
      },
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      3,
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/pipelines/99/jobs?per_page=20",
      {
        headers: {
          Accept: "application/json",
          "PRIVATE-TOKEN": "glpat_secret",
        },
      },
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      4,
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/merge_requests/18/approvals",
      {
        headers: {
          Accept: "application/json",
          "PRIVATE-TOKEN": "glpat_secret",
        },
      },
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      5,
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/merge_requests/18/notes?sort=desc&order_by=updated_at&per_page=5",
      {
        headers: {
          Accept: "application/json",
          "PRIVATE-TOKEN": "glpat_secret",
        },
      },
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      6,
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/merge_requests/18/discussions?per_page=20",
      {
        headers: {
          Accept: "application/json",
          "PRIVATE-TOKEN": "glpat_secret",
        },
      },
    );
    expect(detail).toEqual({
      iid: 18,
      title: "Add GitLab details",
      url: "https://gitlab.com/team/subgroup/project/-/merge_requests/18",
      draft: false,
      author: "dev-a",
      sourceBranch: "feature/gitlab-details",
      targetBranch: "main",
      mergeStatus: "can_be_merged",
      detailedMergeStatus: "mergeable",
      headSha: "def456",
      changesCount: "12",
      userNotesCount: 5,
      blockingDiscussionsResolved: false,
      hasConflicts: false,
      upvotes: 2,
      downvotes: 1,
      latestPipeline: {
        id: 99,
        status: "success",
        ref: "refs/merge-requests/18/head",
        sha: "abc123",
        url: "https://gitlab.com/team/subgroup/project/-/pipelines/99",
      },
      pipelineJobs: [
        {
          id: 801,
          name: "build-linux",
          stage: "build",
          status: "success",
          duration: 125.4,
          url: "https://gitlab.com/team/subgroup/project/-/jobs/801",
          startedAt: "2026-07-03T10:00:00.000Z",
          finishedAt: "2026-07-03T10:02:05.000Z",
        },
        {
          id: 802,
          name: "test-windows",
          stage: "test",
          status: "failed",
          duration: 89,
          url: "https://gitlab.com/team/subgroup/project/-/jobs/802",
          startedAt: "2026-07-03T10:01:00.000Z",
          finishedAt: "2026-07-03T10:02:29.000Z",
        },
      ],
      approvals: {
        approvalsRequired: 2,
        approvalsLeft: 1,
        approved: false,
        approvedBy: ["reviewer-a"],
        userHasApproved: false,
        userCanApprove: true,
      },
      notes: [
        {
          id: 501,
          body: "Looks good after the pipeline fix.",
          author: "reviewer-a",
          createdAt: "2026-07-01T10:00:00.000Z",
          updatedAt: "2026-07-01T10:05:00.000Z",
          system: false,
          internal: false,
        },
        {
          id: 502,
          body: "changed the title",
          author: "gitlab-bot",
          createdAt: "2026-07-01T09:00:00.000Z",
          updatedAt: "2026-07-01T09:00:00.000Z",
          system: true,
          internal: false,
        },
      ],
      discussions: [
        {
          id: "discussion-1",
          resolvable: true,
          resolved: false,
          path: "src/git.ts",
          line: 42,
          author: "reviewer-b",
          body: "This branch should handle null refs.",
          updatedAt: "2026-07-01T11:05:00.000Z",
        },
      ],
    });
  });
});

describe("createGitlabMergeRequest", () => {
  it("posts PRIVATE-TOKEN and maps the created merge request", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          iid: 18,
          title: "Add GitLab creation",
          web_url:
            "https://gitlab.com/team/subgroup/project/-/merge_requests/18",
          draft: true,
          author: { username: "dev-a" },
          source_branch: "feature/gitlab-create",
          target_branch: "main",
          merge_status: "checking",
          detailed_merge_status: "checking",
        }),
        { status: 201 },
      ),
    );

    const mr = await createGitlabMergeRequest(
      gitlabRemote,
      {
        title: " Add GitLab creation ",
        sourceBranch: " feature/gitlab-create ",
        targetBranch: " main ",
        description: "Create from the desktop client",
        draft: true,
      },
      "glpat_secret",
      fetchMock,
    );

    expect(fetchMock).toHaveBeenCalledWith(
      "https://gitlab.com/api/v4/projects/team%2Fsubgroup%2Fproject/merge_requests",
      {
        method: "POST",
        headers: {
          Accept: "application/json",
          "Content-Type": "application/json",
          "PRIVATE-TOKEN": "glpat_secret",
        },
        body: JSON.stringify({
          title: "Add GitLab creation",
          source_branch: "feature/gitlab-create",
          target_branch: "main",
          description: "Create from the desktop client",
          draft: true,
        }),
      },
    );
    expect(mr).toEqual({
      iid: 18,
      title: "Add GitLab creation",
      url: "https://gitlab.com/team/subgroup/project/-/merge_requests/18",
      draft: true,
      author: "dev-a",
      sourceBranch: "feature/gitlab-create",
      targetBranch: "main",
      mergeStatus: "checking",
      detailedMergeStatus: "checking",
    });
  });

  it("requires title, source branch, target branch, and token", async () => {
    await expect(
      createGitlabMergeRequest(
        gitlabRemote,
        {
          title: " ",
          sourceBranch: "feature/gitlab-create",
          targetBranch: "main",
          description: "",
          draft: false,
        },
        "glpat_secret",
      ),
    ).rejects.toThrow("MR title cannot be empty");

    await expect(
      createGitlabMergeRequest(
        gitlabRemote,
        {
          title: "Add GitLab creation",
          sourceBranch: " ",
          targetBranch: "main",
          description: "",
          draft: false,
        },
        "glpat_secret",
      ),
    ).rejects.toThrow("MR source branch cannot be empty");

    await expect(
      createGitlabMergeRequest(
        gitlabRemote,
        {
          title: "Add GitLab creation",
          sourceBranch: "feature/gitlab-create",
          targetBranch: " ",
          description: "",
          draft: false,
        },
        "glpat_secret",
      ),
    ).rejects.toThrow("MR target branch cannot be empty");

    await expect(
      createGitlabMergeRequest(
        gitlabRemote,
        {
          title: "Add GitLab creation",
          sourceBranch: "feature/gitlab-create",
          targetBranch: "main",
          description: "",
          draft: false,
        },
        " ",
      ),
    ).rejects.toThrow("GitLab token is required");
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
