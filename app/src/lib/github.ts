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

export interface GithubCheckRunSummary {
  id: number;
  name: string;
  status: string;
  conclusion: string | null;
  url: string;
  app: string | null;
  startedAt: string;
  completedAt: string;
}

export interface GithubPullRequestComment {
  id: number;
  body: string;
  url: string;
  author: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface GithubPullReviewThread {
  id: number;
  body: string;
  url: string;
  author: string | null;
  path: string;
  line: number | null;
  originalLine: number | null;
  diffHunk: string;
  createdAt: string;
  updatedAt: string;
}

export type GithubPullMergeMethod = "merge" | "squash" | "rebase";

export interface MergeGithubPullRequestInput {
  method: GithubPullMergeMethod;
  headSha: string;
}

export interface GithubPullMergeResult {
  sha: string;
  merged: boolean;
  message: string;
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
  checkRuns: GithubCheckRunSummary[];
  recentComments: GithubPullRequestComment[];
  reviewThreads: GithubPullReviewThread[];
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

interface GithubCheckRunsResponse {
  total_count?: number;
  check_runs?: GithubCheckRunResponse[];
}

interface GithubCheckRunResponse {
  id: number;
  name?: string | null;
  status?: string | null;
  conclusion?: string | null;
  html_url?: string | null;
  started_at?: string | null;
  completed_at?: string | null;
  app?: { slug?: string | null; name?: string | null } | null;
}

interface GithubIssueCommentResponse {
  id: number;
  body?: string | null;
  html_url?: string | null;
  user?: { login?: string | null } | null;
  created_at?: string | null;
  updated_at?: string | null;
}

interface GithubReviewCommentResponse {
  id: number;
  body?: string | null;
  html_url?: string | null;
  path?: string | null;
  line?: number | null;
  original_line?: number | null;
  diff_hunk?: string | null;
  user?: { login?: string | null } | null;
  created_at?: string | null;
  updated_at?: string | null;
}

interface GithubPullMergeResponse {
  sha?: string | null;
  merged?: boolean | null;
  message?: string | null;
}

export function buildGithubPullsApiUrl(
  remote: HostingRemote,
  branch: string | null,
): string | null {
  if (remote.provider !== "github") return null;

  const params = new URLSearchParams({ state: "open" });
  if (branch) params.set("head", `${remote.owner}:${branch}`);
  params.set("per_page", branch ? "20" : "50");
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

export function buildGithubCheckRunsApiUrl(
  remote: HostingRemote,
  ref: string,
): string {
  return `${githubRepoApiBase(remote)}/commits/${encodeURIComponent(ref)}/check-runs?per_page=20`;
}

export function buildGithubIssueCommentsApiUrl(
  remote: HostingRemote,
  issueNumber: number,
  perPage?: number,
): string {
  const url = `${githubRepoApiBase(remote)}/issues/${issueNumber}/comments`;
  return perPage ? `${url}?per_page=${perPage}` : url;
}

export function buildGithubPullReviewCommentsApiUrl(
  remote: HostingRemote,
  pullNumber: number,
): string {
  return `${githubRepoApiBase(remote)}/pulls/${pullNumber}/comments?per_page=20`;
}

export function buildGithubMergePullRequestApiUrl(
  remote: HostingRemote,
  pullNumber: number,
): string {
  return `${githubRepoApiBase(remote)}/pulls/${pullNumber}/merge`;
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

export async function mergeGithubPullRequest(
  remote: HostingRemote,
  pullNumber: number,
  input: MergeGithubPullRequestInput,
  token: string | null,
  fetcher: typeof fetch = fetch,
): Promise<GithubPullMergeResult> {
  const trimmedToken = token?.trim();
  if (!trimmedToken) throw new Error("GitHub token is required");
  const headSha = input.headSha.trim();
  if (!headSha) throw new Error("PR head SHA is required");

  const response = await fetcher(
    buildGithubMergePullRequestApiUrl(remote, pullNumber),
    {
      method: "PUT",
      headers: {
        ...githubHeaders(trimmedToken),
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        merge_method: input.method,
        sha: headSha,
      }),
    },
  );
  if (!response.ok) {
    throw new Error(githubApiErrorMessage(response.status));
  }

  const payload = (await response.json()) as GithubPullMergeResponse;
  return {
    sha: payload.sha ?? "",
    merged: payload.merged ?? false,
    message: payload.message ?? "",
  };
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
  let checkRuns: GithubCheckRunSummary[] = [];
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

    const checkRunsResponse = await fetcher(
      buildGithubCheckRunsApiUrl(remote, headSha),
      { headers: githubHeaders(token) },
    );
    if (checkRunsResponse.ok) {
      checkRuns = toCheckRunSummaries(
        (await checkRunsResponse.json()) as GithubCheckRunsResponse,
      );
    } else if (checkRunsResponse.status !== 403 && checkRunsResponse.status !== 404) {
      throw new Error(githubApiErrorMessage(checkRunsResponse.status));
    }
  }

  const commentsResponse = await fetcher(
    buildGithubIssueCommentsApiUrl(remote, pullNumber, 20),
    { headers: githubHeaders(token) },
  );
  if (!commentsResponse.ok) {
    throw new Error(githubApiErrorMessage(commentsResponse.status));
  }
  const recentComments = (
    (await commentsResponse.json()) as GithubIssueCommentResponse[]
  ).map(toPullRequestComment);

  const reviewCommentsResponse = await fetcher(
    buildGithubPullReviewCommentsApiUrl(remote, pullNumber),
    { headers: githubHeaders(token) },
  );
  if (!reviewCommentsResponse.ok) {
    throw new Error(githubApiErrorMessage(reviewCommentsResponse.status));
  }
  const reviewThreads = (
    (await reviewCommentsResponse.json()) as GithubReviewCommentResponse[]
  ).map(toPullReviewThread);

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
    checkRuns,
    recentComments,
    reviewThreads,
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

function toCheckRunSummaries(
  payload: GithubCheckRunsResponse,
): GithubCheckRunSummary[] {
  return (payload.check_runs ?? []).map((run) => ({
    id: run.id,
    name: run.name ?? "",
    status: run.status ?? "",
    conclusion: run.conclusion ?? null,
    url: run.html_url ?? "",
    app: run.app?.slug ?? run.app?.name ?? null,
    startedAt: run.started_at ?? "",
    completedAt: run.completed_at ?? "",
  }));
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

function toPullReviewThread(
  comment: GithubReviewCommentResponse,
): GithubPullReviewThread {
  return {
    id: comment.id,
    body: comment.body ?? "",
    url: comment.html_url ?? "",
    author: comment.user?.login ?? null,
    path: comment.path ?? "",
    line: comment.line ?? null,
    originalLine: comment.original_line ?? null,
    diffHunk: comment.diff_hunk ?? "",
    createdAt: comment.created_at ?? "",
    updatedAt: comment.updated_at ?? "",
  };
}
