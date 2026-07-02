import type { HostingRemote } from "./hosting";

export interface GitlabMergeRequestSummary {
  iid: number;
  title: string;
  url: string;
  draft: boolean;
  author: string | null;
  sourceBranch: string;
  targetBranch: string;
  mergeStatus: string;
  detailedMergeStatus: string;
}

interface GitlabMergeRequestResponse {
  iid: number;
  title: string;
  web_url: string;
  draft?: boolean;
  work_in_progress?: boolean;
  author?: { username?: string | null } | null;
  source_branch?: string | null;
  target_branch?: string | null;
  merge_status?: string | null;
  detailed_merge_status?: string | null;
}

export function buildGitlabMergeRequestsApiUrl(
  remote: HostingRemote,
  branch: string | null,
): string | null {
  if (remote.provider !== "gitlab" || !branch) return null;

  const projectPath = `${remote.owner}/${remote.repo}`;
  const params = new URLSearchParams({
    state: "opened",
    source_branch: branch,
    per_page: "20",
  });
  return `https://gitlab.com/api/v4/projects/${encodeURIComponent(projectPath)}/merge_requests?${params.toString()}`;
}

export async function fetchGitlabMergeRequests(
  remote: HostingRemote,
  branch: string | null,
  token: string | null,
  fetcher: typeof fetch = fetch,
): Promise<GitlabMergeRequestSummary[]> {
  const url = buildGitlabMergeRequestsApiUrl(remote, branch);
  if (!url) return [];

  const response = await fetcher(url, { headers: gitlabHeaders(token) });
  if (!response.ok) {
    throw new Error(gitlabApiErrorMessage(response.status));
  }

  const payload = (await response.json()) as GitlabMergeRequestResponse[];
  return payload.map(toMergeRequestSummary);
}

export function gitlabApiErrorMessage(status: number): string {
  switch (status) {
    case 401:
      return "GitLab token 无效或已过期";
    case 403:
      return "GitLab API 访问被拒绝，可能是权限不足";
    case 404:
      return "GitLab 项目不存在，或当前 token 没有访问权限";
    default:
      return `GitLab API 请求失败: HTTP ${status}`;
  }
}

function gitlabHeaders(token: string | null): Record<string, string> {
  const headers: Record<string, string> = {
    Accept: "application/json",
  };
  const trimmedToken = token?.trim();
  if (trimmedToken) headers["PRIVATE-TOKEN"] = trimmedToken;
  return headers;
}

function toMergeRequestSummary(
  mr: GitlabMergeRequestResponse,
): GitlabMergeRequestSummary {
  return {
    iid: mr.iid,
    title: mr.title,
    url: mr.web_url,
    draft: mr.draft ?? mr.work_in_progress ?? false,
    author: mr.author?.username ?? null,
    sourceBranch: mr.source_branch ?? "",
    targetBranch: mr.target_branch ?? "",
    mergeStatus: mr.merge_status ?? "",
    detailedMergeStatus: mr.detailed_merge_status ?? "",
  };
}
