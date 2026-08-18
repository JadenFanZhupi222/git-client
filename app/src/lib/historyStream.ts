import type { AgentStreamState } from "./agentStream";

export type HistoryStreamFinding = {
  title: string;
  explanation: string;
};

export type HistoryStreamDraft = {
  summary: string;
  findings: HistoryStreamFinding[];
};

const HISTORY_ARTIFACT_TYPE = "history_investigation";

/** Builds the visible draft exclusively from backend-approved semantic events. */
export function historyDraftFromStream(stream: AgentStreamState | null): HistoryStreamDraft | null {
  const attempts = stream?.attempts;
  const attempt = attempts?.[attempts.length - 1];
  if (!attempt) return null;

  let summary = "";
  const findings: HistoryStreamFinding[] = [];
  for (const part of attempt.artifactText) {
    if (part.artifactType !== HISTORY_ARTIFACT_TYPE) continue;
    if (part.field === "summary") {
      summary = part.text;
      continue;
    }
    if (part.itemIndex === null || part.itemIndex < 0) continue;
    const finding = findings[part.itemIndex] ?? { title: "", explanation: "" };
    findings[part.itemIndex] = finding;
    if (part.field === "finding_title") finding.title = part.text;
    if (part.field === "finding_explanation") finding.explanation = part.text;
  }

  if (!summary && findings.every((finding) => !finding?.title && !finding?.explanation)) return null;
  return { summary, findings: findings.filter(Boolean) };
}
