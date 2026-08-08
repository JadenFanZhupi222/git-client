import type {
  ReviewFindingDto,
  ReviewLanguageDto,
  ReviewRunResultDto,
  ReviewTargetDto,
} from "../bindings";

const CACHE_PREFIX = "pr-review-result-v1";
const GITLAB_CACHE_PREFIX = "gitlab-mr-review-result-v1";

export type ReviewPlatform = "github" | "gitlab";

export type CachedFindingDraft = {
  finding: ReviewFindingDto;
  selected: boolean;
  comment: string;
};

export type CachedReview = {
  version: 1;
  headSha: string;
  modelId: string;
  outputLanguage: ReviewLanguageDto;
  result: ReviewRunResultDto;
  drafts: CachedFindingDraft[];
};

export function loadCachedReview(
  target: ReviewTargetDto,
  expectedHeadSha: string,
  platform: ReviewPlatform = "github",
): CachedReview | null {
  const key = cacheKey(target, platform);
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return null;
    const value = migrateCachedReview(JSON.parse(raw));
    if (!isCachedReview(value) || value.headSha !== expectedHeadSha || value.result.head_sha !== expectedHeadSha) {
      localStorage.removeItem(key);
      return null;
    }
    return value;
  } catch {
    localStorage.removeItem(key);
    return null;
  }
}

function migrateCachedReview(value: unknown): unknown {
  if (!value || typeof value !== "object") return value;
  const candidate = value as { result?: unknown };
  if (!candidate.result || typeof candidate.result !== "object") return value;
  const result = candidate.result as Partial<ReviewRunResultDto>;
  return {
    ...candidate,
    result: {
      ...result,
      model_id: typeof result.model_id === "string" ? result.model_id : "",
      duration_ms: typeof result.duration_ms === "number" ? result.duration_ms : 0,
      diagnostic_id: typeof result.diagnostic_id === "string" ? result.diagnostic_id : "",
      provider_attempts: typeof result.provider_attempts === "number" ? result.provider_attempts : 0,
    },
  };
}

export function saveCachedReview(
  target: ReviewTargetDto,
  value: CachedReview,
  platform: ReviewPlatform = "github",
): void {
  try {
    localStorage.setItem(cacheKey(target, platform), JSON.stringify(value));
  } catch {
    // Review remains usable in memory if storage is unavailable or full.
  }
}

export function clearCachedReview(
  target: ReviewTargetDto,
  platform: ReviewPlatform = "github",
): void {
  try {
    localStorage.removeItem(cacheKey(target, platform));
  } catch {
    // Storage cleanup is best-effort.
  }
}

function cacheKey(target: ReviewTargetDto, platform: ReviewPlatform): string {
  const prefix = platform === "gitlab" ? GITLAB_CACHE_PREFIX : CACHE_PREFIX;
  return `${prefix}:${encodeURIComponent(target.owner)}/${encodeURIComponent(target.repo)}#${target.pull_number}`;
}

function isCachedReview(value: unknown): value is CachedReview {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<CachedReview>;
  return candidate.version === 1
    && typeof candidate.headSha === "string"
    && typeof candidate.modelId === "string"
    && (candidate.outputLanguage === "simplified_chinese" || candidate.outputLanguage === "english")
    && isReviewResult(candidate.result)
    && Array.isArray(candidate.drafts)
    && candidate.drafts.every(isFindingDraft);
}

function isReviewResult(value: unknown): value is ReviewRunResultDto {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<ReviewRunResultDto>;
  return typeof candidate.run_id === "string"
    && typeof candidate.head_sha === "string"
    && typeof candidate.summary === "string"
    && Array.isArray(candidate.reviewed_files)
    && candidate.reviewed_files.every((path) => typeof path === "string")
    && Array.isArray(candidate.findings)
    && candidate.findings.every(isFinding)
    && Boolean(candidate.usage)
    && typeof candidate.usage?.input_tokens === "number"
    && typeof candidate.usage?.output_tokens === "number"
    && typeof candidate.usage?.tool_calls === "number"
    && typeof candidate.model_id === "string"
    && typeof candidate.duration_ms === "number"
    && typeof candidate.diagnostic_id === "string"
    && typeof candidate.provider_attempts === "number";
}

function isFindingDraft(value: unknown): value is CachedFindingDraft {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<CachedFindingDraft>;
  return typeof candidate.selected === "boolean"
    && typeof candidate.comment === "string"
    && isFinding(candidate.finding);
}

function isFinding(value: unknown): value is ReviewFindingDto {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<ReviewFindingDto>;
  return typeof candidate.id === "string"
    && typeof candidate.severity === "string"
    && typeof candidate.path === "string"
    && typeof candidate.side === "string"
    && typeof candidate.line === "number"
    && typeof candidate.title === "string"
    && typeof candidate.failure_scenario === "string"
    && typeof candidate.explanation === "string"
    && typeof candidate.draft_comment === "string";
}
