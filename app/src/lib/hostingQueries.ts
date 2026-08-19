import {
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from "@tanstack/react-query";
import {
  getGithubToken,
  getGitlabToken,
  hasGithubToken,
  hasGitlabToken,
} from "../ipc";
import {
  createGithubPullRequestComment,
  fetchGithubPullRequestDetails,
  fetchGithubPullRequests,
  mergeGithubPullRequest,
  type GithubPullMergeMethod,
  type GithubPullRequestDetails,
} from "./github";
import {
  approveGitlabMergeRequest,
  createGitlabMergeRequestNote,
  fetchGitlabMergeRequestDetails,
  fetchGitlabMergeRequests,
  mergeGitlabMergeRequest,
  retryGitlabJob,
  unapproveGitlabMergeRequest,
  type GitlabMergeRequestDetails,
  type GitlabPipelineJobSummary,
} from "./gitlab";
import type { HostingRemote } from "./hosting";

type RemoteIdentity = readonly [string, string, string, string];

function remoteIdentity(remote: HostingRemote | null): RemoteIdentity {
  return remote
    ? [remote.provider, remote.webBaseUrl, remote.owner, remote.repo]
    : ["", "", "", ""];
}

export const hostingQueryKeys = {
  githubPulls: (remote: HostingRemote | null, branch: string | null) =>
    ["hosting", "github", "pulls", ...remoteIdentity(remote), branch ?? ""] as const,
  githubPull: (remote: HostingRemote | null, number: number | null) =>
    ["hosting", "github", "pull", ...remoteIdentity(remote), number ?? 0] as const,
  gitlabMrs: (remote: HostingRemote | null, branch: string | null) =>
    ["hosting", "gitlab", "mrs", ...remoteIdentity(remote), branch ?? ""] as const,
  gitlabMr: (remote: HostingRemote | null, iid: number | null) =>
    ["hosting", "gitlab", "mr", ...remoteIdentity(remote), iid ?? 0] as const,
};

async function optionalGithubToken(): Promise<string | null> {
  return (await hasGithubToken()) ? getGithubToken() : null;
}

async function optionalGitlabToken(): Promise<string | null> {
  return (await hasGitlabToken()) ? getGitlabToken() : null;
}

async function requiredGithubToken(): Promise<string> {
  const token = await optionalGithubToken();
  if (!token?.trim()) throw new Error("GitHub token is required");
  return token;
}

async function requiredGitlabToken(): Promise<string> {
  const token = await optionalGitlabToken();
  if (!token?.trim()) throw new Error("GitLab token is required");
  return token;
}

function invalidateWithoutRefetch(
  client: QueryClient,
  queryKey: readonly unknown[],
) {
  return client.invalidateQueries({ queryKey, refetchType: "none" });
}

function mutationScope(parts: readonly unknown[], action: string) {
  return { id: `${parts.join("|")}|${action}` };
}

export function useGithubPullRequests(
  remote: HostingRemote | null,
  branch: string | null,
) {
  return useQuery({
    queryKey: hostingQueryKeys.githubPulls(remote, branch),
    queryFn: async () => fetchGithubPullRequests(remote!, branch!, await optionalGithubToken()),
    enabled: !!remote && !!branch,
  });
}

export function useGithubPullRequestDetails(
  remote: HostingRemote | null,
  number: number | null,
) {
  return useQuery({
    queryKey: hostingQueryKeys.githubPull(remote, number),
    queryFn: async () => fetchGithubPullRequestDetails(remote!, number!, await optionalGithubToken()),
    enabled: !!remote && number !== null,
  });
}

export function useGithubPullCommentMutation(
  remote: HostingRemote | null,
  branch: string | null,
) {
  const client = useQueryClient();
  return useMutation({
    scope: mutationScope(hostingQueryKeys.githubPulls(remote, branch), "comment"),
    mutationFn: async ({ detail, body }: { detail: GithubPullRequestDetails; body: string }) => {
      if (!remote) throw new Error("GitHub remote is required");
      return createGithubPullRequestComment(remote, detail.number, body, await requiredGithubToken());
    },
    onSuccess: async (comment, { detail }) => {
      client.setQueryData<GithubPullRequestDetails>(
        hostingQueryKeys.githubPull(remote, detail.number),
        (current) => ({
          ...(current ?? detail),
          comments: (current ?? detail).comments + 1,
          recentComments: [...(current ?? detail).recentComments, comment].slice(-20),
        }),
      );
      await Promise.all([
        invalidateWithoutRefetch(client, hostingQueryKeys.githubPull(remote, detail.number)),
        invalidateWithoutRefetch(client, hostingQueryKeys.githubPulls(remote, branch)),
      ]);
    },
  });
}

export function useGithubPullMergeMutation(
  remote: HostingRemote | null,
  branch: string | null,
) {
  const client = useQueryClient();
  return useMutation({
    scope: mutationScope(hostingQueryKeys.githubPulls(remote, branch), "merge"),
    mutationFn: async ({ detail, method }: { detail: GithubPullRequestDetails; method: GithubPullMergeMethod }) => {
      if (!remote) throw new Error("GitHub remote is required");
      const result = await mergeGithubPullRequest(
        remote,
        detail.number,
        { method, headSha: detail.headSha },
        await requiredGithubToken(),
      );
      return { result, detail };
    },
    onSuccess: async ({ detail }) => {
      client.setQueryData(
        hostingQueryKeys.githubPulls(remote, branch),
        (current: Array<{ number: number }> | undefined) =>
          current?.filter((pull) => pull.number !== detail.number),
      );
      client.removeQueries({ queryKey: hostingQueryKeys.githubPull(remote, detail.number), exact: true });
      await invalidateWithoutRefetch(client, hostingQueryKeys.githubPulls(remote, branch));
    },
  });
}

export function useGitlabMergeRequests(
  remote: HostingRemote | null,
  branch: string | null,
) {
  return useQuery({
    queryKey: hostingQueryKeys.gitlabMrs(remote, branch),
    queryFn: async () => fetchGitlabMergeRequests(remote!, branch!, await optionalGitlabToken()),
    enabled: !!remote && !!branch,
  });
}

export function useGitlabMergeRequestDetails(
  remote: HostingRemote | null,
  iid: number | null,
) {
  return useQuery({
    queryKey: hostingQueryKeys.gitlabMr(remote, iid),
    queryFn: async () => fetchGitlabMergeRequestDetails(remote!, iid!, await optionalGitlabToken()),
    enabled: !!remote && iid !== null,
  });
}

function useGitlabDetailMutation<TVariables, TResult>(
  remote: HostingRemote | null,
  branch: string | null,
  mutationFn: (variables: TVariables, token: string) => Promise<TResult>,
  update: (current: GitlabMergeRequestDetails, result: TResult, variables: TVariables) => GitlabMergeRequestDetails,
  iidOf: (variables: TVariables) => number,
) {
  const client = useQueryClient();
  return useMutation({
    scope: mutationScope(hostingQueryKeys.gitlabMrs(remote, branch), mutationFn.name || "detail"),
    mutationFn: async (variables: TVariables) => mutationFn(variables, await requiredGitlabToken()),
    onSuccess: async (result, variables) => {
      const iid = iidOf(variables);
      client.setQueryData<GitlabMergeRequestDetails>(
        hostingQueryKeys.gitlabMr(remote, iid),
        (current) => current ? update(current, result, variables) : current,
      );
      await Promise.all([
        invalidateWithoutRefetch(client, hostingQueryKeys.gitlabMr(remote, iid)),
        invalidateWithoutRefetch(client, hostingQueryKeys.gitlabMrs(remote, branch)),
      ]);
    },
  });
}

export function useGitlabApprovalMutation(
  remote: HostingRemote | null,
  branch: string | null,
  action: "approve" | "unapprove",
) {
  return useGitlabDetailMutation(
    remote,
    branch,
    ({ iid }: { iid: number }, token) => {
      if (!remote) throw new Error("GitLab remote is required");
      return action === "approve"
        ? approveGitlabMergeRequest(remote, iid, token)
        : unapproveGitlabMergeRequest(remote, iid, token);
    },
    (current, approvals) => ({ ...current, approvals }),
    ({ iid }) => iid,
  );
}

export function useGitlabNoteMutation(
  remote: HostingRemote | null,
  branch: string | null,
) {
  return useGitlabDetailMutation(
    remote,
    branch,
    ({ iid, body }: { iid: number; body: string }, token) => {
      if (!remote) throw new Error("GitLab remote is required");
      return createGitlabMergeRequestNote(remote, iid, body, token);
    },
    (current, note) => ({
      ...current,
      userNotesCount: current.userNotesCount + 1,
      notes: [note, ...current.notes],
    }),
    ({ iid }) => iid,
  );
}

export function useGitlabRetryJobMutation(
  remote: HostingRemote | null,
  branch: string | null,
) {
  return useGitlabDetailMutation(
    remote,
    branch,
    ({ job }: { iid: number; job: GitlabPipelineJobSummary }, token) => {
      if (!remote) throw new Error("GitLab remote is required");
      return retryGitlabJob(remote, job.id, token);
    },
    (current, updatedJob) => ({
      ...current,
      pipelineJobs: current.pipelineJobs.map((job) =>
        job.id === updatedJob.id ? updatedJob : job,
      ),
    }),
    ({ iid }) => iid,
  );
}

export function useGitlabMergeMutation(
  remote: HostingRemote | null,
  branch: string | null,
) {
  const client = useQueryClient();
  return useMutation({
    scope: mutationScope(hostingQueryKeys.gitlabMrs(remote, branch), "merge"),
    mutationFn: async ({ detail, squash }: { detail: GitlabMergeRequestDetails; squash: boolean }) => {
      if (!remote) throw new Error("GitLab remote is required");
      return mergeGitlabMergeRequest(
        remote,
        detail.iid,
        { squash, headSha: detail.headSha },
        await requiredGitlabToken(),
      );
    },
    onSuccess: async (merged) => {
      client.setQueryData(
        hostingQueryKeys.gitlabMrs(remote, branch),
        (current: Array<{ iid: number }> | undefined) =>
          current?.filter((mr) => mr.iid !== merged.iid),
      );
      client.removeQueries({ queryKey: hostingQueryKeys.gitlabMr(remote, merged.iid), exact: true });
      await invalidateWithoutRefetch(client, hostingQueryKeys.gitlabMrs(remote, branch));
    },
  });
}
