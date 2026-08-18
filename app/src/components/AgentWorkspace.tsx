import { useEffect, useMemo, useRef, useState } from "react";
import type { AgentGoalSnapshotDto, AgentIpcErrorDto, AgentSessionSnapshotDto, CredentialKindDto, ReviewModelOptionDto } from "../bindings";
import {
  cancelAgentGoal,
  createAgentGoal,
  extendAgentBudget,
  getAgentSession,
  listenAgentGoalEvents,
  listReviewModels,
  pauseAgentGoal,
  resetAgentSession,
  resumeAgentGoal,
  steerAgentGoal,
} from "../ipc";
import { useAgentStream } from "../hooks/useAgentStream";
import { useT } from "../lib/i18n";
import { AgentStreamPanel } from "./AgentStreamPanel";
import { AgentModelPicker } from "./AgentModelPicker";
import { AgentIcon, AlertIcon, SpinnerIcon } from "./icons";
import { Button } from "./ui/Button";

const CONSENT_KEY = "versionarc.agent.provider-consent.v1";
const TERMINAL = new Set(["completed", "failed", "cancelled"]);
const STREAMING = new Set(["queued", "running", "awaiting_approval", "pausing"]);

export function AgentWorkspace({ repo, onConfigureCredential = () => undefined }: {
  repo: string;
  onConfigureCredential?: (kind: CredentialKindDto) => void;
}) {
  const t = useT();
  const mountedRef = useRef(false);
  const [session, setSession] = useState<AgentSessionSnapshotDto | null>(null);
  const [models, setModels] = useState<ReviewModelOptionDto[]>([]);
  const [selectedModelId, setSelectedModelId] = useState("");
  const [message, setMessage] = useState("");
  const [steeringEchoes, setSteeringEchoes] = useState<string[]>([]);
  const [error, setError] = useState<AgentIpcErrorDto | null>(null);
  const [loading, setLoading] = useState(true);
  const [acting, setActing] = useState(false);
  const [budgetOpen, setBudgetOpen] = useState(false);
  const [budgetDraft, setBudgetDraft] = useState("");
  const [consented, setConsented] = useState(() => localStorage.getItem(CONSENT_KEY) === "accepted");
  const agentStream = useAgentStream();
  const goal = session?.active_goal ?? null;
  const active = Boolean(goal && !TERMINAL.has(goal.status));

  async function refresh() {
    const snapshot = await getAgentSession(repo);
    if (!mountedRef.current) return snapshot;
    setSession(snapshot);
    const nextGoal = snapshot.active_goal;
    if (nextGoal?.status === "completed") {
      agentStream.finish("completed");
      setSteeringEchoes([]);
    } else if (nextGoal?.status === "cancelled") {
      agentStream.finish("cancelled");
    } else if (nextGoal?.status === "failed") {
      agentStream.finish("failed");
    } else if (!nextGoal || !STREAMING.has(nextGoal.status)) {
      agentStream.reset();
    }
    return snapshot;
  }

  useEffect(() => {
    mountedRef.current = true;
    setLoading(true);
    setError(null);
    setSteeringEchoes([]);
    agentStream.reset();
    void Promise.all([getAgentSession(repo), listReviewModels()])
      .then(([nextSession, nextModels]) => {
        if (!mountedRef.current) return;
        const compatible = nextModels.filter((model) => model.capabilities.supports_tool_calling);
        setSession(nextSession);
        setModels(compatible);
        setSelectedModelId((current) => compatible.some((model) => model.id === current)
          ? current
          : (nextSession.active_goal?.model_id ?? compatible[0]?.id ?? ""));
        if (nextSession.active_goal && STREAMING.has(nextSession.active_goal.status)) {
          void agentStream.begin(nextSession.active_goal.goal_id);
        }
      })
      .catch((reason) => {
        if (mountedRef.current) setError(asAgentError(reason, t("agentWorkspace.loadError")));
      })
      .finally(() => {
        if (mountedRef.current) setLoading(false);
      });
    return () => {
      // Background Goals outlive this view. Unmounting only disconnects local event listeners.
      mountedRef.current = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [repo]);

  useEffect(() => {
    const goalId = goal?.goal_id;
    if (!goalId || TERMINAL.has(goal.status)) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listenAgentGoalEvents((event) => {
      if (!disposed && event.goal_id === goalId) void refresh().catch(() => undefined);
    }).then((dispose) => {
      if (disposed) dispose();
      else unlisten = dispose;
    }).catch(() => undefined);
    const timer = window.setInterval(() => void refresh().catch(() => undefined), 2_000);
    return () => {
      disposed = true;
      unlisten?.();
      window.clearInterval(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [repo, goal?.goal_id, goal?.status]);

  function setConsent(checked: boolean) {
    setConsented(checked);
    if (checked) localStorage.setItem(CONSENT_KEY, "accepted");
    else localStorage.removeItem(CONSENT_KEY);
  }

  async function submit() {
    const trimmed = message.trim();
    if (acting || !trimmed || !consented) return;
    setActing(true);
    setError(null);
    try {
      if (goal && !TERMINAL.has(goal.status)) {
        const nextGoal = await steerAgentGoal({
          repo_path: repo,
          goal_id: goal.goal_id,
          expected_revision: goal.revision,
          message: trimmed,
        });
        setSession((current) => current ? { ...current, active_goal: nextGoal } : current);
        setSteeringEchoes((current) => [...current, trimmed]);
      } else {
        if (!selectedModelId) return;
        const nextGoal = await createAgentGoal({
          repo_path: repo,
          goal_id: createGoalId(),
          model_id: selectedModelId,
          message: trimmed,
        });
        setSession((current) => current ? { ...current, active_goal: nextGoal } : current);
        await agentStream.begin(nextGoal.goal_id);
      }
      setMessage("");
    } catch (reason) {
      setError(asAgentError(reason, t("agentWorkspace.error")));
      await refresh().catch(() => undefined);
    } finally {
      if (mountedRef.current) setActing(false);
    }
  }

  async function mutate(action: "pause" | "resume" | "cancel") {
    if (!goal || acting) return;
    setActing(true);
    setError(null);
    try {
      const base = { repo_path: repo, goal_id: goal.goal_id, expected_revision: goal.revision };
      const nextGoal = action === "pause"
        ? await pauseAgentGoal(base)
        : action === "cancel"
          ? await cancelAgentGoal(base)
          : await resumeAgentGoal({ ...base, model_id: selectedModelId || null });
      setSession((current) => current ? { ...current, active_goal: nextGoal } : current);
      if (action === "resume") await agentStream.begin(nextGoal.goal_id);
      if (action === "cancel") agentStream.finish("cancelled");
    } catch (reason) {
      setError(asAgentError(reason, t("agentWorkspace.error")));
      await refresh().catch(() => undefined);
    } finally {
      if (mountedRef.current) setActing(false);
    }
  }

  async function extendBudget() {
    if (!goal || acting) return;
    const account = goal.usage_by_model.find((item) => item.model_id === goal.model_id);
    if (!account) return;
    const requested = Number(budgetDraft);
    if (!Number.isFinite(requested) || requested <= 0) return;
    if (account.limit_micros !== null && Math.round(requested * 1_000_000) <= account.limit_micros) return;
    if (account.limit_tokens !== null && Math.floor(requested) <= account.limit_tokens) return;
    setActing(true);
    setError(null);
    try {
      const nextGoal = await extendAgentBudget({
        repo_path: repo,
        goal_id: goal.goal_id,
        expected_revision: goal.revision,
        model_id: goal.model_id,
        currency: account.currency,
        new_limit_micros: account.limit_micros === null ? null : Math.round(requested * 1_000_000),
        new_limit_tokens: account.limit_tokens === null ? null : Math.floor(requested),
      });
      setSession((current) => current ? { ...current, active_goal: nextGoal } : current);
      setBudgetOpen(false);
      await agentStream.begin(nextGoal.goal_id);
    } catch (reason) {
      setError(asAgentError(reason, t("agentWorkspace.error")));
    } finally {
      if (mountedRef.current) setActing(false);
    }
  }

  async function reset() {
    if (active || acting) return;
    setLoading(true);
    setError(null);
    agentStream.reset();
    try {
      setSession(await resetAgentSession(repo));
      setSteeringEchoes([]);
    } catch (reason) {
      setError(asAgentError(reason, t("agentWorkspace.loadError")));
    } finally {
      if (mountedRef.current) setLoading(false);
    }
  }

  const messages = session?.recent_messages ?? [];
  const activeAccount = useMemo(
    () => goal?.usage_by_model.find((item) => item.model_id === goal.model_id) ?? null,
    [goal],
  );
  const showActiveObjective = Boolean(goal && !TERMINAL.has(goal.status));

  function openBudget() {
    if (!activeAccount) return;
    const current = activeAccount.limit_micros === null
      ? (activeAccount.limit_tokens ?? 0)
      : activeAccount.limit_micros / 1_000_000;
    setBudgetDraft(String(Math.max(current * 2, current + (activeAccount.limit_micros === null ? 1 : 0.01))));
    setBudgetOpen(true);
  }

  return (
    <section className="relative flex h-full min-h-0 flex-col" aria-label={t("agentWorkspace.aria")}>
      <header className="flex shrink-0 items-center gap-3 border-b border-line px-4 py-3">
        <span className="grid size-8 shrink-0 place-items-center rounded-lg border border-accent/25 bg-accent/10 text-accent"><AgentIcon width={17} height={17} /></span>
        <div className="min-w-0 flex-1">
          <h1 className="text-[13px] font-semibold text-fg">{t("agentWorkspace.title")}</h1>
          <p className="truncate text-[10.5px] text-fg-subtle">{t("agentWorkspace.subtitle")}</p>
        </div>
        {goal && <span className="rounded-full border border-line px-2 py-1 text-[10px] text-fg-subtle" aria-label="Goal status">{goalStatusLabel(goal)}</span>}
        <Button type="button" variant="ghost" size="sm" disabled={active || acting || loading} title={t("agentWorkspace.resetTitle")} onClick={() => void reset()}>{t("agentWorkspace.reset")}</Button>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex min-h-full w-full max-w-4xl flex-col px-5 py-6">
          {loading ? (
            <div className="grid flex-1 place-items-center text-xs text-fg-muted"><span className="flex items-center gap-2"><SpinnerIcon width={14} height={14} />{t("agentWorkspace.loading")}</span></div>
          ) : (
            <>
              {session?.memory_summary && <details className="mb-5 border-y border-line py-2 text-[11px] text-fg-muted"><summary className="cursor-pointer font-medium text-fg-subtle">{t("agentWorkspace.memory")}</summary><p className="mt-2 whitespace-pre-wrap leading-relaxed">{session.memory_summary}</p></details>}
              {messages.length === 0 && !showActiveObjective ? (
                <div className="grid flex-1 place-items-center py-16 text-center"><div className="max-w-md"><AgentIcon width={28} height={28} className="mx-auto text-accent" /><h2 className="mt-4 text-lg font-semibold text-fg">{t("agentWorkspace.empty")}</h2><p className="mt-2 text-xs leading-relaxed text-fg-muted">{t("agentWorkspace.emptyDetail")}</p></div></div>
              ) : (
                <div className="space-y-5" aria-live="polite">
                  {messages.map((item, index) => <Message key={`${session?.revision ?? 0}-${index}`} role={item.role} content={item.content} />)}
                  {showActiveObjective && <Message role="user" content={goal!.objective} pending />}
                  {steeringEchoes.map((content, index) => <Message key={`steering-${index}`} role="user" content={content} pending />)}
                </div>
              )}
              {agentStream.stream && <div className="mt-6"><AgentStreamPanel stream={agentStream.stream} /></div>}
              {goal && !TERMINAL.has(goal.status) && (
                <div className="mt-4 flex flex-wrap items-center gap-2 border-y border-line py-3 text-[11px] text-fg-muted">
                  {goal.status === "running" || goal.status === "queued" || goal.status === "pausing" ? <SpinnerIcon width={12} height={12} /> : null}
                  <span>{goalStatusDetail(goal)}</span><span className="font-mono">slice {goal.slice_index + 1}</span>
                  <span className="ml-auto flex gap-2">
                    {(goal.status === "running" || goal.status === "queued" || goal.status === "awaiting_approval") && <Button type="button" variant="ghost" size="sm" disabled={acting} onClick={() => void mutate("pause")}>Pause</Button>}
                    {(goal.status === "paused" || goal.status === "blocked") && <Button type="button" variant="primary" size="sm" disabled={acting || goal.pause_reason === "budget"} onClick={() => void mutate("resume")}>Resume</Button>}
                    {goal.status === "paused" && goal.pause_reason === "budget" && <Button type="button" variant="primary" size="sm" disabled={acting} onClick={openBudget}>Extend budget</Button>}
                    <Button type="button" variant="danger" size="sm" disabled={acting} onClick={() => void mutate("cancel")}>Cancel</Button>
                  </span>
                </div>
              )}
              {error && <div className="mt-5 flex items-start gap-2 border-y border-danger/30 bg-danger/[0.06] px-3 py-2.5 text-xs text-danger" role="alert"><AlertIcon width={14} height={14} className="mt-0.5 shrink-0" /><div><div>{error.message}</div><div className="mt-1 font-mono text-[10px] opacity-70">{error.code}{error.diagnostic_id ? ` · ${error.diagnostic_id}` : ""}</div></div></div>}
              {activeAccount && <p className="mt-4 text-right font-mono text-[10px] text-fg-subtle">{activeAccount.input_tokens} input · {activeAccount.cached_input_tokens} cached · {activeAccount.output_tokens} output · {formatBudget(activeAccount.spent_micros, activeAccount.currency)}</p>}
            </>
          )}
        </div>
      </div>

      {budgetOpen && activeAccount && (
        <div className="absolute inset-0 z-40 grid place-items-center bg-black/35 p-4" role="dialog" aria-modal="true" aria-label="Extend Goal budget">
          <div className="w-full max-w-sm rounded-xl border border-line bg-canvas p-4 shadow-xl">
            <h2 className="text-sm font-semibold text-fg">Extend Goal budget</h2>
            <dl className="mt-3 grid grid-cols-2 gap-y-2 text-[11px] text-fg-muted">
              <dt>Model</dt><dd className="text-right font-mono text-fg">{activeAccount.model_id}</dd>
              <dt>Spent</dt><dd className="text-right font-mono text-fg">{formatBudget(activeAccount.spent_micros, activeAccount.currency)}</dd>
              <dt>Current limit</dt><dd className="text-right font-mono text-fg">{activeAccount.limit_micros === null ? `${activeAccount.limit_tokens ?? 0} tokens` : formatBudget(activeAccount.limit_micros, activeAccount.currency)}</dd>
            </dl>
            <label className="mt-4 block text-[11px] text-fg-muted">New limit ({activeAccount.limit_micros === null ? "tokens" : activeAccount.currency ?? "currency units"})<input autoFocus value={budgetDraft} onChange={(event) => setBudgetDraft(event.currentTarget.value)} inputMode="decimal" className="mt-1 w-full rounded-lg border border-line-strong bg-canvas px-3 py-2 font-mono text-xs text-fg outline-none focus:border-accent" /></label>
            <div className="mt-4 flex justify-end gap-2"><Button type="button" variant="ghost" size="sm" disabled={acting} onClick={() => setBudgetOpen(false)}>Cancel</Button><Button type="button" variant="primary" size="sm" disabled={acting} onClick={() => void extendBudget()}>Apply extension</Button></div>
          </div>
        </div>
      )}

      <footer className="shrink-0 border-t border-line bg-elevated/40 px-5 py-4">
        <div className="mx-auto grid w-full max-w-4xl gap-3">
          <div className="flex items-start gap-4">
            <div className="min-w-0 flex-1"><AgentModelPicker id="repository-agent-model" label={t("agentWorkspace.model")} models={models} value={selectedModelId} onChange={setSelectedModelId} onConfigureCredential={onConfigureCredential} /></div>
            <label className="mt-5 flex max-w-sm items-start gap-2 text-[10.5px] leading-relaxed text-fg-muted"><input type="checkbox" checked={consented} onChange={(event) => setConsent(event.currentTarget.checked)} className="mt-0.5 accent-accent" />{t("agentWorkspace.consent")}</label>
          </div>
          {models.length === 0 && <p className="text-[11px] text-warning">{t("agentWorkspace.noModels")}</p>}
          <form className="flex items-end gap-2" onSubmit={(event) => { event.preventDefault(); void submit(); }}>
            <textarea aria-label={t("agentWorkspace.placeholder")} placeholder={active ? "Steer the active Goal at its next safe boundary…" : t("agentWorkspace.placeholder")} value={message} disabled={acting} maxLength={64 * 1024} rows={3} onChange={(event) => setMessage(event.currentTarget.value)} onKeyDown={(event) => { if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); void submit(); } }} className="min-h-20 min-w-0 flex-1 resize-y rounded-lg border border-line-strong bg-canvas px-3 py-2 text-xs leading-relaxed text-fg outline-none transition-colors focus:border-accent disabled:opacity-60" />
            <Button type="submit" variant="primary" size="md" disabled={acting || !message.trim() || !consented || (!active && !selectedModelId)}>{active ? "Steer" : t("agentWorkspace.send")}</Button>
          </form>
        </div>
      </footer>
    </section>
  );
}

function Message({ role, content, pending = false }: { role: string; content: string; pending?: boolean }) {
  const t = useT();
  const isUser = role === "user";
  return <article className={isUser ? "ml-auto max-w-[78%]" : "mr-auto max-w-[88%]"}><div className={`mb-1 text-[10px] font-semibold uppercase tracking-wide ${isUser ? "text-right text-fg-subtle" : "text-accent"}`}>{isUser ? t("agentWorkspace.you") : t("agentWorkspace.agent")}</div><div className={`whitespace-pre-wrap break-words rounded-xl px-3.5 py-2.5 text-[12.5px] leading-6 ${isUser ? "rounded-br-sm bg-accent text-white" : "rounded-bl-sm border border-line bg-elevated text-fg"} ${pending ? "opacity-70" : ""}`}>{content}</div></article>;
}

function goalStatusLabel(goal: AgentGoalSnapshotDto): string {
  return goal.status.replace(/_/g, " ");
}

function goalStatusDetail(goal: AgentGoalSnapshotDto): string {
  if (goal.status === "paused" && goal.pause_reason === "app_restarted") return "Checkpoint restored. Resume explicitly to continue.";
  if (goal.status === "paused" && goal.pause_reason === "budget") return "Soft model budget reached. Extend it to continue from this checkpoint.";
  if (goal.status === "blocked") return `Blocked: ${(goal.block_reason ?? "review required").replace(/_/g, " ")}`;
  if (goal.status === "awaiting_approval") return "Waiting for tool approval.";
  if (goal.status === "pausing") return "Pausing after the current atomic step.";
  if (goal.status === "paused") return "Paused at a safe checkpoint.";
  return "Running in the background. You can navigate away safely.";
}

function formatBudget(spentMicros: number, currency: string | null): string {
  if (!currency) return `${spentMicros} µ`;
  const symbol = currency === "CNY" ? "¥" : currency === "USD" ? "$" : `${currency} `;
  return `${symbol}${(spentMicros / 1_000_000).toFixed(4)}`;
}

function asAgentError(reason: unknown, fallback: string): AgentIpcErrorDto {
  if (typeof reason === "object" && reason !== null && "code" in reason) {
    const value = reason as Partial<AgentIpcErrorDto>;
    return { code: typeof value.code === "string" ? value.code : "AGENT_UNKNOWN", message: typeof value.message === "string" ? value.message : fallback, recoverable: value.recoverable !== false, diagnostic_id: typeof value.diagnostic_id === "string" ? value.diagnostic_id : "" };
  }
  return { code: "AGENT_UNKNOWN", message: fallback, recoverable: true, diagnostic_id: "" };
}

function createGoalId(): string {
  return `goal-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
}
