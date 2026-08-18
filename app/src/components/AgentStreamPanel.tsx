import type { AgentAttemptStatus, AgentRunStatus, AgentStreamState } from "../lib/agentStream";
import { useT } from "../lib/i18n";
import { resolveToolApproval } from "../ipc";

type AgentStreamPanelProps = {
  stream: AgentStreamState | null;
  preparingLabel?: string;
  onApprovalDecision?: (runId: string, approvalId: string, decision: "allow" | "deny") => void | Promise<void>;
};

const statusKey = {
  starting: "agentStream.status.starting",
  streaming: "agentStream.status.streaming",
  completed: "agentStream.status.completed",
  retrying: "agentStream.status.retrying",
  failed: "agentStream.status.failed",
} as const;

export function AgentStreamPanel({ stream, preparingLabel, onApprovalDecision }: AgentStreamPanelProps) {
  const t = useT();
  if (!stream) return null;
  const decideApproval = onApprovalDecision
    ?? ((runId: string, approvalId: string, decision: "allow" | "deny") => (
      resolveToolApproval(runId, approvalId, decision)
    ));
  const latestStatus = stream.attempts[stream.attempts.length - 1]?.status;

  return (
    <section className="w-full max-w-3xl border-y border-line text-left" aria-label={t("agentStream.title")}>
      <header className="flex items-center justify-between gap-3 py-2.5">
        <div className="flex min-w-0 items-center gap-2">
          <span className={`size-1.5 shrink-0 rounded-full ${headerDotClass(stream.runStatus, latestStatus)}`} aria-hidden="true" />
          <h3 className="truncate text-[11.5px] font-semibold text-fg">{t("agentStream.title")}</h3>
        </div>
        <span className="font-mono text-[10px] text-fg-subtle">
          {stream.attempts.length ? t("agentStream.attempts", { count: stream.attempts.length }) : t("agentStream.waiting")}
        </span>
      </header>
      <div className="max-h-72 overflow-auto border-t border-line py-1">
        {stream.attempts.map((attempt, index) => {
          const isLatest = index === stream.attempts.length - 1;
          const displayedStatus = attemptDisplayStatus(attempt.status, isLatest, stream.runStatus);
          return (
          <article key={attempt.attemptId} className="relative grid grid-cols-[14px_minmax(0,1fr)] gap-2.5 py-2.5">
            {index < stream.attempts.length - 1 && <span className="absolute bottom-0 left-[6px] top-[17px] w-px bg-line" aria-hidden="true" />}
            <span className={`relative z-[1] mt-1 size-3 rounded-full border-2 border-canvas ${statusDotClass(displayedStatus)}`} aria-hidden="true" />
            <div className="min-w-0">
              <div className="flex min-w-0 items-baseline gap-2 text-[11px]">
                <span className="truncate font-medium text-fg" aria-live="polite">
                  {t(activityKey(attempt.status, attempt.tools.length > 0, attempt.text.length > 0, isLatest, stream.runStatus))}
                </span>
                <span className="ml-auto shrink-0 text-[10px] text-fg-subtle">{t(displayStatusKey(displayedStatus))}</span>
              </div>
              <p className="mt-0.5 truncate font-mono text-[10px] text-fg-subtle">
                {t("agentStream.attemptModel", { attempt: attempt.attemptId, model: attempt.modelId ?? t("agentStream.modelPending") })}
              </p>
              {attempt.tools.length > 0 && (
                <ul className="mt-2 space-y-1.5">
                  {attempt.tools.map((tool) => (
                    <li key={tool.callId} className="text-[10.5px] text-fg-muted">
                      <div className="flex min-w-0 items-center gap-2">
                        <span className="size-1 shrink-0 rounded-full bg-accent" aria-hidden="true" />
                        <span className="truncate">{t("agentStream.toolCall", { name: tool.name })}</span>
                      </div>
                      {tool.permission === "pending" && tool.approvalId && (
                        <div className="ml-3 mt-1.5 border-l border-warning/50 pl-2">
                          <p className="text-fg-muted">{tool.approvalSummary ?? t("agentStream.approvalRequired")}</p>
                          <div className="mt-1.5 flex gap-1.5">
                            <button
                              type="button"
                              className="rounded border border-line px-2 py-0.5 text-[10px] text-fg hover:border-accent"
                              onClick={() => void decideApproval(stream.runId, tool.approvalId!, "allow")}
                            >
                              {t("agentStream.approve")}
                            </button>
                            <button
                              type="button"
                              className="rounded border border-line px-2 py-0.5 text-[10px] text-fg-muted hover:border-danger"
                              onClick={() => void decideApproval(stream.runId, tool.approvalId!, "deny")}
                            >
                              {t("agentStream.deny")}
                            </button>
                          </div>
                        </div>
                      )}
                    </li>
                  ))}
                </ul>
              )}
              {attempt.usage && (
                <p className="mt-2 font-mono text-[10px] text-fg-subtle">
                  {t("agentStream.tokens", { input: attempt.usage.input_tokens, output: attempt.usage.output_tokens })}
                </p>
              )}
              {(attempt.text || attempt.tools.some((tool) => tool.arguments)) && (
                <details className="mt-2 border-t border-line/70 pt-1.5">
                  <summary className="w-fit cursor-pointer select-none text-[10px] text-fg-subtle hover:text-fg-muted">
                    {t("agentStream.debugDetails")}
                  </summary>
                  <div className="mt-2 max-h-40 overflow-auto bg-overlay/45 px-2.5 py-2 font-mono text-[10px] leading-[1.55] text-fg-muted">
                    {attempt.text && <pre className="whitespace-pre-wrap break-words">{attempt.text}</pre>}
                    {attempt.tools.map((tool) => tool.arguments && (
                      <div key={tool.callId} className="mt-2 first:mt-0">
                        <div className="text-accent">{tool.name}</div>
                        <pre className="whitespace-pre-wrap break-words">{tool.arguments}</pre>
                      </div>
                    ))}
                  </div>
                </details>
              )}
            </div>
          </article>
          );
        })}
        {stream.attempts.length === 0 && (
          <div className="flex items-center gap-2 py-3 text-[11px] text-fg-muted" aria-live="polite">
            <span className={`size-2 rounded-full ${statusDotClass(stream.runStatus)}`} aria-hidden="true" />
            <span>
              {stream.runStatus === "active"
                ? (preparingLabel ?? t("agentStream.connecting"))
                : t(emptyActivityKey(stream.runStatus))}
            </span>
          </div>
        )}
      </div>
    </section>
  );
}

function activityKey(
  status: AgentAttemptStatus,
  hasTools: boolean,
  hasText: boolean,
  isLatest: boolean,
  runStatus: AgentRunStatus,
) {
  if (isLatest && runStatus === "cancelled") return "agentStream.activity.cancelled" as const;
  if (isLatest && runStatus === "failed") return "agentStream.activity.runFailed" as const;
  if (isLatest && runStatus === "completed") return "agentStream.activity.validated" as const;
  if (status === "failed") return "agentStream.activity.failed" as const;
  if (status === "retrying") return "agentStream.activity.retrying" as const;
  if (status === "completed") return "agentStream.activity.validating" as const;
  if (hasTools) return "agentStream.activity.usingTools" as const;
  if (status === "streaming" && hasText) return "agentStream.activity.generating" as const;
  if (status === "streaming") return "agentStream.activity.waitingResponse" as const;
  return "agentStream.activity.preparingModel" as const;
}

function attemptDisplayStatus(
  status: AgentAttemptStatus,
  isLatest: boolean,
  runStatus: AgentRunStatus,
): AgentAttemptStatus | AgentRunStatus {
  return isLatest && runStatus !== "active" ? runStatus : status;
}

function displayStatusKey(status: AgentAttemptStatus | AgentRunStatus) {
  if (status === "cancelled") return "agentStream.status.cancelled" as const;
  if (status === "active") return "agentStream.status.streaming" as const;
  return statusKey[status];
}

function emptyActivityKey(runStatus: AgentRunStatus) {
  if (runStatus === "cancelled") return "agentStream.activity.cancelled" as const;
  if (runStatus === "failed") return "agentStream.activity.runFailed" as const;
  if (runStatus === "completed") return "agentStream.activity.validated" as const;
  return "agentStream.connecting" as const;
}

function statusDotClass(status: AgentAttemptStatus | AgentRunStatus): string {
  if (status === "failed") return "bg-danger";
  if (status === "completed") return "bg-success";
  if (status === "retrying") return "bg-warning";
  if (status === "cancelled") return "bg-fg-subtle";
  return "bg-accent";
}

function headerDotClass(runStatus: AgentRunStatus, status?: AgentAttemptStatus): string {
  if (runStatus !== "active") return statusDotClass(runStatus);
  return status ? statusDotClass(status) : "bg-accent";
}
