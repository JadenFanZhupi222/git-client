import { useEffect, useRef, useState } from "react";
import type {
  AgentIpcErrorDto,
  AgentSessionSnapshotDto,
  AgentSessionTurnResultDto,
  CredentialKindDto,
  ReviewModelOptionDto,
} from "../bindings";
import {
  cancelAgentTurn,
  getAgentSession,
  listReviewModels,
  resetAgentSession,
  startAgentTurn,
} from "../ipc";
import { useAgentStream } from "../hooks/useAgentStream";
import { useT } from "../lib/i18n";
import { AgentModelPicker } from "./AgentModelPicker";
import { AgentStreamPanel } from "./AgentStreamPanel";
import { AgentIcon, AlertIcon, SpinnerIcon } from "./icons";
import { Button } from "./ui/Button";

const CONSENT_KEY = "repository-agent-provider-consent-v1";

export function AgentWorkspace({
  repo,
  onConfigureCredential = () => undefined,
}: {
  repo: string;
  onConfigureCredential?: (kind: CredentialKindDto) => void;
}) {
  const t = useT();
  const mountedRef = useRef(true);
  const runIdRef = useRef<string | null>(null);
  const [session, setSession] = useState<AgentSessionSnapshotDto | null>(null);
  const [models, setModels] = useState<ReviewModelOptionDto[]>([]);
  const [selectedModelId, setSelectedModelId] = useState("");
  const [message, setMessage] = useState("");
  const [pendingMessage, setPendingMessage] = useState<string | null>(null);
  const [result, setResult] = useState<AgentSessionTurnResultDto | null>(null);
  const [error, setError] = useState<AgentIpcErrorDto | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [consented, setConsented] = useState(() => localStorage.getItem(CONSENT_KEY) === "accepted");
  const agentStream = useAgentStream();

  useEffect(() => {
    mountedRef.current = true;
    setLoading(true);
    setError(null);
    setResult(null);
    setPendingMessage(null);
    agentStream.reset();
    void Promise.all([getAgentSession(repo), listReviewModels()])
      .then(([nextSession, nextModels]) => {
        if (!mountedRef.current) return;
        const compatible = nextModels.filter((model) => model.capabilities.supports_tool_calling);
        setSession(nextSession);
        setModels(compatible);
        setSelectedModelId((current) => compatible.some((model) => model.id === current)
          ? current
          : (compatible[0]?.id ?? ""));
      })
      .catch((reason) => {
        if (mountedRef.current) setError(asAgentError(reason, t("agentWorkspace.loadError")));
      })
      .finally(() => {
        if (mountedRef.current) setLoading(false);
      });
    return () => {
      mountedRef.current = false;
      const runId = runIdRef.current;
      runIdRef.current = null;
      if (runId) void cancelAgentTurn(runId).catch(() => undefined);
    };
    // The repository prop identifies a fresh session workspace lifecycle.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [repo]);

  function setConsent(checked: boolean) {
    setConsented(checked);
    if (checked) localStorage.setItem(CONSENT_KEY, "accepted");
    else localStorage.removeItem(CONSENT_KEY);
  }

  async function submit() {
    const trimmed = message.trim();
    if (busy || !trimmed || !selectedModelId || !consented) return;
    const runId = createRunId();
    runIdRef.current = runId;
    setBusy(true);
    setStopping(false);
    setError(null);
    setResult(null);
    setPendingMessage(trimmed);
    setMessage("");
    try {
      await agentStream.begin(runId);
      const completed = await startAgentTurn({
        repo_path: repo,
        run_id: runId,
        model_id: selectedModelId,
        message: trimmed,
      });
      if (!mountedRef.current || runIdRef.current !== runId) return;
      setResult(completed);
      setSession((current) => fallbackCommittedSession(current, completed, trimmed));
      agentStream.finish("completed");
      void getAgentSession(repo).then((snapshot) => {
        if (mountedRef.current && runIdRef.current === null) setSession(snapshot);
      }).catch(() => undefined);
    } catch (reason) {
      if (!mountedRef.current || runIdRef.current !== runId) return;
      const nextError = asAgentError(reason, t("agentWorkspace.error"));
      setError(nextError);
      agentStream.finish(nextError.code === "AGENT_CANCELLED" ? "cancelled" : "failed");
    } finally {
      if (runIdRef.current === runId) runIdRef.current = null;
      if (mountedRef.current) {
        setPendingMessage(null);
        setBusy(false);
        setStopping(false);
      }
    }
  }

  async function stop() {
    const runId = runIdRef.current;
    if (!runId || stopping) return;
    setStopping(true);
    try {
      await cancelAgentTurn(runId);
    } catch {
      if (mountedRef.current) setStopping(false);
    }
  }

  async function reset() {
    if (busy) return;
    setLoading(true);
    setError(null);
    setResult(null);
    agentStream.reset();
    try {
      setSession(await resetAgentSession(repo));
    } catch (reason) {
      setError(asAgentError(reason, t("agentWorkspace.loadError")));
    } finally {
      if (mountedRef.current) setLoading(false);
    }
  }

  const messages = session?.recent_messages ?? [];
  return (
    <section className="flex h-full min-h-0 flex-col" aria-label={t("agentWorkspace.aria")}>
      <header className="flex shrink-0 items-center gap-3 border-b border-line px-4 py-3">
        <span className="grid size-8 shrink-0 place-items-center rounded-lg border border-accent/25 bg-accent/10 text-accent">
          <AgentIcon width={17} height={17} />
        </span>
        <div className="min-w-0 flex-1">
          <h1 className="text-[13px] font-semibold text-fg">{t("agentWorkspace.title")}</h1>
          <p className="truncate text-[10.5px] text-fg-subtle">{t("agentWorkspace.subtitle")}</p>
        </div>
        <span className="hidden rounded-full border border-line px-2 py-1 text-[10px] text-fg-subtle xl:inline">
          {t("agentWorkspace.safety")}
        </span>
        <Button type="button" variant="ghost" size="sm" disabled={busy || loading} title={t("agentWorkspace.resetTitle")} onClick={() => void reset()}>
          {t("agentWorkspace.reset")}
        </Button>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex min-h-full w-full max-w-4xl flex-col px-5 py-6">
          {loading ? (
            <div className="grid flex-1 place-items-center text-xs text-fg-muted">
              <span className="flex items-center gap-2"><SpinnerIcon width={14} height={14} />{t("agentWorkspace.loading")}</span>
            </div>
          ) : (
            <>
              {session?.memory_summary && (
                <details className="mb-5 border-y border-line py-2 text-[11px] text-fg-muted">
                  <summary className="cursor-pointer font-medium text-fg-subtle">{t("agentWorkspace.memory")}</summary>
                  <p className="mt-2 whitespace-pre-wrap leading-relaxed">{session.memory_summary}</p>
                </details>
              )}
              {messages.length === 0 && !pendingMessage ? (
                <div className="grid flex-1 place-items-center py-16 text-center">
                  <div className="max-w-md">
                    <AgentIcon width={28} height={28} className="mx-auto text-accent" />
                    <h2 className="mt-4 text-lg font-semibold text-fg">{t("agentWorkspace.empty")}</h2>
                    <p className="mt-2 text-xs leading-relaxed text-fg-muted">{t("agentWorkspace.emptyDetail")}</p>
                  </div>
                </div>
              ) : (
                <div className="space-y-5" aria-live="polite">
                  {messages.map((item, index) => (
                    <Message key={`${session?.revision ?? 0}-${index}`} role={item.role} content={item.content} />
                  ))}
                  {pendingMessage && <Message role="user" content={pendingMessage} pending />}
                </div>
              )}

              {agentStream.stream && (
                <div className="mt-6"><AgentStreamPanel stream={agentStream.stream} /></div>
              )}
              {busy && (
                <p className="mt-3 flex items-center gap-2 text-[11px] text-fg-muted" aria-live="polite">
                  <SpinnerIcon width={12} height={12} />{t("agentWorkspace.pending")}
                </p>
              )}
              {error && (
                <div className="mt-5 flex items-start gap-2 border-y border-danger/30 bg-danger/[0.06] px-3 py-2.5 text-xs text-danger" role="alert">
                  <AlertIcon width={14} height={14} className="mt-0.5 shrink-0" />
                  <div>
                    <div>{error.code === "AGENT_CANCELLED" ? t("agentWorkspace.cancelled") : error.message}</div>
                    <div className="mt-1 font-mono text-[10px] opacity-70">{error.code}{error.diagnostic_id ? ` · ${error.diagnostic_id}` : ""}</div>
                  </div>
                </div>
              )}
              {result && (
                <p className="mt-4 text-right font-mono text-[10px] text-fg-subtle">
                  {t("agentWorkspace.usage", {
                    input: result.usage.input_tokens,
                    output: result.usage.output_tokens,
                    rounds: result.model_rounds,
                  })}
                </p>
              )}
            </>
          )}
        </div>
      </div>

      <footer className="shrink-0 border-t border-line bg-elevated/40 px-5 py-4">
        <div className="mx-auto grid w-full max-w-4xl gap-3">
          <div className="flex items-start gap-4">
            <div className="min-w-0 flex-1">
              <AgentModelPicker
                id="repository-agent-model"
                label={t("agentWorkspace.model")}
                models={models}
                value={selectedModelId}
                onChange={setSelectedModelId}
                onConfigureCredential={onConfigureCredential}
              />
            </div>
            <label className="mt-5 flex max-w-sm items-start gap-2 text-[10.5px] leading-relaxed text-fg-muted">
              <input type="checkbox" checked={consented} disabled={busy} onChange={(event) => setConsent(event.currentTarget.checked)} className="mt-0.5 accent-accent" />
              {t("agentWorkspace.consent")}
            </label>
          </div>
          {models.length === 0 && <p className="text-[11px] text-warning">{t("agentWorkspace.noModels")}</p>}
          <form className="flex items-end gap-2" onSubmit={(event) => { event.preventDefault(); void submit(); }}>
            <textarea
              aria-label={t("agentWorkspace.placeholder")}
              placeholder={t("agentWorkspace.placeholder")}
              value={message}
              disabled={busy}
              maxLength={64 * 1024}
              rows={3}
              onChange={(event) => setMessage(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !event.shiftKey) {
                  event.preventDefault();
                  void submit();
                }
              }}
              className="min-h-20 min-w-0 flex-1 resize-y rounded-lg border border-line-strong bg-canvas px-3 py-2 text-xs leading-relaxed text-fg outline-none transition-colors focus:border-accent disabled:opacity-60"
            />
            {busy ? (
              <Button type="button" variant="danger" size="md" disabled={stopping} onClick={() => void stop()}>
                {stopping ? t("agentWorkspace.stopping") : t("agentWorkspace.stop")}
              </Button>
            ) : (
              <Button type="submit" variant="primary" size="md" disabled={!message.trim() || !selectedModelId || !consented}>
                {t("agentWorkspace.send")}
              </Button>
            )}
          </form>
        </div>
      </footer>
    </section>
  );
}

function Message({ role, content, pending = false }: { role: string; content: string; pending?: boolean }) {
  const t = useT();
  const isUser = role === "user";
  return (
    <article className={isUser ? "ml-auto max-w-[78%]" : "mr-auto max-w-[88%]"}>
      <div className={`mb-1 text-[10px] font-semibold uppercase tracking-wide ${isUser ? "text-right text-fg-subtle" : "text-accent"}`}>
        {isUser ? t("agentWorkspace.you") : t("agentWorkspace.agent")}
      </div>
      <div className={`whitespace-pre-wrap break-words rounded-xl px-3.5 py-2.5 text-[12.5px] leading-6 ${
        isUser ? "rounded-br-sm bg-accent text-white" : "rounded-bl-sm border border-line bg-elevated text-fg"
      } ${pending ? "opacity-70" : ""}`}>
        {content}
      </div>
    </article>
  );
}

function fallbackCommittedSession(
  current: AgentSessionSnapshotDto | null,
  result: AgentSessionTurnResultDto,
  userMessage: string,
): AgentSessionSnapshotDto {
  return {
    session_id: result.session_id,
    revision: result.revision,
    memory_summary: current?.memory_summary ?? null,
    recent_messages: [
      ...(current?.recent_messages ?? []),
      { role: "user", content: userMessage },
      { role: "assistant", content: result.final_text },
    ],
  };
}

function asAgentError(reason: unknown, fallback: string): AgentIpcErrorDto {
  if (typeof reason === "object" && reason !== null && "code" in reason) {
    const value = reason as Partial<AgentIpcErrorDto>;
    return {
      code: typeof value.code === "string" ? value.code : "AGENT_UNKNOWN",
      message: typeof value.message === "string" ? value.message : fallback,
      recoverable: value.recoverable !== false,
      diagnostic_id: typeof value.diagnostic_id === "string" ? value.diagnostic_id : "",
    };
  }
  return { code: "AGENT_UNKNOWN", message: fallback, recoverable: true, diagnostic_id: "" };
}

function createRunId(): string {
  return `agent-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
}
