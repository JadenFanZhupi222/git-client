import { act, renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { PropsWithChildren } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { GithubPullRequestDetails } from "./github";
import type { HostingRemote } from "./hosting";

const ipc = vi.hoisted(() => ({
  hasGithubToken: vi.fn(),
  getGithubToken: vi.fn(),
  hasGitlabToken: vi.fn(),
  getGitlabToken: vi.fn(),
}));
const github = vi.hoisted(() => ({
  fetchPulls: vi.fn(),
  fetchDetail: vi.fn(),
  createComment: vi.fn(),
  mergePull: vi.fn(),
}));

vi.mock("../ipc", () => ({
  hasGithubToken: ipc.hasGithubToken,
  getGithubToken: ipc.getGithubToken,
  hasGitlabToken: ipc.hasGitlabToken,
  getGitlabToken: ipc.getGitlabToken,
}));
vi.mock("./github", () => ({
  fetchGithubPullRequests: github.fetchPulls,
  fetchGithubPullRequestDetails: github.fetchDetail,
  createGithubPullRequestComment: github.createComment,
  mergeGithubPullRequest: github.mergePull,
}));
vi.mock("./gitlab", () => ({
  fetchGitlabMergeRequests: vi.fn(),
  fetchGitlabMergeRequestDetails: vi.fn(),
  approveGitlabMergeRequest: vi.fn(),
  unapproveGitlabMergeRequest: vi.fn(),
  createGitlabMergeRequestNote: vi.fn(),
  mergeGitlabMergeRequest: vi.fn(),
  retryGitlabJob: vi.fn(),
}));

import {
  hostingQueryKeys,
  useGithubPullCommentMutation,
  useGithubPullRequestDetails,
  useGithubPullRequests,
} from "./hostingQueries";

const remote: HostingRemote = {
  provider: "github",
  owner: "team",
  repo: "project",
  webBaseUrl: "https://github.com/team/project",
};

function createHarness() {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: Infinity },
      mutations: { retry: false },
    },
  });
  const wrapper = ({ children }: PropsWithChildren) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return { client, wrapper };
}

function detail(number = 7): GithubPullRequestDetails {
  return {
    number,
    title: `PR ${number}`,
    url: `https://github.com/team/project/pull/${number}`,
    draft: false,
    author: "dev",
    headRef: "feature",
    headSha: "abc",
    baseRef: "main",
    mergeable: true,
    mergeableState: "clean",
    comments: 0,
    reviewComments: 0,
    commits: 1,
    changedFiles: 1,
    additions: 1,
    deletions: 0,
    reviews: [],
    combinedStatus: null,
    checkRuns: [],
    recentComments: [],
    reviewThreads: [],
  };
}

describe("hosting queries", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    ipc.hasGithubToken.mockResolvedValue(true);
    ipc.getGithubToken.mockResolvedValue("secret-at-execution");
    ipc.hasGitlabToken.mockResolvedValue(true);
    ipc.getGitlabToken.mockResolvedValue("gitlab-secret");
  });

  it("keys lists by remote and branch without including credentials", () => {
    const key = hostingQueryKeys.githubPulls(remote, "feature/a");
    expect(key).toContain("feature/a");
    expect(key).toContain("team");
    expect(key).toContain("project");
    expect(key).not.toContain("secret-at-execution");
    expect(hostingQueryKeys.githubPulls(remote, "feature/b")).not.toEqual(key);
  });

  it("switches branch queries and reads the token only when each query executes", async () => {
    github.fetchPulls.mockImplementation((_remote, branch) => Promise.resolve([{ number: branch }]));
    const { wrapper } = createHarness();
    const hook = renderHook(
      ({ branch }) => useGithubPullRequests(remote, branch),
      { initialProps: { branch: "feature/a" }, wrapper },
    );
    await waitFor(() => expect(hook.result.current.data).toEqual([{ number: "feature/a" }]));
    hook.rerender({ branch: "feature/b" });
    await waitFor(() => expect(hook.result.current.data).toEqual([{ number: "feature/b" }]));
    expect(ipc.getGithubToken).toHaveBeenCalledTimes(2);
    expect(github.fetchPulls).toHaveBeenNthCalledWith(2, remote, "feature/b", "secret-at-execution");
  });

  it("keeps detail cache per number and accepts completion after unmount", async () => {
    let finish!: (value: GithubPullRequestDetails) => void;
    github.fetchDetail.mockImplementation((_remote, number) =>
      number === 1
        ? Promise.resolve(detail(1))
        : new Promise<GithubPullRequestDetails>((resolve) => { finish = resolve; }),
    );
    const { client, wrapper } = createHarness();
    const hook = renderHook(
      ({ number }) => useGithubPullRequestDetails(remote, number),
      { initialProps: { number: 1 }, wrapper },
    );
    await waitFor(() => expect(hook.result.current.data?.number).toBe(1));
    hook.rerender({ number: 2 });
    await waitFor(() => expect(github.fetchDetail).toHaveBeenCalledTimes(2));
    hook.unmount();
    finish(detail(2));
    await waitFor(() => {
      expect(client.getQueryData(hostingQueryKeys.githubPull(remote, 2))).toEqual(detail(2));
    });
    expect(client.getQueryData(hostingQueryKeys.githubPull(remote, 1))).toEqual(detail(1));
  });

  it("rejects a write with a missing token before calling the hosting API", async () => {
    ipc.hasGithubToken.mockResolvedValue(false);
    const { wrapper } = createHarness();
    const hook = renderHook(
      () => useGithubPullCommentMutation(remote, "feature"),
      { wrapper },
    );
    await expect(hook.result.current.mutateAsync({ detail: detail(), body: "hello" }))
      .rejects.toThrow("GitHub token is required");
    expect(github.createComment).not.toHaveBeenCalled();
  });

  it("serializes concurrent writes and updates then invalidates cached detail", async () => {
    const releases: Array<() => void> = [];
    let active = 0;
    let maxActive = 0;
    github.createComment.mockImplementation(async () => {
      active += 1;
      maxActive = Math.max(maxActive, active);
      await new Promise<void>((resolve) => releases.push(resolve));
      active -= 1;
      return { id: releases.length, body: "ok", url: "url", author: "dev", createdAt: "", updatedAt: "" };
    });
    const { client, wrapper } = createHarness();
    client.setQueryData(hostingQueryKeys.githubPull(remote, 7), detail());
    const hook = renderHook(
      () => useGithubPullCommentMutation(remote, "feature"),
      { wrapper },
    );

    let first!: Promise<unknown>;
    let second!: Promise<unknown>;
    act(() => {
      first = hook.result.current.mutateAsync({ detail: detail(), body: "one" });
      second = hook.result.current.mutateAsync({ detail: detail(), body: "two" });
    });
    await waitFor(() => expect(releases).toHaveLength(1));
    releases[0]();
    await waitFor(() => expect(releases).toHaveLength(2));
    releases[1]();
    await act(async () => { await Promise.all([first, second]); });

    expect(maxActive).toBe(1);
    expect(client.getQueryData<GithubPullRequestDetails>(hostingQueryKeys.githubPull(remote, 7))?.comments).toBe(2);
    expect(client.getQueryState(hostingQueryKeys.githubPull(remote, 7))?.isInvalidated).toBe(true);
  });
});
