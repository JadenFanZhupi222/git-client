import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  cancelIssueTriage,
  listReviewModels,
  onReviewProgress,
  publishIssueTriage,
  startIssueTriage,
  type IpcError,
} from "../ipc";
import type {
  IssueContextDto,
  IssueSnapshotDto,
  IssueTargetDto,
  IssueTriagePublishResultDto,
  IssueTriageResultDto,
  ReviewLanguageDto,
  ReviewModelOptionDto,
  ReviewProgressEventDto,
} from "../bindings";
import { useLang, useT } from "../lib/i18n";
import { estimatedRunCost, formatEstimatedCost } from "../lib/agentCost";
import { CheckIcon, CloseIcon, SpinnerIcon } from "./icons";

const CONSENT_KEY = "issue-triage-consent-v1";
const CACHE_PREFIX = "issue-triage-result-v1";
type CredentialKind = "deepseek" | "github";
type Phase = "select" | "running" | "results" | "confirm" | "publishing" | "publish_result";
type AgentUiError = IpcError & { diagnostic_id?: string };

export function IssueTriageWorkspace({
  target,
  context,
  onClose,
  onConfigureCredential,
  onPublished,
}: {
  target: IssueTargetDto;
  context: IssueContextDto;
  onClose: () => void;
  onConfigureCredential: (kind: CredentialKind) => void;
  onPublished?: () => void;
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
  const [initialCache] = useState(() => loadCachedResult(target, context));
  const [result, setResult] = useState<IssueTriageResultDto | null>(initialCache.result);
  const [cacheStale, setCacheStale] = useState(initialCache.stale);
  const [error, setError] = useState<AgentUiError | null>(null);
  const [cancelledRun, setCancelledRun] = useState<AgentUiError | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const [selectedLabels, setSelectedLabels] = useState<string[]>([]);
  const [replySelected, setReplySelected] = useState(false);
  const [replyDraft, setReplyDraft] = useState("");
  const [publishId, setPublishId] = useState<string | null>(null);
  const [publishSnapshot, setPublishSnapshot] = useState<IssueSnapshotDto | null>(null);
  const [publishResult, setPublishResult] = useState<IssueTriagePublishResultDto | null>(null);

  const busy = phase === "running" || phase === "publishing";
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
    if (!result) return;
    setSelectedLabels([]);
    setReplySelected(false);
    setReplyDraft(result.proposal.suggested_reply);
    setPublishId(null);
    setPublishSnapshot(result.snapshot);
    setPublishResult(null);
  }, [result?.run_id]);

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
    setCancelledRun(null);
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
      setCacheStale(false);
      saveCachedResult(target, next);
      setPhase("results");
    } catch (reason) {
      cleanupListener();
      if (!mountedRef.current) return;
      const nextError = asIpcError(reason);
      setCancelling(false);
      setError(nextError.code === "CANCELLED" ? null : nextError);
      setCancelledRun(nextError.code === "CANCELLED" ? nextError : null);
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
    setCacheStale(false);
    setError(null);
    setCancelledRun(null);
    setPhase("select");
  }

  function toggleLabel(label: string) {
    setSelectedLabels((current) => current.includes(label)
      ? current.filter((item) => item !== label)
      : [...current, label]);
  }

  function reviewPublication() {
    if (selectedLabels.length === 0 && (!replySelected || !replyDraft.trim())) return;
    setError(null);
    setPhase("confirm");
  }

  async function publishSelection() {
    if (!result || !publishSnapshot || busy) return;
    const nextPublishId = publishId ?? createPublishId();
    setPublishId(nextPublishId);
    setError(null);
    setPhase("publishing");
    try {
      const next = await publishIssueTriage({
        publish_id: nextPublishId,
        confirmed: true,
        target,
        expected_snapshot: publishSnapshot,
        labels: selectedLabels,
        reply: replySelected && replyDraft.trim() ? replyDraft.trim() : null,
      });
      if (!mountedRef.current) return;
      setPublishResult(next);
      if (next.snapshot) setPublishSnapshot(next.snapshot);
      clearCachedResult(target);
      setPhase("publish_result");
      if (next.actions.some((action) => action.status === "applied" || action.status === "already_applied")) {
        onPublished?.();
      }
    } catch (reason) {
      if (!mountedRef.current) return;
      setError(asIpcError(reason));
      setPhase("confirm");
    }
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
              {cacheStale && (
                <p role="status" className="mt-4 rounded-md border border-warning/40 bg-warning/10 p-3 text-xs text-warning">
                  {t("issueTriage.cacheStale")}
                </p>
              )}
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

          {phase === "results" && result && (
            <TriageResults
              result={result}
              models={models}
              currentLabels={context.issue.labels.map((label) => label.name)}
              selectedLabels={selectedLabels}
              onToggleLabel={toggleLabel}
              replySelected={replySelected}
              onReplySelected={setReplySelected}
              replyDraft={replyDraft}
              onReplyDraft={setReplyDraft}
            />
          )}
          {phase === "confirm" && result && (
            <PublishConfirmation labels={selectedLabels} reply={replySelected ? replyDraft.trim() : ""} />
          )}
          {phase === "publishing" && (
            <section className="grid min-h-56 place-items-center text-center" aria-live="polite">
              <div className="flex flex-col items-center">
                <SpinnerIcon width={24} height={24} />
                <p className="mt-3 text-sm text-fg">{t("issueTriage.publish.running")}</p>
              </div>
            </section>
          )}
          {phase === "publish_result" && publishResult && <PublishResults result={publishResult} onConfigureCredential={onConfigureCredential} />}
          {cancelledRun && <CancellationNotice error={cancelledRun} />}
          {error && <ErrorNotice error={error} onConfigureCredential={onConfigureCredential} onRetry={error.code === "ISSUE_UPDATED" ? onClose : undefined} />}
        </main>

        <footer className="flex items-center justify-end gap-2 border-t border-line px-5 py-3">
          {phase === "running" ? (
            <button disabled={cancelling} onClick={cancelTriage} className="rounded-md border border-danger/50 px-3 py-1.5 text-xs text-danger disabled:opacity-50">{cancelling ? t("issueTriage.cancelling") : t("issueTriage.cancel")}</button>
          ) : phase === "results" ? (
            <>
              <button onClick={onClose} className="mr-auto rounded-md px-3 py-1.5 text-xs text-fg-muted hover:bg-overlay hover:text-fg">{t("issueTriage.done")}</button>
              <button onClick={analyzeAgain} className="rounded-md border border-line-strong px-3 py-1.5 text-xs text-fg-muted hover:bg-overlay hover:text-fg">{t("issueTriage.again")}</button>
              <button
                disabled={selectedLabels.length === 0 && (!replySelected || !replyDraft.trim())}
                onClick={reviewPublication}
                className="rounded-md bg-accent px-4 py-1.5 text-xs font-semibold text-on-accent disabled:opacity-40"
              >{t("issueTriage.publish.review")}</button>
            </>
          ) : phase === "confirm" ? (
            <>
              <button onClick={() => setPhase("results")} className="rounded-md border border-line-strong px-3 py-1.5 text-xs text-fg-muted hover:bg-overlay hover:text-fg">{t("issueTriage.publish.back")}</button>
              <button disabled={error?.code === "ISSUE_UPDATED"} onClick={() => void publishSelection()} className="rounded-md bg-accent px-4 py-1.5 text-xs font-semibold text-on-accent disabled:opacity-40">{t("issueTriage.publish.confirm")}</button>
            </>
          ) : phase === "publishing" ? (
            <button disabled className="rounded-md bg-accent px-4 py-1.5 text-xs font-semibold text-on-accent opacity-50">{t("issueTriage.publish.running")}</button>
          ) : phase === "publish_result" && publishResult ? (
            publishResult.actions.some((action) => action.status === "failed") ? (
              <>
                <button onClick={onClose} className="rounded-md border border-line-strong px-3 py-1.5 text-xs text-fg-muted hover:bg-overlay hover:text-fg">{t("issueTriage.done")}</button>
                <button disabled={!publishResult.snapshot} onClick={() => void publishSelection()} className="rounded-md bg-accent px-4 py-1.5 text-xs font-semibold text-on-accent disabled:opacity-40">{t("issueTriage.publish.retry")}</button>
              </>
            ) : (
              <button onClick={onClose} className="rounded-md bg-accent px-4 py-1.5 text-xs font-semibold text-on-accent">{t("issueTriage.done")}</button>
            )
          ) : (
            <button disabled={!consented || !selectedModelId} onClick={startTriage} className="rounded-md bg-accent px-4 py-1.5 text-xs font-semibold text-on-accent disabled:opacity-40">{t("issueTriage.start")}</button>
          )}
        </footer>
      </div>
    </div>
  );
}

function TriageResults({
  result,
  models,
  currentLabels,
  selectedLabels,
  onToggleLabel,
  replySelected,
  onReplySelected,
  replyDraft,
  onReplyDraft,
}: {
  result: IssueTriageResultDto;
  models: ReviewModelOptionDto[];
  currentLabels: string[];
  selectedLabels: string[];
  onToggleLabel: (label: string) => void;
  replySelected: boolean;
  onReplySelected: (selected: boolean) => void;
  replyDraft: string;
  onReplyDraft: (value: string) => void;
}) {
  const t = useT();
  const lang = useLang();
  const proposal = result.proposal;
  const cost = estimatedRunCost(result.usage, result.model_id, models);
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
          {proposal.suggested_labels.length > 0 && (
            <div className="flex flex-wrap gap-2">
              {proposal.suggested_labels.map((label) => {
                const present = currentLabels.includes(label);
                return present ? (
                  <span key={label} className="rounded-full border border-line px-2 py-1 text-[11px] text-fg-subtle">
                    {label} · {t("issueTriage.publish.alreadyPresent")}
                  </span>
                ) : (
                  <label key={label} className="flex cursor-pointer items-center gap-2 rounded-md border border-line-strong px-2.5 py-1.5 text-xs text-fg-muted hover:bg-overlay">
                    <input type="checkbox" checked={selectedLabels.includes(label)} onChange={() => onToggleLabel(label)} />
                    <span>{t("issueTriage.publish.addLabel", { label })}</span>
                  </label>
                );
              })}
            </div>
          )}
        </ResultSection>
        <ResultSection title={t("issueTriage.duplicates")} empty={t("issueTriage.none")}>
          {proposal.suspected_duplicate_numbers.length > 0 && <p className="font-mono text-xs text-fg">{proposal.suspected_duplicate_numbers.map((number) => `#${number}`).join(", ")}</p>}
        </ResultSection>
        <ResultSection title={t("issueTriage.reply")} empty={t("issueTriage.none")}>
          {proposal.suggested_reply && (
            <div>
              <label className="flex items-center gap-2 text-xs font-medium text-fg">
                <input type="checkbox" checked={replySelected} onChange={(event) => onReplySelected(event.currentTarget.checked)} />
                <span>{t("issueTriage.publish.postReply")}</span>
              </label>
              <textarea
                aria-label={t("issueTriage.publish.replyDraft")}
                value={replyDraft}
                maxLength={20_000}
                onChange={(event) => onReplyDraft(event.currentTarget.value)}
                className="field mt-2 min-h-28 w-full resize-y rounded-md border border-line-strong bg-canvas px-3 py-2 text-xs leading-5 text-fg"
              />
              <p className="mt-1 text-right font-mono text-[10px] text-fg-subtle">{replyDraft.length.toLocaleString()} / 20,000</p>
            </div>
          )}
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
      {result.diagnostic_id && <p className="mt-1 font-mono text-[10px] text-fg-subtle">{t("issueTriage.diagnostics", { duration: result.duration_ms, attempts: result.provider_attempts, id: result.diagnostic_id })}</p>}
      {cost && <p className="mt-1 text-[11px] text-fg-subtle">{t("issueTriage.estimatedCost", { cost: formatEstimatedCost(cost, lang === "zh" ? "zh-CN" : "en-US") })}</p>}
    </section>
  );
}

function PublishConfirmation({ labels, reply }: { labels: string[]; reply: string }) {
  const t = useT();
  return (
    <section aria-labelledby="issue-publish-confirm-title">
      <h3 id="issue-publish-confirm-title" className="text-base font-semibold text-fg">{t("issueTriage.publish.confirmTitle")}</h3>
      <p className="mt-1 max-w-2xl text-xs leading-5 text-fg-muted">{t("issueTriage.publish.confirmDetail")}</p>
      <div className="mt-5 divide-y divide-line rounded-md border border-line-strong">
        {labels.map((label) => (
          <div key={label} className="flex items-center justify-between gap-4 px-4 py-3 text-xs">
            <span className="text-fg-muted">{t("issueTriage.publish.labelAction")}</span>
            <span className="rounded-full border border-line-strong px-2 py-0.5 font-medium text-fg">{label}</span>
          </div>
        ))}
        {reply && (
          <div className="px-4 py-3">
            <p className="text-xs text-fg-muted">{t("issueTriage.publish.commentAction")}</p>
            <p className="mt-2 max-h-48 overflow-y-auto whitespace-pre-wrap break-words text-xs leading-5 text-fg">{reply}</p>
          </div>
        )}
      </div>
      <p className="mt-4 rounded-md border border-warning/40 bg-warning/10 p-3 text-xs leading-5 text-warning">{t("issueTriage.publish.irreversible")}</p>
    </section>
  );
}

function PublishResults({ result, onConfigureCredential }: { result: IssueTriagePublishResultDto; onConfigureCredential: (kind: CredentialKind) => void }) {
  const t = useT();
  const failed = result.actions.some((action) => action.status === "failed");
  const authFailed = result.actions.some((action) => action.status === "failed" && action.error_code === "AUTH_FAILED");
  return (
    <section aria-labelledby="issue-publish-result-title">
      <div className="flex items-start gap-3">
        <div className={`grid h-9 w-9 shrink-0 place-items-center rounded-full ${failed ? "bg-warning/12 text-warning" : "bg-success/12 text-success"}`}>
          <CheckIcon width={18} height={18} />
        </div>
        <div>
          <h3 id="issue-publish-result-title" className="text-base font-semibold text-fg">{failed ? t("issueTriage.publish.partialTitle") : t("issueTriage.publish.successTitle")}</h3>
          <p className="mt-1 text-xs leading-5 text-fg-muted">{failed ? t("issueTriage.publish.partialDetail") : t("issueTriage.publish.successDetail")}</p>
        </div>
      </div>
      <ul className="mt-5 divide-y divide-line rounded-md border border-line-strong">
        {result.actions.map((action) => (
          <li key={action.action_id} className="flex items-center gap-3 px-4 py-3 text-xs">
            <span className="min-w-0 flex-1 text-fg">
              <span className="block">{action.kind === "label" ? t("issueTriage.publish.addLabel", { label: action.label ?? "" }) : t("issueTriage.publish.postReply")}</span>
              {action.error_code && <span className="mt-0.5 block max-w-[75ch] text-[11px] leading-4 text-fg-subtle">{publishErrorMessage(action.error_code, t)}</span>}
            </span>
            <span className={action.status === "failed" ? "text-danger" : action.status === "already_applied" ? "text-fg-subtle" : "text-success"}>
              {t(`issueTriage.publish.status.${action.status}` as Parameters<typeof t>[0])}
            </span>
          </li>
        ))}
      </ul>
      {authFailed && (
        <button onClick={() => onConfigureCredential("github")} className="mt-3 rounded-md border border-line-strong px-3 py-1.5 text-xs text-fg-muted hover:bg-overlay hover:text-fg">
          {t("issueTriage.publish.openGithubSettings")}
        </button>
      )}
      {failed && !result.snapshot && <p className="mt-4 text-xs leading-5 text-danger">{t("issueTriage.publish.refreshRequired")}</p>}
    </section>
  );
}

function ResultFact({ label, value }: { label: string; value: string }) {
  return <div className="rounded-md border border-line bg-elevated/40 p-3"><p className="text-[10px] uppercase tracking-wide text-fg-subtle">{label}</p><p className="mt-1 text-sm font-semibold text-fg">{value}</p></div>;
}

function ResultSection({ title, empty, children }: { title: string; empty: string; children?: React.ReactNode }) {
  return <section className="rounded-md border border-line p-3"><h4 className="text-xs font-semibold text-fg">{title}</h4><div className="mt-2">{children || <p className="text-xs text-fg-subtle">{empty}</p>}</div></section>;
}

function CancellationNotice({ error }: { error: AgentUiError }) {
  const t = useT();
  return <div role="status" className="mt-4 rounded-md border border-line-strong bg-elevated/60 p-3 text-xs text-fg-muted"><p>{t("issueTriage.cancelledNotice")}</p>{error.diagnostic_id && <p className="mt-1 font-mono text-[10px] text-fg-subtle">{t("issueTriage.errorDiagnostic", { id: error.diagnostic_id })}</p>}</div>;
}

function ErrorNotice({ error, onConfigureCredential, onRetry }: { error: AgentUiError; onConfigureCredential: (kind: CredentialKind) => void; onRetry?: () => void }) {
  const t = useT();
  const credential = error.code === "AI_KEY_MISSING" ? "deepseek" : error.code === "GITHUB_TOKEN_MISSING" ? "github" : null;
  return (
    <div role="alert" className="mt-4 rounded-md border border-danger/40 bg-danger/10 p-3 text-xs text-danger">
      <p>{errorMessage(error, t)}</p>
      {error.diagnostic_id && <p className="mt-1 font-mono text-[10px]">{t("issueTriage.errorDiagnostic", { id: error.diagnostic_id })}</p>}
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

function errorMessage(error: AgentUiError, t: ReturnType<typeof useT>) {
  const known = ["AI_KEY_MISSING", "GITHUB_TOKEN_MISSING", "ISSUE_UPDATED", "ISSUE_NOT_FOUND", "ISSUE_TRIAGE_BUDGET_EXCEEDED", "ISSUE_PUBLISH_FAILED", "NETWORK_ERROR", "RATE_LIMITED", "AUTH_FAILED", "INVALID_MODEL_OUTPUT", "INVALID_REVIEW_MODEL", "AGENT_RESOURCE_BUSY", "CANCELLED"];
  return known.includes(error.code) ? t(`issueTriage.error.${error.code}` as Parameters<typeof t>[0]) : error.message;
}

function publishErrorMessage(code: string, t: ReturnType<typeof useT>) {
  if (code === "AUTH_FAILED") return t("issueTriage.publish.error.AUTH_FAILED");
  return errorMessage({ code, message: code, recoverable: true }, t);
}

function asIpcError(reason: unknown): AgentUiError {
  const candidate = reason as Partial<AgentUiError> | null;
  return { code: candidate?.code ?? "UNKNOWN", message: candidate?.message ?? String(reason), recoverable: candidate?.recoverable ?? true, diagnostic_id: typeof candidate?.diagnostic_id === "string" ? candidate.diagnostic_id : undefined };
}

function createRunId() {
  return globalThis.crypto?.randomUUID?.() ?? `issue-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function createPublishId() {
  return globalThis.crypto?.randomUUID?.() ?? `publish-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function cacheKey(target: IssueTargetDto) {
  return `${CACHE_PREFIX}:${target.owner}/${target.repo}#${target.issue_number}`;
}

function loadCachedResult(target: IssueTargetDto, context: IssueContextDto): { result: IssueTriageResultDto | null; stale: boolean } {
  try {
    const value = localStorage.getItem(cacheKey(target));
    if (!value) return { result: null, stale: false };
    const cached = JSON.parse(value) as Partial<IssueTriageResultDto>;
    const result = {
      ...cached,
      model_id: typeof cached.model_id === "string" ? cached.model_id : "",
      duration_ms: typeof cached.duration_ms === "number" ? cached.duration_ms : 0,
      diagnostic_id: typeof cached.diagnostic_id === "string" ? cached.diagnostic_id : "",
      provider_attempts: typeof cached.provider_attempts === "number" ? cached.provider_attempts : 0,
    } as IssueTriageResultDto;
    if (result.snapshot.updated_at !== context.snapshot.updated_at || result.snapshot.comments !== context.snapshot.comments) {
      localStorage.removeItem(cacheKey(target));
      return { result: null, stale: true };
    }
    return { result, stale: false };
  } catch {
    localStorage.removeItem(cacheKey(target));
    return { result: null, stale: false };
  }
}

function saveCachedResult(target: IssueTargetDto, result: IssueTriageResultDto) {
  try { localStorage.setItem(cacheKey(target), JSON.stringify(result)); } catch { /* cache is best effort */ }
}

function clearCachedResult(target: IssueTargetDto) {
  localStorage.removeItem(cacheKey(target));
}
