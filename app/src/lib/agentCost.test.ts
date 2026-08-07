import { describe, expect, it } from "vitest";
import type { ReviewModelOptionDto } from "../bindings";
import { estimatedRunCost, formatEstimatedCost } from "./agentCost";

const model: ReviewModelOptionDto = {
  id: "priced-model",
  label: "Priced",
  provider: "Fixture",
  provider_id: "fixture",
  capabilities: {
    supports_tool_calling: true,
    supports_structured_output: true,
    reports_usage: true,
    context_window_tokens: 100_000,
    max_output_tokens: 8_000,
  },
  pricing: {
    currency: "USD",
    input_cache_hit_per_million_micros: 2_800,
    input_cache_miss_per_million_micros: 140_000,
    output_per_million_micros: 280_000,
    source_url: "https://example.test/pricing",
    source_version: "fixture-v1",
    checked_at: "2026-08-07",
  },
};

describe("agent cost", () => {
  it("uses actual tokens and the conservative cache-miss input price", () => {
    expect(estimatedRunCost(
      { input_tokens: 1_000_000, output_tokens: 500_000, tool_calls: 2 },
      "priced-model",
      [model],
    )).toEqual({ currency: "USD", micros: 280_000 });
  });

  it("does not invent a cost when result pricing is unavailable", () => {
    expect(estimatedRunCost(
      { input_tokens: 10, output_tokens: 5, tool_calls: 0 },
      "legacy-model",
      [model],
    )).toBeNull();
  });

  it("formats micro-currency without rounding tiny runs to zero", () => {
    expect(formatEstimatedCost({ currency: "USD", micros: 1 }, "en-US")).toBe("$0.000001");
  });
});
