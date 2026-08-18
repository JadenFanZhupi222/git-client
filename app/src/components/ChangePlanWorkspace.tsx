import { useEffect, useRef, useState } from "react";
import type {
  AgentIpcErrorDto,
  ChangePlanResultDto,
  CredentialKindDto,
  ReviewModelOptionDto,
} from "../bindings";
import {
  analyzeChangePlan,
  cancelChangePlan,
  commitChangeGroup,
  listReviewModels,
} from "../ipc";
import { useT } from "../lib/i18n";
import { useAgentStream } from "../hooks/useAgentStream";
import { AgentModelPicker } from "./AgentModelPicker";
import { AgentStreamPanel } from "./AgentStreamPanel";
import { AlertIcon, CheckIcon, CloseIcon, CommitIcon, RefreshIcon, SpinnerIcon } from "./icons";
import { useToast } from "./Toast";
import { Button } from "./ui/Button";

const MODEL_CONSENT_KEY = "change-plan-model-consent-v1";

type Phase = "loading" | "results" | "committing";

export function ChangePlanWorkspace({
  repo,
  onClose,
  onCommitted,
  onConfigureCredential = () => undefined,
}: {
  repo: string;
  onClose: () => void;
  onCommitted: () => void;
  onConfigureCredential?: (kind: CredentialKindDto) => void;
}) {
  const t = useT();
  const toast = useToast();
  const mountedRef = useRef(true);
  const runIdRef = useRef<string | null>(null);
  const [phase, setPhase] = useState<Phase>("loading");
  const [plan, setPlan] = useState<ChangePlanResultDto | null>(null);
  const [error, setError] = useState<AgentIpcErrorDto | null>(null);
  const [messages, setMessages] = useState<Record<string, string>>({});
  const [confirmed, setConfirmed] = useState<Set<string>>(new Set());
  const [committingGroup, setCommittingGroup] = useState<string | null>(null);
  const [models, setModels] = useState<ReviewModelOptionDto[]>([]);
  const [selectedModelId, setSelectedModelId] = useState("");
  const [showEnhancement, setShowEnhancement] = useState(false);
  const [consented, setConsented] = useState(() => localStorage.getItem(MODEL_CONSENT_KEY) === "accepted");
  const agentStream = useAgentStream();

  useEffect(() => {
    mountedRef.current = true;
    void listReviewModels()
      .then((next) => {
        if (!mountedRef.current) return;
        const compatible = next.filter((model) => model.capabilities.supports_structured_output);
        setModels(compatible);
        setSelectedModelId((current) => compatible.some((model) => model.id === current)
          ? current
          : (compatible[0]?.id ?? ""));
      })
      .catch(() => undefined);
    void runPlan(null);
    return () => {
      mountedRef.current = false;
      const runId = runIdRef.current;
      runIdRef.current = null;
      if (runId) void cancelChangePlan(runId).catch(() => undefined);
    };
    // The repository prop identifies a fresh workspace lifecycle.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [repo]);

  async function runPlan(modelId: string | null) {
    const previousRun = runIdRef.current;
    if (previousRun) void cancelChangePlan(previousRun).catch(() => undefined);
    const runId = createRunId();
    runIdRef.current = runId;
    setPhase("loading");
    setError(null);
    try {
      if (modelId) await agentStream.begin(runId);
      else agentStream.reset();
      const next = await analyzeChangePlan({ run_id: runId, repo_path: repo, model_id: modelId });
      if (!mountedRef.current || runIdRef.current !== runId) return;
      setPlan(next);
      setMessages(Object.fromEntries(next.groups.map((group) => [group.id, group.commit_message])));
      setConfirmed(new Set());
      setPhase("results");
      if (modelId) agentStream.finish("completed");
    } catch (reason) {
      if (!mountedRef.current || runIdRef.current !== runId) return;
      setError(asAgentError(reason));
      setPhase("results");
      if (modelId) agentStream.finish("failed");
    } finally {
      if (runIdRef.current === runId) runIdRef.current = null;
    }
  }

  function acceptConsent(checked: boolean) {
    setConsented(checked);
    if (checked) localStorage.setItem(MODEL_CONSENT_KEY, "accepted");
    else localStorage.removeItem(MODEL_CONSENT_KEY);
  }

  async function executeGroup(groupId: string) {
    if (!plan || phase !== "results" || !confirmed.has(groupId)) return;
    setPhase("committing");
    setCommittingGroup(groupId);
    setError(null);
    try {
      const result = await commitChangeGroup({
        run_id: createRunId(),
        repo_path: repo,
        snapshot_id: plan.snapshot_id,
        group_id: groupId,
        commit_message: messages[groupId] ?? "",
        confirmed: true,
      });
      toast({ kind: "success", title: t("changePlan.committed", { sha: result.sha.slice(0, 7) }) });
      onCommitted();
      await runPlan(null);
    } catch (reason) {
      setError(asAgentError(reason));
      setPhase("results");
    } finally {
      setCommittingGroup(null);
    }
  }

  const busy = phase === "loading" || phase === "committing";
  return (
    <section className="flex min-h-0 flex-1 flex-col" aria-label={t("changePlan.title")}>
      <header className="flex h-[41px] shrink-0 items-center gap-2 border-b border-line px-3">
        <CommitIcon width={14} height={14} className="text-accent" />
        <div className="min-w-0 flex-1">
          <div className="text-[12px] font-semibold text-fg">{t("changePlan.title")}</div>
          <div className="text-[10.5px] text-fg-subtle">{t("changePlan.localBadge")}</div>
        </div>
        <Button
          type="button"
          variant="secondary"
          size="chip"
          disabled={busy || !plan}
          onClick={() => setShowEnhancement((current) => !current)}
        >
          {t("changePlan.enhance")}
        </Button>
        <button
          type="button"
          onClick={onClose}
          disabled={phase === "committing"}
          aria-label={t("changePlan.close")}
          className="grid h-7 w-7 place-items-center rounded-md text-fg-muted hover:bg-overlay hover:text-fg disabled:opacity-40"
        >
          <CloseIcon width={13} height={13} />
        </button>
      </header>

      {showEnhancement && (
        <div className="shrink-0 border-b border-line bg-elevated/40 px-4 py-3">
          <div className="flex items-start gap-4">
            <div className="min-w-0 flex-1">
              <AgentModelPicker
                id="change-plan-model"
                label={t("changePlan.model")}
                models={models}
                value={selectedModelId}
                onChange={setSelectedModelId}
                onConfigureCredential={onConfigureCredential}
              />
            </div>
            <div className="w-64 shrink-0 pt-5">
              <label className="flex items-start gap-2 text-[11px] leading-relaxed text-fg-muted">
                <input
                  type="checkbox"
                  checked={consented}
                  onChange={(event) => acceptConsent(event.currentTarget.checked)}
                  className="mt-0.5 accent-accent"
                />
                {t("changePlan.consent")}
              </label>
              <Button
                type="button"
                size="sm"
                disabled={busy || !consented || !selectedModelId}
                onClick={() => void runPlan(selectedModelId)}
                className="mt-2 w-full"
              >
                {busy ? <SpinnerIcon width={13} height={13} /> : null}
                {t("changePlan.runEnhancement")}
              </Button>
            </div>
          </div>
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto">
        {phase === "loading" && agentStream.stream && (
          <div className="mx-auto w-full max-w-4xl px-5 pt-5">
            <AgentStreamPanel stream={agentStream.stream} />
          </div>
        )}
        {phase === "loading" && !plan ? (
          <div className="grid h-full place-items-center text-xs text-fg-muted">
            <span className="flex items-center gap-2"><SpinnerIcon width={14} height={14} />{t("changePlan.analyzing")}</span>
          </div>
        ) : (
          <div className="mx-auto w-full max-w-4xl px-5 py-5">
            {error && (
              <div className="mb-4 flex items-start gap-2 border-y border-danger/30 bg-danger/[0.06] px-3 py-2.5 text-xs text-danger" role="alert">
                <AlertIcon width={14} height={14} className="mt-0.5 shrink-0" />
                <div className="min-w-0 flex-1">
                  <div>{error.message}</div>
                  <div className="mt-1 font-mono text-[10px] opacity-70">{error.code} · {error.diagnostic_id}</div>
                </div>
                <button type="button" onClick={() => void runPlan(null)} className="flex shrink-0 items-center gap-1 text-[11px] font-medium">
                  <RefreshIcon width={12} height={12} />{t("changePlan.refresh")}
                </button>
              </div>
            )}

            {plan && (
              <>
                <div className="mb-5 flex items-start justify-between gap-4">
                  <div>
                    <h2 className="text-[15px] font-semibold text-fg">{plan.summary}</h2>
                    <p className="mt-1 text-[11px] text-fg-muted">
                      {plan.enhanced ? t("changePlan.enhancedSummary") : t("changePlan.localSummary")}
                    </p>
                  </div>
                  <button type="button" disabled={busy} onClick={() => void runPlan(null)} className="flex shrink-0 items-center gap-1.5 text-[11px] text-fg-muted hover:text-fg disabled:opacity-40">
                    <RefreshIcon width={12} height={12} />{t("changePlan.refresh")}
                  </button>
                </div>

                {plan.warnings.length > 0 && (
                  <section className="mb-5 border-y border-line" aria-label={t("changePlan.warnings")}>
                    {plan.warnings.map((warning, index) => (
                      <div key={`${warning.code}-${index}`} className="flex gap-2 border-b border-line px-1 py-2.5 last:border-b-0">
                        <AlertIcon width={13} height={13} className={`mt-0.5 shrink-0 ${warning.severity === "blocker" ? "text-danger" : warning.severity === "warning" ? "text-warning" : "text-fg-subtle"}`} />
                        <div className="min-w-0 text-[11.5px] leading-relaxed text-fg-muted">
                          <span className="text-fg">{warning.message}</span>
                          {warning.paths.length > 0 && <div className="mt-0.5 truncate font-mono text-[10.5px] text-fg-subtle" title={warning.paths.join("\n")}>{warning.paths.join(", ")}</div>}
                        </div>
                      </div>
                    ))}
                  </section>
                )}

                <div className="flex items-center justify-between border-b border-line pb-2">
                  <h3 className="text-[11px] font-semibold uppercase tracking-[0.05em] text-fg-muted">{t("changePlan.groups")}</h3>
                  <span className="font-mono text-[10.5px] text-fg-subtle">{plan.groups.length}</span>
                </div>
                {plan.groups.length === 0 ? (
                  <div className="py-12 text-center text-xs text-fg-muted">{t("changePlan.clean")}</div>
                ) : plan.groups.map((group, index) => {
                  const checked = confirmed.has(group.id);
                  const committing = committingGroup === group.id;
                  return (
                    <section key={group.id} className="border-b border-line py-4" aria-labelledby={`change-group-${group.id}`}>
                      <div className="flex items-start gap-3">
                        <span className="mt-0.5 grid h-5 w-5 shrink-0 place-items-center rounded-full bg-overlay font-mono text-[10px] text-fg-muted">{index + 1}</span>
                        <div className="min-w-0 flex-1">
                          <div className="flex items-baseline gap-2">
                            <h4 id={`change-group-${group.id}`} className="text-[13px] font-semibold text-fg">{group.title}</h4>
                            <span className="text-[10.5px] text-fg-subtle">{t("changePlan.fileCount", { n: group.files.length })}</span>
                          </div>
                          <p className="mt-1 text-[11.5px] leading-relaxed text-fg-muted">{group.rationale}</p>
                          <ul className="mt-2 divide-y divide-line/70 border-y border-line/70">
                            {group.files.map((file) => (
                              <li key={`${file.path}-${file.staged}`} className="flex items-center gap-2 px-1 py-1.5 font-mono text-[11px]">
                                <span className="w-4 text-center text-fg-subtle">{file.staged ? "S" : "W"}</span>
                                <span className="min-w-0 flex-1 truncate text-fg-muted" title={file.path}>{file.path}</span>
                                <span className="shrink-0 text-success">+{file.additions}</span>
                                <span className="shrink-0 text-danger">-{file.deletions}</span>
                              </li>
                            ))}
                          </ul>
                          <label className="mt-3 block text-[11px] font-medium text-fg-muted" htmlFor={`change-message-${group.id}`}>
                            {t("changePlan.commitMessage")}
                          </label>
                          <input
                            id={`change-message-${group.id}`}
                            value={messages[group.id] ?? ""}
                            onChange={(event) => {
                              const value = event.currentTarget.value;
                              setMessages((current) => ({ ...current, [group.id]: value }));
                            }}
                            disabled={!group.executable || busy}
                            className="field mt-1 h-8 w-full rounded-md border border-line bg-canvas px-2.5 font-mono text-[11.5px] text-fg disabled:opacity-50"
                          />
                          {group.blocked_reason ? (
                            <p className="mt-2 flex items-start gap-1.5 text-[11px] text-warning"><AlertIcon width={12} height={12} className="mt-0.5 shrink-0" />{group.blocked_reason}</p>
                          ) : (
                            <div className="mt-3 flex items-center gap-3">
                              <label className="flex min-w-0 flex-1 items-start gap-2 text-[11px] leading-relaxed text-fg-muted">
                                <input
                                  type="checkbox"
                                  checked={checked}
                                  disabled={busy}
                                  onChange={(event) => {
                                    const checkedNow = event.currentTarget.checked;
                                    setConfirmed((current) => toggleSetValue(current, group.id, checkedNow));
                                  }}
                                  className="mt-0.5 accent-accent"
                                />
                                {t("changePlan.confirm", { n: group.files.length })}
                              </label>
                              <Button
                                type="button"
                                variant="commit"
                                size="sm"
                                disabled={busy || !checked || !(messages[group.id] ?? "").trim()}
                                onClick={() => void executeGroup(group.id)}
                              >
                                {committing ? <SpinnerIcon width={13} height={13} /> : <CheckIcon width={13} height={13} />}
                                {t("changePlan.commitGroup")}
                              </Button>
                            </div>
                          )}
                        </div>
                      </div>
                    </section>
                  );
                })}
              </>
            )}
          </div>
        )}
      </div>
    </section>
  );
}

function toggleSetValue(current: Set<string>, value: string, enabled: boolean): Set<string> {
  const next = new Set(current);
  if (enabled) next.add(value);
  else next.delete(value);
  return next;
}

function createRunId(): string {
  return typeof crypto.randomUUID === "function"
    ? crypto.randomUUID()
    : `change-plan-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function asAgentError(reason: unknown): AgentIpcErrorDto {
  if (reason && typeof reason === "object" && "code" in reason) {
    const error = reason as Partial<AgentIpcErrorDto>;
    return {
      code: error.code ?? "CHANGE_PLAN_FAILED",
      message: error.message ?? "Change planning failed",
      recoverable: error.recoverable ?? true,
      diagnostic_id: error.diagnostic_id ?? "change-plan",
    };
  }
  return {
    code: "CHANGE_PLAN_FAILED",
    message: reason instanceof Error ? reason.message : String(reason),
    recoverable: true,
    diagnostic_id: "change-plan",
  };
}
