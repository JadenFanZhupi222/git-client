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

  it("drops malformed storage and supports explicit cleanup", () => {
    localStorage.setItem("pr-review-result-v1:acme/rocket#17", "not-json");
    expect(loadCachedReview(target, "abc123")).toBeNull();
    clearCachedReview(target);
    expect(localStorage.length).toBe(0);
  });
});
