export type HistorySelection = {
  commitId: string;
  file: string | null;
};

const PREFIX = "history.selection.v1:";

function key(repo: string): string {
  return `${PREFIX}${repo}`;
}

export function readHistorySelection(repo: string): HistorySelection | null {
  const storageKey = key(repo);
  try {
    const raw = localStorage.getItem(storageKey);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<HistorySelection>;
    if (typeof parsed.commitId !== "string" || parsed.commitId.length === 0) {
      localStorage.removeItem(storageKey);
      return null;
    }
    return {
      commitId: parsed.commitId,
      file: typeof parsed.file === "string" && parsed.file.length > 0 ? parsed.file : null,
    };
  } catch {
    localStorage.removeItem(storageKey);
    return null;
  }
}

export function writeHistorySelection(repo: string, selection: HistorySelection): void {
  try {
    localStorage.setItem(key(repo), JSON.stringify(selection));
  } catch {
    // Persistence is best-effort; History remains fully usable without storage.
  }
}
