import type { AgentStreamState } from "../lib/agentStream";
import { useT } from "../lib/i18n";

type AgentStreamPanelProps = {
  stream: AgentStreamState | null;
};

const statusKey = {
  starting: "agentStream.status.starting",
  streaming: "agentStream.status.streaming",
  completed: "agentStream.status.completed",
  retrying: "agentStream.status.retrying",
  failed: "agentStream.status.failed",
} as const;

export function AgentStreamPanel({ stream }: AgentStreamPanelProps) {
  const t = useT();
  if (!stream) return null;

  return (
    <section className="w-full max-w-3xl border-y border-line bg-elevated/50 text-left" aria-label={t("agentStream.title")}>
      <header className="flex items-center justify-between gap-3 px-3 py-2">
        <h3 className="text-[11px] font-semibold uppercase tracking-[0.14em] text-fg-muted">{t("agentStream.title")}</h3>
        <span className="font-mono text-[10px] text-fg-subtle">{stream.attempts.length ? t("agentStream.attempts", { count: stream.attempts.length }) : t("agentStream.waiting")}</span>
      </header>
      <div className="max-h-64 overflow-auto border-t border-line">
        {stream.attempts.map((attempt) => (
          <article key={attempt.attemptId} className="border-b border-line px-3 py-2 last:border-b-0">
            <div className="flex items-center gap-2 text-[11px]">
              <span className={`size-1.5 rounded-full ${attempt.status === "failed" ? "bg-danger" : attempt.status === "completed" ? "bg-success" : "bg-accent"}`} aria-hidden="true" />
              <span className="font-medium text-fg">{t("agentStream.attempt", { number: attempt.attemptId })}</span>
              <span className="text-fg-subtle">{attempt.modelId ?? t("agentStream.modelPending")}</span>
              <span className="ml-auto text-fg-muted">{t(statusKey[attempt.status])}</span>
            </div>
            {attempt.text && (
              <pre className="mt-2 max-h-40 overflow-auto whitespace-pre-wrap break-words font-mono text-[11px] leading-5 text-fg" aria-live="polite">{attempt.text}</pre>
            )}
            {attempt.tools.length > 0 && (
              <ul className="mt-2 space-y-1">
                {attempt.tools.map((tool) => (
                  <li key={tool.callId} className="flex min-w-0 gap-2 font-mono text-[10px] text-fg-muted">
                    <span className="shrink-0 text-accent">{tool.name}</span>
                    <span className="truncate">{tool.arguments || t("agentStream.toolPending")}</span>
                  </li>
                ))}
              </ul>
            )}
            {attempt.usage && (
              <p className="mt-2 font-mono text-[10px] text-fg-subtle">{t("agentStream.tokens", { input: attempt.usage.input_tokens, output: attempt.usage.output_tokens })}</p>
            )}
          </article>
        ))}
        {stream.attempts.length === 0 && <p className="px-3 py-5 text-center text-xs text-fg-muted" aria-live="polite">{t("agentStream.connecting")}</p>}
      </div>
    </section>
  );
}
