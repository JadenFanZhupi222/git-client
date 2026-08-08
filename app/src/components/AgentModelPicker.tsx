import { useEffect, useMemo, useState } from "react";
import type { CredentialKindDto, ReviewModelOptionDto } from "../bindings";
import { credentialStatus } from "../ipc";
import { useT } from "../lib/i18n";
import { Button } from "./ui/Button";

type AiCredentialKind = Extract<CredentialKindDto, "deepseek" | "openai" | "anthropic">;
type CredentialState = "checking" | "configured" | "missing" | "unavailable";

export function AgentModelPicker({
  id,
  label,
  models,
  value,
  onChange,
  onConfigureCredential,
}: {
  id: string;
  label: string;
  models: ReviewModelOptionDto[];
  value: string;
  onChange: (modelId: string) => void;
  onConfigureCredential: (kind: CredentialKindDto) => void;
}) {
  const t = useT();
  const groups = useMemo(() => groupModels(models), [models]);
  const providerKinds = useMemo(
    () => Array.from(new Set(models.map((model) => providerCredentialKind(model.provider_id)).filter(isPresent))),
    [models],
  );
  const [credentialStates, setCredentialStates] = useState<Partial<Record<AiCredentialKind, CredentialState>>>({});
  const selected = models.find((model) => model.id === value) ?? null;
  const selectedCredentialKind = selected ? providerCredentialKind(selected.provider_id) : null;
  const selectedCredentialState = selectedCredentialKind
    ? (credentialStates[selectedCredentialKind] ?? "checking")
    : "unavailable";

  useEffect(() => {
    let active = true;
    setCredentialStates(Object.fromEntries(providerKinds.map((kind) => [kind, "checking"])));
    void Promise.allSettled(providerKinds.map(async (kind) => [kind, await credentialStatus(kind)] as const))
      .then((results) => {
        if (!active) return;
        const next: Partial<Record<AiCredentialKind, CredentialState>> = {};
        results.forEach((result, index) => {
          const kind = providerKinds[index];
          next[kind] = result.status === "fulfilled"
            ? result.value[1]
              ? "configured"
              : "missing"
            : "unavailable";
        });
        setCredentialStates(next);
      });
    return () => {
      active = false;
    };
  }, [providerKinds]);

  return (
    <div className="grid min-w-52 gap-1.5">
      <label htmlFor={id} className="text-[11px] font-medium text-fg-muted">
        {label}
      </label>
      <select
        id={id}
        value={value}
        disabled={models.length === 0}
        onChange={(event) => onChange(event.currentTarget.value)}
        className="field h-8 rounded-md border border-line bg-canvas px-2 text-xs font-normal text-fg disabled:opacity-50"
      >
        {groups.map(([provider, providerModels]) => (
          <optgroup key={provider} label={provider}>
            {providerModels.map((model) => (
              <option key={model.id} value={model.id}>
                {model.label}
              </option>
            ))}
          </optgroup>
        ))}
      </select>

      {selected ? (
        <div className="mt-1 rounded-md bg-overlay px-3 py-2.5 text-[11px]" aria-live="polite">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <span className="font-medium text-fg">{selected.provider}</span>
            <span className="font-mono text-fg-muted">{selected.id}</span>
            <CredentialBadge state={selectedCredentialState} />
          </div>
          <ModelFacts model={selected} className="mt-2" />
          {selectedCredentialKind && selectedCredentialState !== "configured" && selectedCredentialState !== "checking" && (
            <div className="mt-2 flex items-center justify-between gap-3 border-t border-line pt-2">
              <span className="leading-relaxed text-fg-muted">
                {selectedCredentialState === "missing"
                  ? t("agentModel.credentialMissing")
                  : t("agentModel.credentialUnavailable")}
              </span>
              <Button
                type="button"
                variant="secondary"
                size="chip"
                onClick={() => onConfigureCredential(selectedCredentialKind)}
                className="shrink-0"
              >
                {t("agentModel.configureProvider", { provider: selected.provider })}
              </Button>
            </div>
          )}
        </div>
      ) : (
        <p className="text-[11px] text-fg-muted">{t("agentModel.noneAvailable")}</p>
      )}
    </div>
  );
}

export function CompatibleModelList({ models }: { models: ReviewModelOptionDto[] }) {
  const t = useT();
  if (models.length === 0) {
    return <p className="mt-2 text-xs text-fg-muted">{t("agentModel.noneAvailable")}</p>;
  }

  return (
    <ul className="mt-2 divide-y divide-line border-y border-line" aria-label={t("settings.ai.compatibleModels")}>
      {models.map((model) => (
        <li key={model.id} className="py-2.5 first:pt-2 last:pb-2">
          <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
            <span className="text-xs font-medium text-fg">{model.label}</span>
            <span className="font-mono text-[11px] text-fg-muted">{model.id}</span>
          </div>
          <ModelFacts model={model} className="mt-1.5" />
        </li>
      ))}
    </ul>
  );
}

function ModelFacts({ model, className = "" }: { model: ReviewModelOptionDto; className?: string }) {
  const t = useT();
  const facts = [
    t("agentModel.context", { tokens: formatTokenCount(model.capabilities.context_window_tokens) }),
    t("agentModel.output", { tokens: formatTokenCount(model.capabilities.max_output_tokens) }),
    model.capabilities.supports_tool_calling ? t("agentModel.tools") : null,
    model.capabilities.supports_structured_output ? t("agentModel.structuredOutput") : null,
  ].filter(isPresent);
  return (
    <div className={`flex flex-wrap items-center gap-x-2 gap-y-1 text-[11px] text-fg-muted ${className}`}>
      {facts.map((fact) => <span key={fact}>{fact}</span>)}
      {model.pricing && (
        <span className="font-mono text-fg">
          {t("agentModel.price", {
            input: formatUsd(model.pricing.input_cache_miss_per_million_micros),
            output: formatUsd(model.pricing.output_per_million_micros),
          })}
        </span>
      )}
    </div>
  );
}

function CredentialBadge({ state }: { state: CredentialState }) {
  const t = useT();
  const label = state === "configured"
    ? t("agentModel.status.configured")
    : state === "missing"
      ? t("agentModel.status.notConfigured")
      : state === "unavailable"
        ? t("agentModel.status.unavailable")
        : t("agentModel.status.checking");
  return (
    <span className={`ml-auto rounded-full px-1.5 py-0.5 font-medium ${
      state === "configured" ? "bg-success/15 text-success" : "bg-canvas text-fg-muted"
    }`}>
      {label}
    </span>
  );
}

function providerCredentialKind(providerId: string): AiCredentialKind | null {
  return providerId === "deepseek" || providerId === "openai" || providerId === "anthropic"
    ? providerId
    : null;
}

function groupModels(models: ReviewModelOptionDto[]): [string, ReviewModelOptionDto[]][] {
  const groups = new Map<string, ReviewModelOptionDto[]>();
  for (const model of models) {
    const current = groups.get(model.provider) ?? [];
    current.push(model);
    groups.set(model.provider, current);
  }
  return Array.from(groups.entries());
}

function formatTokenCount(tokens: number): string {
  if (tokens >= 1_000_000) return `${Number((tokens / 1_000_000).toFixed(2))}M`;
  if (tokens >= 1_000) return `${Number((tokens / 1_000).toFixed(0))}K`;
  return String(tokens);
}

function formatUsd(micros: number): string {
  const dollars = micros / 1_000_000;
  return `$${Number(dollars.toFixed(2))}`;
}

function isPresent<T>(value: T | null | undefined): value is T {
  return value !== null && value !== undefined;
}
