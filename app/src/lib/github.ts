import type { HostingRemote } from "./hosting";

export interface GithubPullRequestSummary {
  number: number;
  title: string;
  url: string;
  draft: boolean;
  author: string | null;
  headRef: string;
  headSha: string;
  baseRef: string;
}

export interface GithubPullReviewSummary {
  state: string;
  author: string | null;
}

export interface GithubCombinedStatusSummary {
  state: string;
  totalCount: number;
  statuses: Array<{
    context: string;
    state: string;
    targetUrl: string | null;
  }>;
}

export interface GithubPullRequestComment {
  id: number;
  body: string;
  url: string;
  author: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface GithubPullRequestDetails extends GithubPullRequestSummary {
  mergeable: boolean | null;
  mergeableState: string | null;
  comments: number;
  reviewComments: number;
  commits: number;
  changedFiles: number;
  additions: number;
  deletions: number;
  reviews: GithubPullReviewSummary[];
  combinedStatus: GithubCombinedStatusSummary | null;
}

export interface CreateGithubPullRequestInput {
  title: string;
  head: string;
  base: string;
  body: string;
  draft: boolean;
}

interface GithubPullRequestResponse {
  number: number;
  title: string;
  html_url: string;
  draft?: boolean;
  mergeable?: boolean | null;
  mergeable_state?: string | null;
  comments?: number;
  review_comments?: number;
  commits?: number;
  changed_files?: number;
  additions?: number;
  deletions?: number;
  user?: { login?: string | null } | null;
  head?: { ref?: string | null; sha?: string | null } | null;
  base?: { ref?: string | null } | null;
}

interface GithubReviewResponse {
  state?: string | null;
  user?: { login?: string | null } | null;
}

interface GithubCombinedStatusResponse {
  state?: string | null;
  total_count?: number;
  statuses?: Array<{
    context?: string | null;
    state?: string | null;
    target_url?: string | null;
  }>;
}

interface GithubIssueCommentResponse {
  id: number;
  body?: string | null;
  html_url?: string | null;
  user?: { login?: string | null } | null;
  created_at?: string | null;
  updated_at?: string | null;
}

export function buildGithubPullsApiUrl(
  remote: HostingRemote,
  branch: string | null,
): string | null {
  if (remote.provider !== "github" || !branch) return null;

  const params = new URLSearchParams({
    state: "open",
    head: `${remote.owner}:${branch}`,
    per_page: "20",
  });
  return `https://api.github.com/repos/${encodeURIComponent(remote.owner)}/${encodeURIComponent(remote.repo)}/pulls?${params.toString()}`;
}

export function buildGithubCreatePullApiUrl(remote: HostingRemote): string {
  return `${githubRepoApiBase(remote)}/pulls`;
}

export function buildGithubPullApiUrl(
  remote: HostingRemote,
  pullNumber: number,
): string {
  return `${githubRepoApiBase(remote)}/pulls/${pullNumber}`;
}

export function buildGithubReviewsApiUrl(
  remote: HostingRemote,
  pullNumber: number,
): string {
  return `${githubRepoApiBase(remote)}/pulls/${pullNumber}/reviews?per_page=30`;
}

export function buildGithubCombinedStatusApiUrl(
  remote: HostingRemote,
  ref: string,
): string {
  return `${githubRepoApiBase(remote)}/commits/${encodeURIComponent(ref)}/status`;
}

export function buildGithubIssueCommentsApiUrl(
  remote: HostingRemote,
  issueNumber: number,
): string {
  return `${githubRepoApiBase(remote)}/issues/${issueNumber}/comments`;
}

export async function fetchGithubPullRequests(
  remote: HostingRemote,
  branch: string | null,
  token: string | null,
  fetcher: typeof fetch = fetch,
): Promise<GithubPullRequestSummary[]> {
  const url = buildGithubPullsApiUrl(remote, branch);
  if (!url) return [];

  const headers: Record<string, string> = {
    Accept: "application/vnd.github+json",
  };
  const trimmedToken = token?.trim();
  if (trimmedToken) headers.Authorization = `Bearer ${trimmedToken}`;

  const response = await fetcher(url, { headers });
  if (!response.ok) {
    throw new Error(githubApiErrorMessage(response.status));
  }

  const payload = (await response.json()) as GithubPullRequestResponse[];
  return payload.map((pr) => ({
    ...toPullRequestSummary(pr),
  }));
}

export async function createGithubPullRequest(
  remote: HostingRemote,
  input: CreateGithubPullRequestInput,
  token: string | null,
  fetcher: typeof fetch = fetch,
): Promise<GithubPullRequestSummary> {
  const payload = normalizeCreatePullRequestInput(input);
  const response = await fetcher(buildGithubCreatePullApiUrl(remote), {
    method: "POST",
    headers: {
      ...githubHeaders(token),
      "Content-Type": "application/json",
    },
    body: JSON.stringify(payload),
  });
  if (!response.ok) {
    throw new Error(githubApiErrorMessage(response.status));
  }
  return toPullRequestSummary(
    (await response.json()) as GithubPullRequestResponse,
  );
}

export async function createGithubPullRequestComment(
  remote: HostingRemote,
  pullNumber: number,
  body: string,
  token: string | null,
  fetcher: typeof fetch = fetch,
): Promise<GithubPullRequestComment> {
  const trimmedBody = body.trim();
  if (!trimmedBody) throw new Error("PR comment cannot be empty");
  const trimmedToken = token?.trim();
  if (!trimmedToken) throw new Error("GitHub token is required");

  const response = await fetcher(
    buildGithubIssueCommentsApiUrl(remote, pullNumber),
    {
      method: "POST",
      headers: {
        ...githubHeaders(trimmedToken),
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ body: trimmedBody }),
    },
  );
  if (!response.ok) {
    throw new Error(githubApiErrorMessage(response.status));
  }

  return toPullRequestComment(
    (await response.json()) as GithubIssueCommentResponse,
  );
}

export async function fetchGithubPullRequestDetails(
  remote: HostingRemote,
  pullNumber: number,
  token: string | null,
  fetcher: typeof fetch = fetch,
): Promise<GithubPullRequestDetails> {
  const pullResponse = await fetcher(
    buildGithubPullApiUrl(remote, pullNumber),
    {
      headers: githubHeaders(token),
    },
  );
  if (!pullResponse.ok) {
    throw new Error(githubApiErrorMessage(pullResponse.status));
  }

  const pull = (await pullResponse.json()) as GithubPullRequestResponse;
  const reviewsResponse = await fetcher(
    buildGithubReviewsApiUrl(remote, pullNumber),
    { headers: githubHeaders(token) },
  );
  if (!reviewsResponse.ok) {
    throw new Error(githubApiErrorMessage(reviewsResponse.status));
  }
  const reviews = (await reviewsResponse.json()) as GithubReviewResponse[];

  const headSha = pull.head?.sha ?? "";
  let combinedStatus: GithubCombinedStatusSummary | null = null;
  if (headSha) {
    const statusResponse = await fetcher(
      buildGithubCombinedStatusApiUrl(remote, headSha),
      { headers: githubHeaders(token) },
    );
    if (!statusResponse.ok) {
      throw new Error(githubApiErrorMessage(statusResponse.status));
    }
    combinedStatus = toCombinedStatusSummary(
      (await statusResponse.json()) as GithubCombinedStatusResponse,
    );
  }

  return {
    ...toPullRequestSummary(pull),
    mergeable: pull.mergeable ?? null,
    mergeableState: pull.mergeable_state ?? null,
    comments: pull.comments ?? 0,
    reviewComments: pull.review_comments ?? 0,
    commits: pull.commits ?? 0,
    changedFiles: pull.changed_files ?? 0,
    additions: pull.additions ?? 0,
    deletions: pull.deletions ?? 0,
    reviews: reviews.map((review) => ({
      state: review.state ?? "",
      author: review.user?.login ?? null,
    })),
    combinedStatus,
  };
}

export function githubApiErrorMessage(status: number): string {
  switch (status) {
    case 401:
      return "GitHub token 无效或已过期";
    case 403:
      return "GitHub API 访问被拒绝，可能是权限不足或触发限流";
    case 404:
      return "GitHub 仓库不存在，或当前 token 没有访问权限";
    default:
      return `GitHub API 请求失败: HTTP ${status}`;
  }
}

function githubRepoApiBase(remote: HostingRemote): string {
  return `https://api.github.com/repos/${encodeURIComponent(remote.owner)}/${encodeURIComponent(remote.repo)}`;
}

function githubHeaders(token: string | null): Record<string, string> {
  const headers: Record<string, string> = {
    Accept: "application/vnd.github+json",
  };
  const trimmedToken = token?.trim();
  if (trimmedToken) headers.Authorization = `Bearer ${trimmedToken}`;
  return headers;
}

function normalizeCreatePullRequestInput(
  input: CreateGithubPullRequestInput,
): CreateGithubPullRequestInput {
  const title = input.title.trim();
  const head = input.head.trim();
  const base = input.base.trim();
  if (!title) throw new Error("PR 标题不能为空");
  if (!head) throw new Error("PR source branch 不能为空");
  if (!base) throw new Error("PR base branch 不能为空");
  return {
    title,
    body: input.body,
    head,
    base,
    draft: input.draft,
  };
}

function toPullRequestSummary(
  pr: GithubPullRequestResponse,
): GithubPullRequestSummary {
  return {
    number: pr.number,
    title: pr.title,
    url: pr.html_url,
    draft: pr.draft ?? false,
    author: pr.user?.login ?? null,
    headRef: pr.head?.ref ?? "",
    headSha: pr.head?.sha ?? "",
    baseRef: pr.base?.ref ?? "",
  };
}

function toCombinedStatusSummary(
  status: GithubCombinedStatusResponse,
): GithubCombinedStatusSummary {
  return {
    state: status.state ?? "pending",
    totalCount: status.total_count ?? 0,
    statuses: (status.statuses ?? []).map((item) => ({
      context: item.context ?? "",
      state: item.state ?? "",
      targetUrl: item.target_url ?? null,
    })),
  };
}

function toPullRequestComment(
  comment: GithubIssueCommentResponse,
): GithubPullRequestComment {
  return {
    id: comment.id,
    body: comment.body ?? "",
    url: comment.html_url ?? "",
    author: comment.user?.login ?? null,
    createdAt: comment.created_at ?? "",
    updatedAt: comment.updated_at ?? "",
  };
}
