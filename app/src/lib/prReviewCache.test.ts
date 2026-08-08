import { beforeEach, describe, expect, it } from "vitest";
import type { ReviewRunResultDto } from "../bindings";
import { clearCachedReview, loadCachedReview, saveCachedReview } from "./prReviewCache";

const target = { owner: "acme", repo: "rocket", pull_number: 17 };
const result: ReviewRunResultDto = {
  run_id: "run-1",
  head_sha: "abc123",
  summary: "No issues.",
  reviewed_files: ["src/a.ts"],
  findings: [],
  usage: { input_tokens: 12, output_tokens: 3, tool_calls: 0 },
  model_id: "deepseek-v4-flash",
  duration_ms: 840,
  diagnostic_id: "diag-0123456789abcdef",
  provider_attempts: 1,
};

describe("prReviewCache", () => {
  beforeEach(() => localStorage.clear());

  it("restores a valid review pinned to the expected head", () => {
    saveCachedReview(target, {
      version: 1,
      headSha: "abc123",
      modelId: "deepseek-v4-flash",
      outputLanguage: "english",
      result,
      drafts: [],
    });

    expect(loadCachedReview(target, "abc123")?.result).toEqual(result);
  });

  it("invalidates cached output when the pull request head changes", () => {
    saveCachedReview(target, {
      version: 1,
      headSha: "abc123",
      modelId: "deepseek-v4-flash",
      outputLanguage: "english",
      result,
      drafts: [],
    });

    expect(loadCachedReview(target, "def456")).toBeNull();
    expect(loadCachedReview(target, "abc123")).toBeNull();
  });

  it("migrates a valid v1 result saved before diagnostics were added", () => {
    const { model_id: _model, duration_ms: _duration, diagnostic_id: _diagnostic, provider_attempts: _attempts, ...legacyResult } = result;
    localStorage.setItem("pr-review-result-v1:acme/rocket#17", JSON.stringify({
      version: 1,
      headSha: "abc123",
      modelId: "deepseek-v4-flash",
      outputLanguage: "english",
      result: legacyResult,
      drafts: [],
    }));

    expect(loadCachedReview(target, "abc123")?.result).toMatchObject({
      duration_ms: 0,
      diagnostic_id: "",
      provider_attempts: 0,
      model_id: "",
    });
  });

  it("drops malformed storage and supports explicit cleanup", () => {
    localStorage.setItem("pr-review-result-v1:acme/rocket#17", "not-json");
    expect(loadCachedReview(target, "abc123")).toBeNull();
    clearCachedReview(target);
    expect(localStorage.length).toBe(0);
  });

  it("isolates GitHub pull request and GitLab merge request caches", () => {
    const cached = {
      version: 1 as const,
      headSha: "abc123",
      modelId: "deepseek-v4-flash",
      outputLanguage: "english" as const,
      result,
      drafts: [],
    };
    saveCachedReview(target, cached, "gitlab");

    expect(loadCachedReview(target, "abc123", "github")).toBeNull();
    expect(loadCachedReview(target, "abc123", "gitlab")?.result).toEqual(result);
    clearCachedReview(target, "gitlab");
    expect(loadCachedReview(target, "abc123", "gitlab")).toBeNull();
  });
});
