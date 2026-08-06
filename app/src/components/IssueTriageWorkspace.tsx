import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  cancelIssueTriage,
  listReviewModels,
  onReviewProgress,
  startIssueTriage,
  type IpcError,
} from "../ipc";
import type {
  IssueContextDto,
  IssueTargetDto,
  IssueTriageResultDto,
  ReviewLanguageDto,
  ReviewModelOptionDto,
  ReviewProgressEventDto,
} from "../bindings";
import { useLang, useT } from "../lib/i18n";
import { CheckIcon, CloseIcon, SpinnerIcon } from "./icons";

const CONSENT_KEY = "issue-triage-consent-v1";
const CACHE_PREFIX = "issue-triage-result-v1";
type CredentialKind = "deepseek" | "github";
type Phase = "select" | "running" | "results";

export function IssueTriageWorkspace({
  target,
  context,
  onClose,
  onConfigureCredential,
}: {
  target: IssueTargetDto;
  context: IssueContextDto;
  onClose: () => void;
  onConfigureCredential: (kind: CredentialKind) => void;
}) {
  const t = useT();
  const lang = useLang();
  const dialogRef = useRef<HTMLDivElement>(null);
  const mountedRef = useRef(true);
  const runIdRef = useRef<string | null>(null);
  const unsubscribeRef = useRef<(() => void) | null>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);
  const [phase, setPhase] = useState<Phase>("select");
  const [models, setModels] = useState<ReviewModelOptionDto[]>([]);
  const [selectedModelId, setSelectedModelId] = useState("");
  const [outputLanguage, setOutputLanguage] = useState<ReviewLanguageDto>(lang === "zh" ? "simplified_chinese" : "english");
  const [consented, setConsented] = useState(() => localStorage.getItem(CONSENT_KEY) === "accepted");
  const [progress, setProgress] = useState<ReviewProgressEventDto | null>(null);
  const [result, setResult] = useState<IssueTriageResultDto | null>(() => loadCachedResult(target, context));
  const [error, setError] = useState<IpcError | null>(null);
  const [cancelling, setCancelling] = useState(false);

  const busy = phase === "running";
  const estimatedTokens = useMemo(() => {
    const bytes = new TextEncoder().encode(JSON.stringify(context)).length;
    return Math.ceil(bytes / 3) + 900;
  }, [context]);

  useLayoutEffect(() => {
    mountedRef.current = true;
    previousFocusRef.current = document.activeElement as HTMLElement | null;
    dialogRef.current?.focus();
    return () => {
      mountedRef.current = false;
      cleanupListener();
      const runId = runIdRef.current;
      runIdRef.current = null;
      if (runId) void cancelIssueTriage(runId).catch(() => undefined);
      previousFocusRef.current?.focus();
    };
  }, []);

  useEffect(() => {
    if (result) setPhase("results");
    void listReviewModels()
      .then((next) => {
        if (!mountedRef.current) return;
        setModels(next);
        setSelectedModelId((current) => next.some((model) => model.id === current) ? current : (next[0]?.id ?? ""));
      })
      .catch((reason) => { if (mountedRef.current) setError(asIpcError(reason)); });
  }, []);

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        if (!busy) onClose();
        return;
      }
      if (event.key !== "Tab" || !dialogRef.current) return;
      const focusable = Array.from(dialogRef.current.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ));
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (document.activeElement === dialogRef.current) {
        event.preventDefault();
        first.focus();
      } else if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [busy, onClose]);

  function cleanupListener() {
    unsubscribeRef.current?.();
    unsubscribeRef.current = null;
  }

  function acceptConsent() {
    localStorage.setItem(CONSENT_KEY, "accepted");
    setConsented(true);
  }

  async function startTriage() {
    if (!consented || !selectedModelId || busy) return;
    setError(null);
    setProgress(null);
    setCancelling(false);
    setPhase("running");
    const runId = createRunId();
    runIdRef.current = runId;
    try {
      const unsubscribe = await onReviewProgress((event) => {
        if (mountedRef.current && event.run_id === runId) setProgress(event);
      });
      if (!mountedRef.current || runIdRef.current !== runId) {
        unsubscribe();
        return;
      }
      unsubscribeRef.current = unsubscribe;
      const next = await startIssueTriage({
        run_id: runId,
        target,
        expected_updated_at: context.snapshot.updated_at,
        expected_comments: context.snapshot.comments,
        model_id: selectedModelId,
        output_language: outputLanguage,
      });
      cleanupListener();
      if (!mountedRef.current) return;
      setResult(next);
      saveCachedResult(target, next);
      setPhase("results");
    } catch (reason) {
      cleanupListener();
      if (!mountedRef.current) return;
      const nextError = asIpcError(reason);
      setCancelling(false);
      setError(nextError.code === "CANCELLED" ? null : nextError);
      setPhase("select");
    } finally {
      if (runIdRef.current === runId) runIdRef.current = null;
    }
  }

  async function cancelTriage() {
    if (!runIdRef.current || cancelling) return;
    setCancelling(true);
    try {
      await cancelIssueTriage(runIdRef.current);
    } catch (reason) {
      if (mountedRef.current) {
        setCancelling(false);
        setError(asIpcError(reason));
      }
    }
  }

  function analyzeAgain() {
    clearCachedResult(target);
    setResult(null);
    setError(null);
    setPhase("select");
  }

  return (
    <div data-testid="issue-triage-backdrop" className="overlay-in fixed inset-0 z-[70] flex items-center justify-center bg-black/50 p-4" onClick={(event) => { event.stopPropagation(); if (!busy) onClose(); }}>
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="issue-triage-title"
        tabIndex={-1}
        className="panel-in popover flex max-h-[90vh] w-full max-w-3xl flex-col overflow-hidden rounded-lg border border-line-strong bg-canvas outline-none"
        onClick={(event) => event.stopPropagation()}
      >
        <header className="flex items-center gap-3 border-b border-line px-5 py-4">
          <div className="min-w-0">
            <h2 id="issue-triage-title" className="text-base font-semibold text-fg">{t("issueTriage.title")}</h2>
            <p className="truncate text-xs text-fg-subtle">{target.owner}/{target.repo} · #{target.issue_number}</p>
          </div>
          <button aria-label={t("issueTriage.closeAria")} disabled={busy} onClick={onClose} className="ml-auto grid h-8 w-8 place-items-center rounded text-fg-muted hover:bg-overlay hover:text-fg disabled:opacity-40">
            <CloseIcon width={14} height={14} />
          </button>
        </header>

        <main className="min-h-0 flex-1 overflow-y-auto p-5">
          <div className="mb-4 flex flex-wrap gap-3 rounded-md border border-line bg-elevated/60 px-3 py-2 text-xs text-fg-muted">
            <span>{t("issueTriage.snapshot")}: <code className="text-fg">{context.snapshot.updated_at}</code></span>
            <span>{t("issueTriage.comments", { count: context.comments.length })}</span>
            <span>{t("issueTriage.labels", { count: context.available_labels.length })}</span>
            <span>{t("issueTriage.similar", { count: context.similar_issues.length })}</span>
          </div>

          {phase === "select" && (
            <section>
              <div className="flex flex-wrap items-end gap-3 border-y border-line py-3">
                <label className="grid min-w-44 gap-1 text-[11px] font-medium text-fg-muted">
                  {t("issueTriage.model")}
                  <select value={selectedModelId} onChange={(event) => setSelectedModelId(event.currentTarget.value)} className="h-8 rounded-md border border-line bg-canvas px-2 text-xs font-normal text-fg outline-none focus:border-accent">
                    {models.map((model) => <option key={model.id} value={model.id}>{model.label}</option>)}
                  </select>
                </label>
                <label className="grid min-w-44 gap-1 text-[11px] font-medium text-fg-muted">
                  {t("issueTriage.language")}
                  <select value={outputLanguage} onChange={(event) => setOutputLanguage(event.currentTarget.value as ReviewLanguageDto)} className="h-8 rounded-md border border-line bg-canvas px-2 text-xs font-normal text-fg outline-none focus:border-accent">
                    <option value="simplified_chinese">{t("prReview.language.zh")}</option>
                    <option value="english">{t("prReview.language.en")}</option>
                  </select>
                </label>
                <div className="ml-auto pb-0.5 text-right text-[11px] text-fg-muted">
                  <p className="font-medium text-fg">{t("issueTriage.estimatedTokens", { count: estimatedTokens.toLocaleString() })}</p>
                  <p>{t("issueTriage.estimateHint")}</p>
                </div>
              </div>
              <div className="mt-4 rounded-md border border-line bg-elevated/35 p-4">
                <h3 className="text-sm font-semibold text-fg">{context.issue.title}</h3>
                <p className="mt-2 text-xs leading-5 text-fg-muted">{t("issueTriage.readOnly")}</p>
              </div>
              {!consented && (
                <label className="mt-4 flex items-start gap-2 rounded-md border border-accent/40 bg-accent/10 p-3 text-xs text-fg">
                  <input type="checkbox" className="mt-0.5" checked={false} onChange={acceptConsent} />
                  <span>{t("issueTriage.consent")}</span>
                </label>
              )}
            </section>
          )}

          {phase === "running" && (
            <section className="grid min-h-56 place-items-center text-center" aria-live="polite">
              <div className="flex flex-col items-center">
                <SpinnerIcon width={24} height={24} />
                <p className="mt-3 text-sm text-fg">{progressLabel(progress, t)}</p>
              </div>
            </section>
          )}

          {phase === "results" && result && <TriageResults result={result} />}
          {error && <ErrorNotice error={error} onConfigureCredential={onConfigureCredential} onRetry={error.code === "ISSUE_UPDATED" ? onClose : undefined} />}
        </main>

        <footer className="flex items-center justify-end gap-2 border-t border-line px-5 py-3">
          {phase === "running" ? (
            <button disabled={cancelling} onClick={cancelTriage} className="rounded-md border border-danger/50 px-3 py-1.5 text-xs text-danger disabled:opacity-50">{cancelling ? t("issueTriage.cancelling") : t("issueTriage.cancel")}</button>
          ) : phase === "results" ? (
            <>
              <button onClick={analyzeAgain} className="rounded-md border border-line-strong px-3 py-1.5 text-xs text-fg-muted hover:bg-overlay hover:text-fg">{t("issueTriage.again")}</button>
              <button onClick={onClose} className="rounded-md bg-accent px-4 py-1.5 text-xs font-semibold text-on-accent">{t("issueTriage.done")}</button>
            </>
          ) : (
            <button disabled={!consented || !selectedModelId} onClick={startTriage} className="rounded-md bg-accent px-4 py-1.5 text-xs font-semibold text-on-accent disabled:opacity-40">{t("issueTriage.start")}</button>
          )}
        </footer>
      </div>
    </div>
  );
}

function TriageResults({ result }: { result: IssueTriageResultDto }) {
  const t = useT();
  const proposal = result.proposal;
  return (
    <section aria-labelledby="issue-triage-result">
      <div className="flex items-start gap-3">
        <div className="grid h-9 w-9 shrink-0 place-items-center rounded-full bg-success/12 text-success"><CheckIcon width={18} height={18} /></div>
        <div>
          <h3 id="issue-triage-result" className="text-base font-semibold text-fg">{t("issueTriage.result")}</h3>
          <p className="mt-1 text-xs leading-5 text-fg-muted">{proposal.summary}</p>
        </div>
      </div>

      <div className="mt-5 grid grid-cols-3 gap-2 max-sm:grid-cols-1">
        <ResultFact label={t("issueTriage.category")} value={proposal.category} />
        <ResultFact label={t("issueTriage.priority")} value={proposal.priority} />
        <ResultFact label={t("issueTriage.confidence")} value={`${Math.round(proposal.confidence * 100)}%`} />
      </div>

      <div className="mt-4 grid gap-3">
        <ResultSection title={t("issueTriage.suggestedLabels")} empty={t("issueTriage.none")}>
          {proposal.suggested_labels.length > 0 && <div className="flex flex-wrap gap-1.5">{proposal.suggested_labels.map((label) => <span key={label} className="rounded-full border border-line-strong px-2 py-0.5 text-[11px] text-fg-muted">{label}</span>)}</div>}
        </ResultSection>
        <ResultSection title={t("issueTriage.duplicates")} empty={t("issueTriage.none")}>
          {proposal.suspected_duplicate_numbers.length > 0 && <p className="font-mono text-xs text-fg">{proposal.suspected_duplicate_numbers.map((number) => `#${number}`).join(", ")}</p>}
        </ResultSection>
        <ResultSection title={t("issueTriage.reply")} empty={t("issueTriage.none")}>
          {proposal.suggested_reply && <p className="whitespace-pre-wrap text-xs leading-5 text-fg-muted">{proposal.suggested_reply}</p>}
        </ResultSection>
        <ResultSection title={t("issueTriage.rationale")} empty={t("issueTriage.none")}>
          {proposal.rationale.length > 0 && <ul className="grid gap-1 pl-4 text-xs leading-5 text-fg-muted">{proposal.rationale.map((item, index) => <li key={`${index}-${item}`} className="list-disc">{item}</li>)}</ul>}
        </ResultSection>
      </div>

      <p className="mt-4 text-[11px] text-fg-subtle">
        {t("issueTriage.usage", { input: result.usage.input_tokens, output: result.usage.output_tokens })} · {t("issueTriage.analyzedComments", { count: result.comments_analyzed })}
        {result.comments_truncated ? ` · ${t("issueTriage.truncated")}` : ""}
      </p>
      <p className="mt-1 text-[11px] text-fg-subtle">{t("issueTriage.savedLocally")}</p>
    </section>
  );
}

function ResultFact({ label, value }: { label: string; value: string }) {
  return <div className="rounded-md border border-line bg-elevated/40 p-3"><p className="text-[10px] uppercase tracking-wide text-fg-subtle">{label}</p><p className="mt-1 text-sm font-semibold text-fg">{value}</p></div>;
}

function ResultSection({ title, empty, children }: { title: string; empty: string; children?: React.ReactNode }) {
  return <section className="rounded-md border border-line p-3"><h4 className="text-xs font-semibold text-fg">{title}</h4><div className="mt-2">{children || <p className="text-xs text-fg-subtle">{empty}</p>}</div></section>;
}

function ErrorNotice({ error, onConfigureCredential, onRetry }: { error: IpcError; onConfigureCredential: (kind: CredentialKind) => void; onRetry?: () => void }) {
  const t = useT();
  const credential = error.code === "AI_KEY_MISSING" ? "deepseek" : error.code === "GITHUB_TOKEN_MISSING" ? "github" : null;
  return (
    <div role="alert" className="mt-4 rounded-md border border-danger/40 bg-danger/10 p-3 text-xs text-danger">
      <p>{errorMessage(error, t)}</p>
      <div className="mt-2 flex gap-2">
        {credential && <button onClick={() => onConfigureCredential(credential)} className="rounded border border-danger/50 px-2 py-1">{t("issueTriage.openSettings")}</button>}
        {onRetry && <button onClick={onRetry} className="rounded border border-danger/50 px-2 py-1">{t("issueTriage.refresh")}</button>}
      </div>
    </div>
  );
}

function progressLabel(progress: ReviewProgressEventDto | null, t: ReturnType<typeof useT>) {
  if (!progress) return t("issueTriage.stage.loading_issue");
  const known = ["loading_issue", "analyzing_issue", "completed", "failed", "cancelled"];
  return known.includes(progress.stage)
    ? t(`issueTriage.stage.${progress.stage}` as Parameters<typeof t>[0])
    : t("issueTriage.stage.analyzing_issue");
}

function errorMessage(error: IpcError, t: ReturnType<typeof useT>) {
  const known = ["AI_KEY_MISSING", "GITHUB_TOKEN_MISSING", "ISSUE_UPDATED", "ISSUE_NOT_FOUND", "ISSUE_TRIAGE_BUDGET_EXCEEDED", "NETWORK_ERROR", "RATE_LIMITED", "AUTH_FAILED", "INVALID_MODEL_OUTPUT", "INVALID_REVIEW_MODEL", "CANCELLED"];
  return known.includes(error.code) ? t(`issueTriage.error.${error.code}` as Parameters<typeof t>[0]) : error.message;
}

function asIpcError(reason: unknown): IpcError {
  const candidate = reason as Partial<IpcError> | null;
  return { code: candidate?.code ?? "UNKNOWN", message: candidate?.message ?? String(reason), recoverable: candidate?.recoverable ?? true };
}

function createRunId() {
  return globalThis.crypto?.randomUUID?.() ?? `issue-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function cacheKey(target: IssueTargetDto) {
  return `${CACHE_PREFIX}:${target.owner}/${target.repo}#${target.issue_number}`;
}

function loadCachedResult(target: IssueTargetDto, context: IssueContextDto): IssueTriageResultDto | null {
  try {
    const value = localStorage.getItem(cacheKey(target));
    if (!value) return null;
    const result = JSON.parse(value) as IssueTriageResultDto;
    if (result.snapshot.updated_at !== context.snapshot.updated_at || result.snapshot.comments !== context.snapshot.comments) {
      localStorage.removeItem(cacheKey(target));
      return null;
    }
    return result;
  } catch {
    localStorage.removeItem(cacheKey(target));
    return null;
  }
}

function saveCachedResult(target: IssueTargetDto, result: IssueTriageResultDto) {
  try { localStorage.setItem(cacheKey(target), JSON.stringify(result)); } catch { /* cache is best effort */ }
}

function clearCachedResult(target: IssueTargetDto) {
  localStorage.removeItem(cacheKey(target));
}
