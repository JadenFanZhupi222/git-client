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

export interface GitlabPipelineSummary {
  id: number;
  status: string;
  ref: string;
  sha: string;
  url: string | null;
}

export interface GitlabPipelineJobSummary {
  id: number;
  name: string;
  stage: string;
  status: string;
  duration: number | null;
  url: string | null;
  startedAt: string;
  finishedAt: string;
}

export interface GitlabApprovalSummary {
  approvalsRequired: number;
  approvalsLeft: number;
  approved: boolean;
  approvedBy: string[];
  userHasApproved: boolean;
  userCanApprove: boolean;
}

export interface GitlabMergeRequestNote {
  id: number;
  body: string;
  author: string | null;
  createdAt: string;
  updatedAt: string;
  system: boolean;
  internal: boolean;
}

export interface GitlabMergeRequestDiscussion {
  id: string;
  resolvable: boolean;
  resolved: boolean;
  path: string;
  line: number | null;
  author: string | null;
  body: string;
  updatedAt: string;
}

export interface GitlabMergeRequestDetails
  extends GitlabMergeRequestSummary {
  headSha: string;
  changesCount: string;
  userNotesCount: number;
  blockingDiscussionsResolved: boolean | null;
  hasConflicts: boolean;
  upvotes: number;
  downvotes: number;
  latestPipeline: GitlabPipelineSummary | null;
  pipelineJobs: GitlabPipelineJobSummary[];
  approvals: GitlabApprovalSummary | null;
  notes: GitlabMergeRequestNote[];
  discussions: GitlabMergeRequestDiscussion[];
}

export interface CreateGitlabMergeRequestInput {
  title: string;
  sourceBranch: string;
  targetBranch: string;
  description: string;
  draft: boolean;
}

export interface MergeGitlabMergeRequestInput {
  squash: boolean;
  headSha: string;
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
  changes_count?: string | null;
  user_notes_count?: number | null;
  blocking_discussions_resolved?: boolean | null;
  has_conflicts?: boolean | null;
  upvotes?: number | null;
  downvotes?: number | null;
  sha?: string | null;
}

interface GitlabPipelineResponse {
  id: number;
  status?: string | null;
  ref?: string | null;
  sha?: string | null;
  web_url?: string | null;
}

interface GitlabPipelineJobResponse {
  id: number;
  name?: string | null;
  stage?: string | null;
  status?: string | null;
  duration?: number | null;
  web_url?: string | null;
  started_at?: string | null;
  finished_at?: string | null;
}

interface GitlabApprovalsResponse {
  approvals_required?: number | null;
  approvals_left?: number | null;
  approved?: boolean | null;
  approved_by?: Array<{
    user?: { username?: string | null } | null;
  }> | null;
  user_has_approved?: boolean | null;
  user_can_approve?: boolean | null;
}

interface GitlabMergeRequestNoteResponse {
  id: number;
  body?: string | null;
  author?: { username?: string | null } | null;
  created_at?: string | null;
  updated_at?: string | null;
  system?: boolean | null;
  internal?: boolean | null;
}

interface GitlabDiscussionResponse {
  id: string;
  individual_note?: boolean | null;
  notes?: GitlabDiscussionNoteResponse[] | null;
}

interface GitlabDiscussionNoteResponse {
  id: number;
  type?: string | null;
  body?: string | null;
  author?: { username?: string | null } | null;
  created_at?: string | null;
  updated_at?: string | null;
  system?: boolean | null;
  resolvable?: boolean | null;
  resolved?: boolean | null;
  position?: {
    new_path?: string | null;
    old_path?: string | null;
    new_line?: number | null;
    old_line?: number | null;
  } | null;
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

export function buildGitlabCreateMergeRequestApiUrl(
  remote: HostingRemote,
): string {
  const projectPath = `${remote.owner}/${remote.repo}`;
  return `https://gitlab.com/api/v4/projects/${encodeURIComponent(projectPath)}/merge_requests`;
}

export function buildGitlabMergeRequestApiUrl(
  remote: HostingRemote,
  iid: number,
): string {
  return `${buildGitlabCreateMergeRequestApiUrl(remote)}/${iid}`;
}

export function buildGitlabMergeRequestPipelinesApiUrl(
  remote: HostingRemote,
  iid: number,
): string {
  const params = new URLSearchParams({ per_page: "1" });
  return `${buildGitlabMergeRequestApiUrl(remote, iid)}/pipelines?${params.toString()}`;
}

export function buildGitlabPipelineJobsApiUrl(
  remote: HostingRemote,
  pipelineId: number,
): string {
  const projectPath = `${remote.owner}/${remote.repo}`;
  const params = new URLSearchParams({ per_page: "20" });
  return `https://gitlab.com/api/v4/projects/${encodeURIComponent(projectPath)}/pipelines/${pipelineId}/jobs?${params.toString()}`;
}

export function buildGitlabRetryJobApiUrl(
  remote: HostingRemote,
  jobId: number,
): string {
  const projectPath = `${remote.owner}/${remote.repo}`;
  return `https://gitlab.com/api/v4/projects/${encodeURIComponent(projectPath)}/jobs/${jobId}/retry`;
}

export function buildGitlabMergeRequestApprovalsApiUrl(
  remote: HostingRemote,
  iid: number,
): string {
  return `${buildGitlabMergeRequestApiUrl(remote, iid)}/approvals`;
}

export function buildGitlabMergeRequestNotesApiUrl(
  remote: HostingRemote,
  iid: number,
): string {
  const params = new URLSearchParams({
    sort: "desc",
    order_by: "updated_at",
    per_page: "5",
  });
  return `${buildGitlabMergeRequestApiUrl(remote, iid)}/notes?${params.toString()}`;
}

export function buildGitlabMergeRequestDiscussionsApiUrl(
  remote: HostingRemote,
  iid: number,
): string {
  const params = new URLSearchParams({ per_page: "20" });
  return `${buildGitlabMergeRequestApiUrl(remote, iid)}/discussions?${params.toString()}`;
}

export function buildGitlabMergeRequestNoteCreateApiUrl(
  remote: HostingRemote,
  iid: number,
): string {
  return `${buildGitlabMergeRequestApiUrl(remote, iid)}/notes`;
}

export function buildGitlabMergeRequestApproveApiUrl(
  remote: HostingRemote,
  iid: number,
): string {
  return `${buildGitlabMergeRequestApiUrl(remote, iid)}/approve`;
}

export function buildGitlabMergeRequestUnapproveApiUrl(
  remote: HostingRemote,
  iid: number,
): string {
  return `${buildGitlabMergeRequestApiUrl(remote, iid)}/unapprove`;
}

export function buildGitlabMergeRequestMergeApiUrl(
  remote: HostingRemote,
  iid: number,
): string {
  return `${buildGitlabMergeRequestApiUrl(remote, iid)}/merge`;
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

export async function createGitlabMergeRequest(
  remote: HostingRemote,
  input: CreateGitlabMergeRequestInput,
  token: string | null,
  fetcher: typeof fetch = fetch,
): Promise<GitlabMergeRequestSummary> {
  const payload = normalizeCreateMergeRequestInput(input);
  const trimmedToken = token?.trim();
  if (!trimmedToken) throw new Error("GitLab token is required");

  const response = await fetcher(buildGitlabCreateMergeRequestApiUrl(remote), {
    method: "POST",
    headers: {
      ...gitlabHeaders(trimmedToken),
      "Content-Type": "application/json",
    },
    body: JSON.stringify(payload),
  });
  if (!response.ok) {
    throw new Error(gitlabApiErrorMessage(response.status));
  }

  return toMergeRequestSummary(
    (await response.json()) as GitlabMergeRequestResponse,
  );
}

export async function fetchGitlabMergeRequestDetails(
  remote: HostingRemote,
  iid: number,
  token: string | null,
  fetcher: typeof fetch = fetch,
): Promise<GitlabMergeRequestDetails> {
  const detailResponse = await fetcher(
    buildGitlabMergeRequestApiUrl(remote, iid),
    { headers: gitlabHeaders(token) },
  );
  if (!detailResponse.ok) {
    throw new Error(gitlabApiErrorMessage(detailResponse.status));
  }
  const detail = (await detailResponse.json()) as GitlabMergeRequestResponse;

  const pipelinesResponse = await fetcher(
    buildGitlabMergeRequestPipelinesApiUrl(remote, iid),
    { headers: gitlabHeaders(token) },
  );
  if (!pipelinesResponse.ok) {
    throw new Error(gitlabApiErrorMessage(pipelinesResponse.status));
  }
  const pipelines = (await pipelinesResponse.json()) as GitlabPipelineResponse[];
  const latestPipeline = pipelines[0] ? toPipelineSummary(pipelines[0]) : null;

  let pipelineJobs: GitlabPipelineJobSummary[] = [];
  if (latestPipeline) {
    const jobsResponse = await fetcher(
      buildGitlabPipelineJobsApiUrl(remote, latestPipeline.id),
      { headers: gitlabHeaders(token) },
    );
    if (jobsResponse.ok) {
      pipelineJobs = ((await jobsResponse.json()) as GitlabPipelineJobResponse[]).map(
        toPipelineJobSummary,
      );
    }
  }

  let approvals: GitlabApprovalSummary | null = null;
  const approvalsResponse = await fetcher(
    buildGitlabMergeRequestApprovalsApiUrl(remote, iid),
    { headers: gitlabHeaders(token) },
  );
  if (approvalsResponse.ok) {
    approvals = toApprovalSummary(
      (await approvalsResponse.json()) as GitlabApprovalsResponse,
    );
  }

  let notes: GitlabMergeRequestNote[] = [];
  const notesResponse = await fetcher(
    buildGitlabMergeRequestNotesApiUrl(remote, iid),
    { headers: gitlabHeaders(token) },
  );
  if (notesResponse.ok) {
    notes = ((await notesResponse.json()) as GitlabMergeRequestNoteResponse[]).map(
      toMergeRequestNote,
    );
  }

  let discussions: GitlabMergeRequestDiscussion[] = [];
  const discussionsResponse = await fetcher(
    buildGitlabMergeRequestDiscussionsApiUrl(remote, iid),
    { headers: gitlabHeaders(token) },
  );
  if (discussionsResponse.ok) {
    discussions = ((await discussionsResponse.json()) as GitlabDiscussionResponse[])
      .map(toMergeRequestDiscussion)
      .filter((discussion): discussion is GitlabMergeRequestDiscussion =>
        Boolean(discussion),
      );
  }

  return {
    ...toMergeRequestSummary(detail),
    headSha: detail.sha ?? "",
    changesCount: detail.changes_count ?? "",
    userNotesCount: detail.user_notes_count ?? 0,
    blockingDiscussionsResolved:
      detail.blocking_discussions_resolved ?? null,
    hasConflicts: detail.has_conflicts ?? false,
    upvotes: detail.upvotes ?? 0,
    downvotes: detail.downvotes ?? 0,
    latestPipeline,
    pipelineJobs,
    approvals,
    notes,
    discussions,
  };
}

export async function mergeGitlabMergeRequest(
  remote: HostingRemote,
  iid: number,
  input: MergeGitlabMergeRequestInput,
  token: string | null,
  fetcher: typeof fetch = fetch,
): Promise<GitlabMergeRequestSummary> {
  const trimmedToken = token?.trim();
  if (!trimmedToken) throw new Error("GitLab token is required");
  const headSha = input.headSha.trim();
  if (!headSha) throw new Error("MR head SHA is required");

  const response = await fetcher(
    buildGitlabMergeRequestMergeApiUrl(remote, iid),
    {
      method: "PUT",
      headers: {
        ...gitlabHeaders(trimmedToken),
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        sha: headSha,
        squash: input.squash,
      }),
    },
  );
  if (!response.ok) {
    throw new Error(gitlabApiErrorMessage(response.status));
  }

  return toMergeRequestSummary(
    (await response.json()) as GitlabMergeRequestResponse,
  );
}

export async function retryGitlabJob(
  remote: HostingRemote,
  jobId: number,
  token: string | null,
  fetcher: typeof fetch = fetch,
): Promise<GitlabPipelineJobSummary> {
  const trimmedToken = token?.trim();
  if (!trimmedToken) throw new Error("GitLab token is required");

  const response = await fetcher(buildGitlabRetryJobApiUrl(remote, jobId), {
    method: "POST",
    headers: gitlabHeaders(trimmedToken),
  });
  if (!response.ok) {
    throw new Error(gitlabApiErrorMessage(response.status));
  }

  return toPipelineJobSummary(
    (await response.json()) as GitlabPipelineJobResponse,
  );
}

export async function createGitlabMergeRequestNote(
  remote: HostingRemote,
  iid: number,
  body: string,
  token: string | null,
  fetcher: typeof fetch = fetch,
): Promise<GitlabMergeRequestNote> {
  const trimmedBody = body.trim();
  if (!trimmedBody) throw new Error("MR note cannot be empty");
  const trimmedToken = token?.trim();
  if (!trimmedToken) throw new Error("GitLab token is required");

  const response = await fetcher(
    buildGitlabMergeRequestNoteCreateApiUrl(remote, iid),
    {
      method: "POST",
      headers: {
        ...gitlabHeaders(trimmedToken),
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ body: trimmedBody }),
    },
  );
  if (!response.ok) {
    throw new Error(gitlabApiErrorMessage(response.status));
  }

  return toMergeRequestNote(
    (await response.json()) as GitlabMergeRequestNoteResponse,
  );
}

export async function approveGitlabMergeRequest(
  remote: HostingRemote,
  iid: number,
  token: string | null,
  fetcher: typeof fetch = fetch,
): Promise<GitlabApprovalSummary> {
  const trimmedToken = token?.trim();
  if (!trimmedToken) throw new Error("GitLab token is required");

  const response = await fetcher(
    buildGitlabMergeRequestApproveApiUrl(remote, iid),
    {
      method: "POST",
      headers: gitlabHeaders(trimmedToken),
    },
  );
  if (!response.ok) {
    throw new Error(gitlabApiErrorMessage(response.status));
  }

  return toApprovalSummary((await response.json()) as GitlabApprovalsResponse);
}

export async function unapproveGitlabMergeRequest(
  remote: HostingRemote,
  iid: number,
  token: string | null,
  fetcher: typeof fetch = fetch,
): Promise<GitlabApprovalSummary> {
  const trimmedToken = token?.trim();
  if (!trimmedToken) throw new Error("GitLab token is required");

  const response = await fetcher(
    buildGitlabMergeRequestUnapproveApiUrl(remote, iid),
    {
      method: "POST",
      headers: gitlabHeaders(trimmedToken),
    },
  );
  if (!response.ok) {
    throw new Error(gitlabApiErrorMessage(response.status));
  }

  return toApprovalSummary((await response.json()) as GitlabApprovalsResponse);
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

function normalizeCreateMergeRequestInput(
  input: CreateGitlabMergeRequestInput,
): {
  title: string;
  source_branch: string;
  target_branch: string;
  description: string;
  draft: boolean;
} {
  const title = input.title.trim();
  const sourceBranch = input.sourceBranch.trim();
  const targetBranch = input.targetBranch.trim();
  if (!title) throw new Error("MR title cannot be empty");
  if (!sourceBranch) throw new Error("MR source branch cannot be empty");
  if (!targetBranch) throw new Error("MR target branch cannot be empty");
  return {
    title,
    source_branch: sourceBranch,
    target_branch: targetBranch,
    description: input.description,
    draft: input.draft,
  };
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

function toPipelineSummary(
  pipeline: GitlabPipelineResponse,
): GitlabPipelineSummary {
  return {
    id: pipeline.id,
    status: pipeline.status ?? "",
    ref: pipeline.ref ?? "",
    sha: pipeline.sha ?? "",
    url: pipeline.web_url ?? null,
  };
}

function toPipelineJobSummary(
  job: GitlabPipelineJobResponse,
): GitlabPipelineJobSummary {
  return {
    id: job.id,
    name: job.name ?? "",
    stage: job.stage ?? "",
    status: job.status ?? "",
    duration: job.duration ?? null,
    url: job.web_url ?? null,
    startedAt: job.started_at ?? "",
    finishedAt: job.finished_at ?? "",
  };
}

function toApprovalSummary(
  approvals: GitlabApprovalsResponse,
): GitlabApprovalSummary {
  return {
    approvalsRequired: approvals.approvals_required ?? 0,
    approvalsLeft: approvals.approvals_left ?? 0,
    approved: approvals.approved ?? false,
    approvedBy: (approvals.approved_by ?? [])
      .map((entry) => entry.user?.username ?? "")
      .filter(Boolean),
    userHasApproved: approvals.user_has_approved ?? false,
    userCanApprove: approvals.user_can_approve ?? false,
  };
}

function toMergeRequestNote(
  note: GitlabMergeRequestNoteResponse,
): GitlabMergeRequestNote {
  return {
    id: note.id,
    body: note.body ?? "",
    author: note.author?.username ?? null,
    createdAt: note.created_at ?? "",
    updatedAt: note.updated_at ?? "",
    system: note.system ?? false,
    internal: note.internal ?? false,
  };
}

function toMergeRequestDiscussion(
  discussion: GitlabDiscussionResponse,
): GitlabMergeRequestDiscussion | null {
  const note = (discussion.notes ?? []).find(
    (entry) => !entry.system && entry.body,
  );
  if (!note) return null;
  const position = note.position;
  return {
    id: discussion.id,
    resolvable: note.resolvable ?? false,
    resolved: note.resolved ?? false,
    path: position?.new_path ?? position?.old_path ?? "",
    line: position?.new_line ?? position?.old_line ?? null,
    author: note.author?.username ?? null,
    body: note.body ?? "",
    updatedAt: note.updated_at ?? "",
  };
}
