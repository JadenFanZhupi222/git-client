import type { ReviewModelOptionDto, ReviewUsageDto } from "../bindings";

export function estimatedRunCost(
  usage: ReviewUsageDto,
  modelId: string,
  models: ReviewModelOptionDto[],
): { currency: string; micros: number } | null {
  const pricing = models.find((model) => model.id === modelId)?.pricing;
  if (!pricing) return null;
  const micros = Math.ceil(
    (usage.input_tokens * pricing.input_cache_miss_per_million_micros
      + usage.output_tokens * pricing.output_per_million_micros) / 1_000_000,
  );
  return { currency: pricing.currency, micros };
}

export function formatEstimatedCost(
  cost: { currency: string; micros: number },
  locale: string,
): string {
  return new Intl.NumberFormat(locale, {
    style: "currency",
    currency: cost.currency,
    minimumFractionDigits: 2,
    maximumFractionDigits: 6,
  }).format(cost.micros / 1_000_000);
}
