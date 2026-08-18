import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  cancelPrReview,
  getGitlabReviewPreflight,
  getReviewPreflight,
  listReviewModels,
  onReviewProgress,
  startGitlabMrReview,
  startPrReview,
  submitGitlabMrReview,
  submitPrReview,
  type IpcError,
} from "../ipc";
import type {
  CredentialKindDto,
  PublishedReviewDto,
  ReviewFindingDto,
  ReviewLanguageDto,
  ReviewModelOptionDto,
  ReviewPreflightDto,
  ReviewProgressEventDto,
  ReviewRunResultDto,
  ReviewTargetDto,
} from "../bindings";
import { useLang, useT } from "../lib/i18n";
import { estimatedRunCost, formatEstimatedCost } from "../lib/agentCost";
import { useAgentStream } from "../hooks/useAgentStream";
import {
  clearCachedReview,
  loadCachedReview,
  saveCachedReview,
  type ReviewPlatform,
} from "../lib/prReviewCache";
import { CheckIcon, CloseIcon, SpinnerIcon } from "./icons";
import { AgentModelPicker } from "./AgentModelPicker";
import { AgentStreamPanel } from "./AgentStreamPanel";

const CONSENT_KEY = "pr-review-consent-v1";
const MAX_FILES = 30;
const MAX_PATCH_BYTES = 200_000;

type CredentialKind = CredentialKindDto;
type Phase = "preflight" | "select" | "running" | "results" | "published";
type AgentUiError = IpcError & { diagnostic_id?: string };

type FindingDraft = {
  finding: ReviewFindingDto;
  selected: boolean;
  comment: string;
};

export function PrReviewWorkspace({
  target,
  onClose,
  onConfigureCredential,
  onFocusReady,
  platform = "github",
}: {
  target: ReviewTargetDto;
  onClose: () => void;
  onConfigureCredential: (kind: CredentialKind) => void;
  onFocusReady?: () => void;
  platform?: ReviewPlatform;
}) {
  const t = useT();
  const lang = useLang();
  const dialogRef = useRef<HTMLDivElement>(null);
  const selectAllRef = useRef<HTMLInputElement>(null);
  const mountedRef = useRef(true);
  const runIdRef = useRef<string | null>(null);
  const unsubscribeRef = useRef<(() => void) | null>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);
  const [phase, setPhase] = useState<Phase>("preflight");
  const [preflight, setPreflight] = useState<ReviewPreflightDto | null>(null);
  const [selectedFiles, setSelectedFiles] = useState<Set<string>>(new Set());
  const [models, setModels] = useState<ReviewModelOptionDto[]>([]);
  const [selectedModelId, setSelectedModelId] = useState("");
  const [outputLanguage, setOutputLanguage] = useState<ReviewLanguageDto>(
    lang === "zh" ? "simplified_chinese" : "english",
  );
  const [consented, setConsented] = useState(() => localStorage.getItem(CONSENT_KEY) === "accepted");
  const [error, setError] = useState<AgentUiError | null>(null);
  const [cancelledRun, setCancelledRun] = useState<AgentUiError | null>(null);
  const [progress, setProgress] = useState<ReviewProgressEventDto | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const [result, setResult] = useState<ReviewRunResultDto | null>(null);
  const [drafts, setDrafts] = useState<FindingDraft[]>([]);
  const [submitting, setSubmitting] = useState(false);
  const [published, setPublished] = useState<PublishedReviewDto | null>(null);
  const agentStream = useAgentStream();

  const busy = phase === "running" || submitting;
  const getPreflightRequest = platform === "gitlab" ? getGitlabReviewPreflight : getReviewPreflight;
  const startReviewRequest = platform === "gitlab" ? startGitlabMrReview : startPrReview;
  const submitReviewRequest = platform === "gitlab" ? submitGitlabMrReview : submitPrReview;

  useLayoutEffect(() => {
    mountedRef.current = true;
    previousFocusRef.current = document.activeElement as HTMLElement | null;
    dialogRef.current?.focus();
    onFocusReady?.();
    return () => {
      mountedRef.current = false;
      cleanupListener();
      const runId = runIdRef.current;
      runIdRef.current = null;
      if (runId) void cancelPrReview(runId).catch(() => undefined);
      previousFocusRef.current?.focus();
    };
  }, []);

  useEffect(() => {
    void loadPreflight();
  }, [target.owner, target.repo, target.pull_number, platform]);

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        if (!busy) onClose();
        return;
      }
      if (event.key !== "Tab" || !dialogRef.current) return;
      const focusable = Array.from(
        dialogRef.current.querySelectorAll<HTMLElement>(
          'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [href], [tabindex]:not([tabindex="-1"])',
        ),
      );
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (document.activeElement === dialogRef.current) {
        event.preventDefault();
        (event.shiftKey ? last : first).focus();
      } else if (event.shiftKey && document.activeElement === first) {
        event.preventDefault(); last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault(); first.focus();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [busy, onClose]);

  function cleanupListener() {
    unsubscribeRef.current?.();
    unsubscribeRef.current = null;
  }

  async function loadPreflight() {
    cleanupListener();
    setPhase("preflight");
    setPreflight(null);
    setSelectedFiles(new Set());
    setError(null);
    setCancelledRun(null);
    setProgress(null);
    setResult(null);
    setDrafts([]);
    setPublished(null);
    try {
      const [next, nextModels] = await Promise.all([
        getPreflightRequest(target),
        listReviewModels(),
      ]);
      if (!mountedRef.current) return;
      setPreflight(next);
      setModels(nextModels);
      setSelectedModelId((current) => (
        nextModels.some((model) => model.id === current) ? current : (nextModels[0]?.id ?? "")
      ));
      const cached = loadCachedReview(target, next.head_sha, platform);
      if (cached) {
        const reviewable = new Set(
          next.files.filter((file) => file.reviewable).map((file) => file.path),
        );
        setSelectedFiles(
          new Set(cached.result.reviewed_files.filter((path) => reviewable.has(path))),
        );
        setSelectedModelId(
          nextModels.some((model) => model.id === cached.modelId)
            ? cached.modelId
            : (nextModels[0]?.id ?? ""),
        );
        setOutputLanguage(cached.outputLanguage);
        setResult(cached.result);
        setDrafts(cached.drafts);
        setPhase("results");
      } else {
        setSelectedFiles(
          new Set(next.requires_selection ? [] : next.files.filter((file) => file.reviewable).map((file) => file.path)),
        );
        setPhase("select");
      }
    } catch (reason) {
      if (mountedRef.current) setError(asIpcError(reason));
    }
  }

  const selectedPatchBytes = useMemo(() => {
    if (!preflight) return 0;
    return preflight.files.reduce(
      (sum, file) => sum + (selectedFiles.has(file.path) ? file.patch_bytes : 0),
      0,
    );
  }, [preflight, selectedFiles]);
  const reviewableFiles = useMemo(
    () => preflight?.files.filter((file) => file.reviewable) ?? [],
    [preflight],
  );
  const allReviewableSelected = reviewableFiles.length > 0
    && reviewableFiles.every((file) => selectedFiles.has(file.path));
  const selectedReviewableCount = reviewableFiles.filter((file) => selectedFiles.has(file.path)).length;
  const tokenEstimate = useMemo(
    () => estimateInitialInputTokens(selectedPatchBytes, selectedFiles.size),
    [selectedPatchBytes, selectedFiles.size],
  );

  useEffect(() => {
    if (selectAllRef.current) {
      selectAllRef.current.indeterminate = selectedReviewableCount > 0 && !allReviewableSelected;
    }
  }, [allReviewableSelected, selectedReviewableCount]);

  useEffect(() => {
    if (phase !== "results" || !result) return;
    saveCachedReview(target, {
      version: 1,
      headSha: result.head_sha,
      modelId: selectedModelId,
      outputLanguage,
      result,
      drafts,
    }, platform);
  }, [
    drafts,
    outputLanguage,
    platform,
    phase,
    result,
    selectedModelId,
    target.owner,
    target.pull_number,
    target.repo,
  ]);
  const selectionError = selectedFiles.size > MAX_FILES
    ? t("prReview.limitFiles", { count: selectedFiles.size, limit: MAX_FILES })
    : selectedPatchBytes > MAX_PATCH_BYTES
      ? t("prReview.limitBytes", { count: selectedPatchBytes.toLocaleString(), limit: MAX_PATCH_BYTES.toLocaleString() })
      : null;

  function toggleFile(path: string) {
    setSelectedFiles((current) => {
      const next = new Set(current);
      if (next.has(path)) next.delete(path); else next.add(path);
      return next;
    });
  }

  function toggleAllFiles() {
    setSelectedFiles(() => {
      if (allReviewableSelected) return new Set();
      return new Set(reviewableFiles.map((file) => file.path));
    });
  }

  function acceptConsent() {
    localStorage.setItem(CONSENT_KEY, "accepted");
    setConsented(true);
  }

  async function startReview() {
    if (!preflight || !consented || !selectedModelId || selectedFiles.size === 0 || selectionError) return;
    setError(null);
    setCancelledRun(null);
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
      await agentStream.begin(runId);
      const next = await startReviewRequest({
        run_id: runId,
        target,
        expected_head_sha: preflight.head_sha,
        selected_files: Array.from(selectedFiles),
        model_id: selectedModelId,
        output_language: outputLanguage,
      });
      cleanupListener();
      agentStream.end();
      if (!mountedRef.current) return;
      setResult(next);
      setDrafts(sortFindings(next.findings).map((finding) => ({ finding, selected: true, comment: finding.draft_comment })));
      setPhase("results");
      setCancelling(false);
    } catch (reason) {
      cleanupListener();
      agentStream.end();
      if (!mountedRef.current) return;
      const nextError = asIpcError(reason);
      setCancelling(false);
      if (nextError.code === "CANCELLED") {
        setError(null);
        setCancelledRun(nextError);
        setPhase("select");
      } else {
        setError(nextError);
        setPhase("select");
      }
    } finally {
      if (runIdRef.current === runId) runIdRef.current = null;
    }
  }

  async function cancelReview() {
    if (!runIdRef.current || cancelling) return;
    setCancelling(true);
    try {
      await cancelPrReview(runIdRef.current);
    } catch (reason) {
      if (mountedRef.current) {
        setCancelling(false);
        setError(asIpcError(reason));
      }
    }
  }

  async function submitReview() {
    if (!result || submitting) return;
    const findings = drafts
      .filter((draft) => draft.selected && draft.comment.trim())
      .map((draft) => ({ ...draft.finding, draft_comment: draft.comment.trim() }));
    if (findings.length === 0) return;
    setSubmitting(true);
    setError(null);
    try {
      const next = await submitReviewRequest({ target, head_sha: result.head_sha, findings });
      if (!mountedRef.current) return;
      clearCachedReview(target, platform);
      setPublished(next);
      setPhase("published");
    } catch (reason) {
      if (mountedRef.current) setError(asIpcError(reason));
    } finally {
      if (mountedRef.current) setSubmitting(false);
    }
  }

  function requestClose() {
    if (!busy) onClose();
  }

  function reviewAgain() {
    clearCachedReview(target, platform);
    setResult(null);
    setDrafts([]);
    setPublished(null);
    setError(null);
    setCancelledRun(null);
    setPhase("select");
  }

  const selectedFindings = drafts.filter((draft) => draft.selected && draft.comment.trim()).length;

  return (
    <div data-testid="pr-review-backdrop" className="overlay-in fixed inset-0 z-[70] flex items-center justify-center bg-black/50 p-4" onClick={(event) => { event.stopPropagation(); requestClose(); }}>
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="pr-review-title"
        tabIndex={-1}
        className="panel-in popover flex max-h-[90vh] w-full max-w-4xl flex-col overflow-hidden rounded-lg border border-line-strong bg-canvas outline-none"
        onClick={(event) => event.stopPropagation()}
      >
        <header className="flex items-center gap-3 border-b border-line px-5 py-4">
          <div className="min-w-0">
            <h2 id="pr-review-title" className="text-base font-semibold text-fg">{t("prReview.title")}</h2>
            <p className="truncate text-xs text-fg-subtle">{target.owner}/{target.repo} · #{target.pull_number}</p>
          </div>
          <button aria-label={t("prReview.closeAria")} disabled={busy} onClick={requestClose} className="ml-auto grid h-8 w-8 place-items-center rounded text-fg-muted hover:bg-overlay hover:text-fg disabled:opacity-40">
            <CloseIcon width={14} height={14} />
          </button>
        </header>

        <main className="min-h-0 flex-1 overflow-y-auto p-5">
          {phase === "preflight" && !preflight && !error && <StatusLine text={t("prReview.preflightLoading")} />}
          {preflight && phase !== "published" && (
            <div className="mb-4 flex flex-wrap gap-3 rounded-md border border-line bg-elevated/60 px-3 py-2 text-xs text-fg-muted">
              <span>{t("prReview.headSha")}: <code className="text-fg">{preflight.head_sha.slice(0, 7)}</code></span>
              <span>{t("prReview.changedFiles", { count: preflight.files.length })}</span>
              <span>{t("prReview.reviewableFiles", { count: preflight.files.filter((file) => file.reviewable).length })}</span>
              <span>{t("prReview.patchBytes", { count: preflight.total_patch_bytes.toLocaleString() })}</span>
            </div>
          )}

          {phase === "select" && preflight && (
            <section aria-labelledby="pr-review-files">
              <div className="mb-4 flex flex-wrap items-start gap-3 border-y border-line py-3">
                <AgentModelPicker
                  id="pr-review-model"
                  label={t("prReview.model")}
                  models={models}
                  value={selectedModelId}
                  onChange={setSelectedModelId}
                  onConfigureCredential={onConfigureCredential}
                />
                <label className="grid min-w-40 gap-1 text-[11px] font-medium text-fg-muted">
                  {t("prReview.outputLanguage")}
                  <select
                    aria-label={t("prReview.outputLanguage")}
                    value={outputLanguage}
                    onChange={(event) => setOutputLanguage(event.currentTarget.value as ReviewLanguageDto)}
                    className="h-8 rounded-md border border-line bg-canvas px-2 text-xs font-normal text-fg outline-none focus:border-accent"
                  >
                    <option value="simplified_chinese">{t("prReview.language.zh")}</option>
                    <option value="english">{t("prReview.language.en")}</option>
                  </select>
                </label>
                <div className="ml-auto pb-0.5 text-right text-[11px] text-fg-muted" aria-live="polite">
                  <p className="font-medium text-fg">{t("prReview.estimatedTokens", { range: tokenEstimate ? formatTokenRange(tokenEstimate, lang) : "—" })}</p>
                  <p>{t("prReview.estimateHint")}</p>
                </div>
              </div>
              <h3 id="pr-review-files" className="text-sm font-semibold text-fg">{t("prReview.selectFiles")}</h3>
              {preflight.requires_selection && <p className="mt-1 text-xs text-fg-subtle">{t("prReview.selectionRequired")}</p>}
              <label className="mt-3 flex cursor-pointer items-center gap-3 rounded-t-md border border-line bg-elevated/60 px-3 py-2 text-xs text-fg">
                <input
                  ref={selectAllRef}
                  type="checkbox"
                  aria-label={t("prReview.selectAllAria")}
                  checked={allReviewableSelected}
                  disabled={reviewableFiles.length === 0}
                  onChange={toggleAllFiles}
                />
                <span className="font-medium">{t("prReview.selectAll")}</span>
                <span className="ml-auto text-fg-subtle">{t("prReview.reviewableFiles", { count: reviewableFiles.length })}</span>
              </label>
              <div className="max-h-64 overflow-y-auto rounded-b-md border border-t-0 border-line">
                {preflight.files.map((file) => (
                  <label key={file.path} className="flex cursor-pointer items-center gap-3 border-b border-line px-3 py-2 text-xs last:border-b-0 has-[:disabled]:cursor-not-allowed has-[:disabled]:opacity-55">
                    <input type="checkbox" aria-label={file.reviewable ? file.path : `${file.path} — ${t("prReview.notReviewable")}`} checked={selectedFiles.has(file.path)} disabled={!file.reviewable} onChange={() => toggleFile(file.path)} />
                    <span className="min-w-0 flex-1 truncate font-mono text-fg">{file.path}</span>
                    <span className="text-fg-subtle">{file.reviewable ? t("prReview.fileBytes", { count: file.patch_bytes.toLocaleString() }) : t("prReview.notReviewable")}</span>
                  </label>
                ))}
              </div>
              <p aria-live="polite" className="mt-2 text-xs text-fg-muted">{t("prReview.selected", { count: selectedFiles.size, bytes: selectedPatchBytes.toLocaleString() })}</p>
              {selectionError && <p className="mt-1 text-xs text-danger">{selectionError}</p>}
              {!consented && (
                <label className="mt-4 flex items-start gap-2 rounded-md border border-accent/40 bg-accent/10 p-3 text-xs text-fg">
                  <input type="checkbox" className="mt-0.5" checked={false} onChange={acceptConsent} />
                  <span>{t("prReview.consent")}</span>
                </label>
              )}
            </section>
          )}

          {phase === "running" && (
            <section className="flex min-h-44 flex-col items-center justify-center gap-5 py-5 text-center" aria-live="polite">
              <div className="flex flex-col items-center"><SpinnerIcon width={24} height={24} /><p className="mt-3 text-sm text-fg">{progressLabel(progress, t)}</p></div>
              <AgentStreamPanel stream={agentStream.stream} />
            </section>
          )}

          {phase === "results" && result && (
            <Results result={result} models={models} drafts={drafts} setDrafts={setDrafts} disabled={submitting} />
          )}

          {phase === "published" && published && (
            <section className="grid min-h-48 place-items-center text-center">
              <div><h3 className="text-lg font-semibold text-fg">{t("prReview.published")}</h3><p className="mt-2 text-xs text-fg-subtle">{t("prReview.reviewId", { id: published.review_id })}</p>{published.html_url && <button onClick={() => void openUrl(published.html_url!)} className="mt-4 rounded-md bg-accent px-4 py-2 text-xs font-semibold text-on-accent">{t("prReview.openGithub")}</button>}</div>
            </section>
          )}

          {cancelledRun && <CancellationNotice error={cancelledRun} />}
          {error && <ErrorNotice error={error} onRetry={error.code === "PR_UPDATED" ? loadPreflight : phase === "preflight" ? loadPreflight : undefined} onConfigureCredential={onConfigureCredential} />}
        </main>

        <footer className="flex items-center justify-end gap-2 border-t border-line px-5 py-3">
          {phase === "running" ? (
            <button disabled={cancelling} onClick={cancelReview} className="rounded-md border border-danger/50 px-3 py-1.5 text-xs text-danger disabled:opacity-50">{cancelling ? t("prReview.cancelling") : t("prReview.cancel")}</button>
          ) : phase === "results" ? (
            <>
              <button disabled={submitting} onClick={reviewAgain} className="rounded-md border border-line-strong px-3 py-1.5 text-xs text-fg-muted hover:bg-overlay hover:text-fg disabled:opacity-40">{t("prReview.reviewAgain")}</button>
              {drafts.length === 0 ? (
                <button onClick={requestClose} className="rounded-md bg-accent px-4 py-1.5 text-xs font-semibold text-on-accent">{t("prReview.done")}</button>
              ) : (
                <button disabled={submitting || selectedFindings === 0} onClick={submitReview} className="rounded-md bg-accent px-4 py-1.5 text-xs font-semibold text-on-accent disabled:opacity-40">{submitting ? t("prReview.submitting") : t("prReview.submit")}</button>
              )}
            </>
          ) : phase === "select" ? (
            <button disabled={!consented || !selectedModelId || selectedFiles.size === 0 || Boolean(selectionError)} onClick={startReview} className="rounded-md bg-accent px-4 py-1.5 text-xs font-semibold text-on-accent disabled:opacity-40">{t("prReview.start")}</button>
          ) : null}
        </footer>
      </div>
    </div>
  );
}

function Results({ result, models, drafts, setDrafts, disabled }: { result: ReviewRunResultDto; models: ReviewModelOptionDto[]; drafts: FindingDraft[]; setDrafts: React.Dispatch<React.SetStateAction<FindingDraft[]>>; disabled: boolean }) {
  const t = useT();
  const lang = useLang();
  const cost = estimatedRunCost(result.usage, result.model_id, models);
  if (drafts.length === 0) return <NoFindingsResult result={result} models={models} />;
  return <section aria-labelledby="pr-review-findings">
    <h3 id="pr-review-findings" className="text-sm font-semibold text-fg">{t("prReview.findings")}</h3>
    <p className="mt-1 text-xs text-fg-subtle">{result.summary} · {t("prReview.usage", { input: result.usage.input_tokens, output: result.usage.output_tokens, tools: result.usage.tool_calls })}</p>
    <p className="mt-1 break-words text-xs text-fg-subtle">{t("prReview.reviewedFiles", { files: result.reviewed_files.join(", ") })}</p>
    {result.diagnostic_id && <p className="mt-1 font-mono text-[10px] text-fg-subtle">{t("prReview.diagnostics", { duration: result.duration_ms, attempts: result.provider_attempts, id: result.diagnostic_id })}</p>}
    {cost && <p className="mt-1 text-[11px] text-fg-subtle">{t("prReview.estimatedCost", { cost: formatEstimatedCost(cost, lang === "zh" ? "zh-CN" : "en-US") })}</p>}
    <p className="mt-1 text-[11px] text-fg-subtle">{t("prReview.savedLocally")}</p>
    <div className="mt-4 grid gap-3">{drafts.map((draft, index) => <article key={draft.finding.id} role="group" aria-label={`${severityLabel(draft.finding.severity, t)}: ${draft.finding.title}`} className="rounded-md border border-line bg-elevated/50 p-4">
      <div className="flex items-start gap-3"><input aria-label={t("prReview.includeFinding", { title: draft.finding.title })} type="checkbox" checked={draft.selected} disabled={disabled} onChange={(event) => { const selected = event.currentTarget.checked; setDrafts((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, selected } : item)); }} /><div className="min-w-0 flex-1"><div className="flex flex-wrap items-center gap-2"><span className="rounded bg-overlay px-1.5 py-0.5 text-[10px] font-semibold uppercase text-fg-muted">{severityLabel(draft.finding.severity, t)}</span><code className="text-[11px] text-fg-subtle">{draft.finding.path} · {draft.finding.side}:{draft.finding.line}</code></div><h4 className="mt-2 text-sm font-semibold text-fg">{draft.finding.title}</h4><p className="mt-2 text-xs text-fg"><strong>{t("prReview.failureScenario")}:</strong> {draft.finding.failure_scenario}</p><p className="mt-1 text-xs text-fg-muted">{draft.finding.explanation}</p><label className="mt-3 block text-[11px] font-medium text-fg-muted">{t("prReview.draftComment")}<textarea aria-label={t("prReview.draftComment")} rows={3} value={draft.comment} disabled={disabled} onChange={(event) => { const comment = event.currentTarget.value; setDrafts((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, comment } : item)); }} className="mt-1 w-full resize-y rounded-md border border-line bg-canvas px-2 py-1.5 text-xs text-fg outline-none focus:border-accent disabled:opacity-60" /></label></div></div>
    </article>)}</div>
  </section>;
}

function NoFindingsResult({ result, models }: { result: ReviewRunResultDto; models: ReviewModelOptionDto[] }) {
  const t = useT();
  const lang = useLang();
  const cost = estimatedRunCost(result.usage, result.model_id, models);
  return <section aria-labelledby="pr-review-clear" className="py-5 text-center">
    <div className="mx-auto grid h-10 w-10 place-items-center rounded-full bg-success/12 text-success">
      <CheckIcon width={20} height={20} />
    </div>
    <h3 id="pr-review-clear" className="mt-3 text-base font-semibold text-fg">{t("prReview.noFindings")}</h3>
    <p className="mt-1 text-xs text-fg-muted">{t("prReview.noFindingsDescription", { count: result.reviewed_files.length, sha: result.head_sha.slice(0, 7) })}</p>
    <p className="mt-1 text-[11px] text-fg-subtle">{t("prReview.savedLocally")}</p>
    <details className="mx-auto mt-5 max-w-2xl border-t border-line pt-3 text-left">
      <summary className="cursor-pointer select-none text-xs font-medium text-fg-muted hover:text-fg">{t("prReview.modelSummary")}</summary>
      <p className="mt-3 max-h-36 overflow-y-auto whitespace-pre-wrap text-xs leading-5 text-fg-muted">{result.summary}</p>
    </details>
    <p className="mt-4 text-[11px] text-fg-subtle">{t("prReview.usage", { input: result.usage.input_tokens, output: result.usage.output_tokens, tools: result.usage.tool_calls })}</p>
    {result.diagnostic_id && <p className="mt-1 font-mono text-[10px] text-fg-subtle">{t("prReview.diagnostics", { duration: result.duration_ms, attempts: result.provider_attempts, id: result.diagnostic_id })}</p>}
    {cost && <p className="mt-1 text-[11px] text-fg-subtle">{t("prReview.estimatedCost", { cost: formatEstimatedCost(cost, lang === "zh" ? "zh-CN" : "en-US") })}</p>}
  </section>;
}

function StatusLine({ text }: { text: string }) { return <div className="flex items-center gap-2 py-10 text-sm text-fg-muted"><SpinnerIcon width={16} height={16} />{text}</div>; }

function CancellationNotice({ error }: { error: AgentUiError }) {
  const t = useT();
  return <div role="status" className="mt-4 rounded-md border border-line-strong bg-elevated/60 p-3 text-xs text-fg-muted"><p>{t("prReview.cancelledNotice")}</p>{error.diagnostic_id && <p className="mt-1 font-mono text-[10px] text-fg-subtle">{t("prReview.errorDiagnostic", { id: error.diagnostic_id })}</p>}</div>;
}

function ErrorNotice({ error, onRetry, onConfigureCredential }: { error: AgentUiError; onRetry?: () => void; onConfigureCredential: (kind: CredentialKind) => void }) {
  const t = useT();
  const credential: CredentialKind | null = error.code === "AI_KEY_MISSING"
    ? "deepseek"
    : error.code === "OPENAI_KEY_MISSING"
      ? "openai"
      : error.code === "ANTHROPIC_KEY_MISSING"
        ? "anthropic"
        : error.code === "GITHUB_TOKEN_MISSING"
          ? "github"
          : error.code === "GITLAB_TOKEN_MISSING"
            ? "gitlab"
          : null;
  return <div role="alert" className="mt-4 rounded-md border border-danger/40 bg-danger/10 p-3 text-xs text-danger"><p>{errorMessage(error, t)}</p>{error.diagnostic_id && <p className="mt-1 font-mono text-[10px]">{t("prReview.errorDiagnostic", { id: error.diagnostic_id })}</p>}<div className="mt-2 flex gap-2">{credential && <button onClick={() => onConfigureCredential(credential)} className="rounded border border-danger/50 px-2 py-1">{t("prReview.openSettings")}</button>}{onRetry && <button onClick={onRetry} className="rounded border border-danger/50 px-2 py-1">{error.code === "PR_UPDATED" ? t("prReview.refreshPreflight") : t("prReview.retry")}</button>}</div></div>;
}

function progressLabel(progress: ReviewProgressEventDto | null, t: ReturnType<typeof useT>) {
  if (!progress) return t("prReview.stage.loading_pr");
  if (progress.stage === "tool_call" && progress.tool_name) return t("prReview.stage.toolDetail", { tool: progress.tool_name, count: progress.tool_calls ?? 1 });
  const key = `prReview.stage.${progress.stage}` as Parameters<typeof t>[0];
  return t(key);
}

function severityLabel(severity: string, t: ReturnType<typeof useT>) {
  const normalized = ["high", "medium", "low"].includes(severity) ? severity : "low";
  return t(`prReview.severity.${normalized}` as Parameters<typeof t>[0]);
}

function sortFindings(findings: ReviewFindingDto[]) {
  const order: Record<string, number> = { high: 0, medium: 1, low: 2 };
  return findings.map((finding, index) => ({ finding, index })).sort((a, b) => (order[a.finding.severity] ?? 3) - (order[b.finding.severity] ?? 3) || a.index - b.index).map(({ finding }) => finding);
}

function errorMessage(error: AgentUiError, t: ReturnType<typeof useT>) {
  const known = ["AI_KEY_MISSING", "OPENAI_KEY_MISSING", "ANTHROPIC_KEY_MISSING", "GITHUB_TOKEN_MISSING", "GITLAB_TOKEN_MISSING", "PR_UPDATED", "REVIEW_BUDGET_EXCEEDED", "NETWORK_ERROR", "RATE_LIMITED", "AUTH_FAILED", "INVALID_MODEL_OUTPUT", "INVALID_REVIEW_MODEL", "REVIEW_PUBLISH_FAILED", "AGENT_RESOURCE_BUSY", "CANCELLED"];
  return known.includes(error.code) ? t(`prReview.error.${error.code}` as Parameters<typeof t>[0]) : error.message;
}

function asIpcError(reason: unknown): AgentUiError {
  const candidate = reason as Partial<AgentUiError> | null;
  return { code: candidate?.code ?? "UNKNOWN", message: candidate?.message ?? String(reason), recoverable: candidate?.recoverable ?? true, diagnostic_id: typeof candidate?.diagnostic_id === "string" ? candidate.diagnostic_id : undefined };
}

function createRunId() {
  return globalThis.crypto?.randomUUID?.() ?? `review-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

type TokenEstimate = { lower: number; upper: number };

function estimateInitialInputTokens(patchBytes: number, fileCount: number): TokenEstimate | null {
  if (patchBytes <= 0 || fileCount <= 0) return null;
  return {
    lower: Math.ceil(patchBytes / 4) + 700 + fileCount * 20,
    upper: Math.ceil(patchBytes / 2) + 1_100 + fileCount * 40,
  };
}

function formatTokenRange(estimate: TokenEstimate, lang: "zh" | "en") {
  const formatter = new Intl.NumberFormat(lang === "zh" ? "zh-CN" : "en-US", {
    notation: "compact",
    maximumFractionDigits: 1,
  });
  return `${formatter.format(estimate.lower)}–${formatter.format(estimate.upper)}`;
}
