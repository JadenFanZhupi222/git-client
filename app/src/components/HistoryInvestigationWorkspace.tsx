import { useEffect, useRef, useState } from "react";
import type {
  AgentIpcErrorDto,
  CredentialKindDto,
  HistoryInvestigationResultDto,
  ReviewModelOptionDto,
} from "../bindings";
import {
  cancelHistoryInvestigation,
  investigateRepositoryHistory,
  listReviewModels,
} from "../ipc";
import { useT } from "../lib/i18n";
import { historyDraftFromStream, type HistoryStreamDraft } from "../lib/historyStream";
import { useAgentStream } from "../hooks/useAgentStream";
import { AgentModelPicker } from "./AgentModelPicker";
import { AgentStreamPanel } from "./AgentStreamPanel";
import { AlertIcon, CloseIcon, HistoryIcon, SpinnerIcon } from "./icons";
import { Button } from "./ui/Button";

const MODEL_CONSENT_KEY = "history-investigation-model-consent-v1";

export function HistoryInvestigationWorkspace({
  repo,
  selectedFile,
  onClose,
  onOpenEvidence,
  onConfigureCredential = () => undefined,
}: {
  repo: string;
  selectedFile: string | null;
  onClose: () => void;
  onOpenEvidence: (commitId: string, path?: string) => void;
  onConfigureCredential?: (kind: CredentialKindDto) => void;
}) {
  const t = useT();
  const mountedRef = useRef(true);
  const runIdRef = useRef<string | null>(null);
  const [question, setQuestion] = useState("");
  const [useFileScope, setUseFileScope] = useState(!!selectedFile);
  const [models, setModels] = useState<ReviewModelOptionDto[]>([]);
  const [selectedModelId, setSelectedModelId] = useState("");
  const [consented, setConsented] = useState(
    () => localStorage.getItem(MODEL_CONSENT_KEY) === "accepted",
  );
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<HistoryInvestigationResultDto | null>(null);
  const [error, setError] = useState<AgentIpcErrorDto | null>(null);
  const agentStream = useAgentStream();
  const streamingDraft = busy ? historyDraftFromStream(agentStream.stream) : null;

  useEffect(() => {
    mountedRef.current = true;
    void listReviewModels()
      .then((next) => {
        if (!mountedRef.current) return;
        const compatible = next.filter((model) => model.capabilities.supports_structured_output);
        setModels(compatible);
        setSelectedModelId(compatible[0]?.id ?? "");
      })
      .catch((reason) => {
        if (mountedRef.current) setError(asAgentError(reason));
      });
    return () => {
      mountedRef.current = false;
      const runId = runIdRef.current;
      runIdRef.current = null;
      if (runId) void cancelHistoryInvestigation(runId).catch(() => undefined);
    };
  }, [repo]);

  useEffect(() => {
    setUseFileScope(!!selectedFile);
  }, [selectedFile]);

  function acceptConsent(checked: boolean) {
    setConsented(checked);
    if (checked) localStorage.setItem(MODEL_CONSENT_KEY, "accepted");
    else localStorage.removeItem(MODEL_CONSENT_KEY);
  }

  async function runInvestigation() {
    const trimmed = question.trim();
    if (trimmed.length < 5 || !selectedModelId || !consented) return;
    const previous = runIdRef.current;
    if (previous) void cancelHistoryInvestigation(previous).catch(() => undefined);
    const runId = createRunId();
    runIdRef.current = runId;
    setBusy(true);
    setResult(null);
    setError(null);
    try {
      await agentStream.begin(runId);
      const next = await investigateRepositoryHistory({
        run_id: runId,
        repo_path: repo,
        question: trimmed,
        file: useFileScope ? selectedFile : null,
        model_id: selectedModelId,
      });
      if (!mountedRef.current || runIdRef.current !== runId) return;
      setResult(next);
    } catch (reason) {
      if (!mountedRef.current || runIdRef.current !== runId) return;
      setError(asAgentError(reason));
    } finally {
      agentStream.end();
      if (runIdRef.current === runId) runIdRef.current = null;
      if (mountedRef.current) setBusy(false);
    }
  }

  function cancel() {
    const runId = runIdRef.current;
    if (!runId) return;
    void cancelHistoryInvestigation(runId).catch(() => undefined);
  }

  return (
    <section className="flex min-h-0 min-w-0 flex-1 flex-col" aria-label={t("historyInvestigator.title")}>
      <header className="flex h-[41px] shrink-0 items-center gap-2 border-b border-line px-3">
        <HistoryIcon width={14} height={14} className="text-accent" />
        <div className="min-w-0 flex-1">
          <div className="text-[12px] font-semibold text-fg">{t("historyInvestigator.title")}</div>
          <div className="text-[10.5px] text-fg-subtle">{t("historyInvestigator.readOnly")}</div>
        </div>
        <button
          type="button"
          onClick={onClose}
          aria-label={t("historyInvestigator.close")}
          className="grid h-7 w-7 place-items-center rounded-md text-fg-muted hover:bg-overlay hover:text-fg"
        >
          <CloseIcon width={13} height={13} />
        </button>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto w-full max-w-4xl px-5 py-5">
          <div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_280px]">
            <div>
              <label htmlFor="history-investigation-question" className="text-[11px] font-medium text-fg-muted">
                {t("historyInvestigator.question")}
              </label>
              <textarea
                id="history-investigation-question"
                value={question}
                maxLength={1000}
                rows={5}
                disabled={busy}
                onChange={(event) => setQuestion(event.currentTarget.value)}
                placeholder={t("historyInvestigator.placeholder")}
                className="field mt-1.5 w-full resize-y rounded-md border border-line bg-canvas px-3 py-2 text-[12px] leading-relaxed text-fg placeholder:text-fg-subtle disabled:opacity-60"
              />
              <div className="mt-2 flex items-center justify-between gap-3 text-[10.5px] text-fg-subtle">
                <span>{t("historyInvestigator.evidenceLimit")}</span>
                <span className="font-mono">{question.length}/1000</span>
              </div>
              {selectedFile && (
                <label className="mt-3 flex items-start gap-2 border-y border-line py-2.5 text-[11px] leading-relaxed text-fg-muted">
                  <input
                    type="checkbox"
                    checked={useFileScope}
                    disabled={busy}
                    onChange={(event) => setUseFileScope(event.currentTarget.checked)}
                    className="mt-0.5 accent-accent"
                  />
                  <span className="min-w-0">
                    {t("historyInvestigator.scopeFile")}
                    <span className="ml-1 break-all font-mono text-fg">{selectedFile}</span>
                  </span>
                </label>
              )}
            </div>

            <div>
              <AgentModelPicker
                id="history-investigation-model"
                label={t("historyInvestigator.model")}
                models={models}
                value={selectedModelId}
                onChange={setSelectedModelId}
                onConfigureCredential={onConfigureCredential}
              />
              <label className="mt-3 flex items-start gap-2 text-[11px] leading-relaxed text-fg-muted">
                <input
                  type="checkbox"
                  checked={consented}
                  disabled={busy}
                  onChange={(event) => acceptConsent(event.currentTarget.checked)}
                  className="mt-0.5 accent-accent"
                />
                {t("historyInvestigator.consent")}
              </label>
              <div className="mt-3 flex gap-2">
                <Button
                  type="button"
                  size="sm"
                  disabled={busy || question.trim().length < 5 || !selectedModelId || !consented}
                  onClick={() => void runInvestigation()}
                  className="flex-1"
                >
                  {busy ? <SpinnerIcon width={13} height={13} /> : <HistoryIcon width={13} height={13} />}
                  {busy ? t("historyInvestigator.investigating") : t("historyInvestigator.run")}
                </Button>
                {busy && (
                  <Button type="button" variant="secondary" size="sm" onClick={cancel}>
                    {t("historyInvestigator.cancel")}
                  </Button>
                )}
              </div>
            </div>
          </div>

          {agentStream.stream && (
            <div className="mt-5">
              <AgentStreamPanel
                stream={agentStream.stream}
                active={busy}
                preparingLabel={t("historyInvestigator.activity.preparingEvidence")}
              />
            </div>
          )}

          {error && (
            <div className="mt-5 flex items-start gap-2 border-y border-danger/30 bg-danger/[0.06] px-3 py-2.5 text-xs text-danger" role="alert">
              <AlertIcon width={14} height={14} className="mt-0.5 shrink-0" />
              <div className="min-w-0 flex-1">
                <div>{error.message}</div>
                <div className="mt-1 font-mono text-[10px] opacity-70">{error.code} · {error.diagnostic_id}</div>
              </div>
            </div>
          )}

          {streamingDraft && !result && (
            <StreamingHistoryAnswer draft={streamingDraft} />
          )}

          {result && (
            <div className="mt-7">
              <div className="flex flex-wrap items-center gap-2 border-b border-line pb-3">
                <h2 className="min-w-0 flex-1 text-[15px] font-semibold leading-relaxed text-fg">{result.summary}</h2>
                <span className={`rounded-full px-2 py-0.5 text-[10.5px] font-medium ${confidenceClass(result.confidence)}`}>
                  {t(`historyInvestigator.confidence.${normalizeConfidence(result.confidence)}`)}
                </span>
              </div>
              <div className="flex flex-wrap items-center gap-1.5 border-b border-line py-2.5 text-[10.5px] text-fg-subtle">
                <span>{t("historyInvestigator.retrieval", { n: result.evidence_commit_count })}</span>
                {result.evidence_sources.map((source) => (
                  <span key={source} className="rounded bg-overlay px-1.5 py-0.5 text-fg-muted">
                    {sourceLabel(source, t)}
                  </span>
                ))}
                {result.search_terms.map((term) => (
                  <span key={term} className="rounded bg-accent/10 px-1.5 py-0.5 font-mono text-accent">S:{term}</span>
                ))}
              </div>
              <div className="divide-y divide-line">
                {result.findings.map((finding, index) => (
                  <article key={`${finding.title}-${index}`} className="py-4">
                    <h3 className="text-[13px] font-semibold text-fg">{finding.title}</h3>
                    <p className="mt-1.5 text-[11.5px] leading-relaxed text-fg-muted">{finding.explanation}</p>
                    <div className="mt-2 flex flex-wrap gap-1.5">
                      {finding.commit_ids.map((commit) => (
                        <button
                          type="button"
                          key={commit}
                          onClick={() => onOpenEvidence(commit)}
                          title={t("historyInvestigator.openCommit", { commit })}
                          className="rounded bg-accent/10 px-1.5 py-0.5 font-mono text-[10px] text-accent hover:bg-accent/20"
                        >
                          {commit}
                        </button>
                      ))}
                      {finding.paths.map((path) => {
                        const link = finding.evidence_links.find((candidate) => candidate.path === path);
                        return link ? (
                          <button
                            type="button"
                            key={path}
                            onClick={() => onOpenEvidence(link.commit_id, link.path)}
                            title={t("historyInvestigator.openDiff", { commit: link.commit_id, path })}
                            className="max-w-full truncate rounded bg-overlay px-1.5 py-0.5 font-mono text-[10px] text-fg-muted hover:bg-elevated hover:text-fg"
                          >
                            {path}
                          </button>
                        ) : (
                          <span key={path} className="max-w-full truncate rounded bg-overlay px-1.5 py-0.5 font-mono text-[10px] text-fg-muted" title={path}>{path}</span>
                        );
                      })}
                    </div>
                  </article>
                ))}
              </div>
              {result.caveats.length > 0 && (
                <section className="border-y border-warning/25 bg-warning/[0.04] px-3 py-3">
                  <h3 className="text-[11px] font-semibold uppercase tracking-wide text-warning">{t("historyInvestigator.caveats")}</h3>
                  <ul className="mt-2 list-disc space-y-1 pl-4 text-[11px] leading-relaxed text-fg-muted">
                    {result.caveats.map((caveat, index) => <li key={`${caveat}-${index}`}>{caveat}</li>)}
                  </ul>
                </section>
              )}
              <p className="mt-3 font-mono text-[10px] text-fg-subtle">
                {t("historyInvestigator.usage", {
                  commits: result.findings.reduce((all, finding) => new Set([...all, ...finding.commit_ids]), new Set<string>()).size,
                  input: result.usage.input_tokens,
                  output: result.usage.output_tokens,
                })}
              </p>
            </div>
          )}
        </div>
      </div>
    </section>
  );
}

function StreamingHistoryAnswer({ draft }: { draft: HistoryStreamDraft }) {
  const t = useT();
  return (
    <section className="mt-7" aria-label={t("historyInvestigator.streaming.label")} aria-live="polite">
      <div className="flex flex-wrap items-center gap-2 border-b border-line pb-3">
        {draft.summary && (
          <h2 className="min-w-0 flex-1 text-[15px] font-semibold leading-relaxed text-fg">
            {draft.summary}
            {draft.findings.length === 0 && <StreamingCaret />}
          </h2>
        )}
        <span className="rounded-full bg-accent/10 px-2 py-0.5 text-[10.5px] font-medium text-accent">
          {t("historyInvestigator.streaming.badge")}
        </span>
      </div>
      {draft.findings.length > 0 && (
        <div className="divide-y divide-line">
          {draft.findings.map((finding, index) => (
            <article key={index} className="py-4">
              {finding.title && (
                <h3 className="text-[13px] font-semibold text-fg">
                  {finding.title}
                  {index === draft.findings.length - 1 && !finding.explanation && <StreamingCaret />}
                </h3>
              )}
              {finding.explanation && (
                <p className="mt-1.5 text-[11.5px] leading-relaxed text-fg-muted">
                  {finding.explanation}
                  {index === draft.findings.length - 1 && <StreamingCaret />}
                </p>
              )}
            </article>
          ))}
        </div>
      )}
      <p className="border-t border-line pt-2.5 text-[10.5px] text-fg-subtle">
        {t("historyInvestigator.streaming.validationNotice")}
      </p>
    </section>
  );
}

function StreamingCaret() {
  return <span className="ml-0.5 inline-block h-[1em] w-px translate-y-[2px] bg-accent" aria-hidden="true" />;
}

type Translate = ReturnType<typeof useT>;

function sourceLabel(source: string, t: Translate): string {
  const key = {
    recent_history: "historyInvestigator.source.recentHistory",
    file_history: "historyInvestigator.source.fileHistory",
    pickaxe: "historyInvestigator.source.pickaxe",
    blame: "historyInvestigator.source.blame",
    commit_diffs: "historyInvestigator.source.commitDiffs",
  }[source] as
    | "historyInvestigator.source.recentHistory"
    | "historyInvestigator.source.fileHistory"
    | "historyInvestigator.source.pickaxe"
    | "historyInvestigator.source.blame"
    | "historyInvestigator.source.commitDiffs"
    | undefined;
  return key ? t(key) : source;
}

function normalizeConfidence(value: string): "high" | "medium" | "low" {
  return value === "high" || value === "medium" ? value : "low";
}

function confidenceClass(value: string): string {
  if (value === "high") return "bg-success/15 text-success";
  if (value === "medium") return "bg-warning/15 text-warning";
  return "bg-overlay text-fg-muted";
}

function createRunId(): string {
  return typeof crypto.randomUUID === "function"
    ? crypto.randomUUID()
    : `history-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function asAgentError(reason: unknown): AgentIpcErrorDto {
  if (reason && typeof reason === "object" && "code" in reason) {
    const error = reason as Partial<AgentIpcErrorDto>;
    return {
      code: error.code ?? "HISTORY_INVESTIGATION_FAILED",
      message: error.message ?? "History investigation failed",
      recoverable: error.recoverable ?? true,
      diagnostic_id: error.diagnostic_id ?? "history-investigation",
    };
  }
  return {
    code: "HISTORY_INVESTIGATION_FAILED",
    message: reason instanceof Error ? reason.message : String(reason),
    recoverable: true,
    diagnostic_id: "history-investigation",
  };
}
