// app/src/App.tsx
import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  getStatus,
  stageFile,
  unstageFile,
  commit,
  type StatusDto,
  type FileEntryDto,
  type IpcError,
} from "./ipc";

export default function App() {
  const [repo, setRepo] = useState<string | null>(null);
  const [status, setStatus] = useState<StatusDto | null>(null);
  const [message, setMessage] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function refreshStatus(repoPath: string) {
    setError(null);
    try {
      setStatus(await getStatus(repoPath));
    } catch (e) {
      setError((e as IpcError).message ?? String(e));
    }
  }

  async function pickRepo() {
    const dir = await open({ directory: true, title: "选择一个 git 仓库" });
    if (typeof dir !== "string") return;
    setRepo(dir);
    setInfo(null);
    await refreshStatus(dir);
  }

  async function run(action: () => Promise<void>) {
    if (!repo) return;
    setBusy(true);
    setError(null);
    try {
      await action();
      await refreshStatus(repo);
    } catch (e) {
      setError((e as IpcError).message ?? String(e));
    } finally {
      setBusy(false);
    }
  }

  const staged = status?.entries.filter((e) => e.staged) ?? [];
  const unstaged = status?.entries.filter((e) => !e.staged) ?? [];

  function Row({ entry, staged }: { entry: FileEntryDto; staged: boolean }) {
    return (
      <li style={{ display: "flex", alignItems: "center", gap: 8, padding: "2px 0" }}>
        <span style={{ fontSize: 12, color: "#888", width: 84 }}>{entry.state}</span>
        <span style={{ flex: 1, fontFamily: "monospace" }}>{entry.path}</span>
        <button
          disabled={busy}
          onClick={() =>
            run(() =>
              staged ? unstageFile(repo!, entry.path) : stageFile(repo!, entry.path)
            )
          }
        >
          {staged ? "取消暂存" : "暂存"}
        </button>
      </li>
    );
  }

  async function doCommit() {
    if (!repo) return;
    setBusy(true);
    setError(null);
    try {
      const sha = await commit(repo, message);
      setInfo(`已提交 ${sha.slice(0, 7)}`);
      setMessage("");
      await refreshStatus(repo);
    } catch (e) {
      setError((e as IpcError).message ?? String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main style={{ fontFamily: "system-ui", padding: 24, maxWidth: 720 }}>
      <h1>Git 客户端 · 阶段 1</h1>
      <button onClick={pickRepo} disabled={busy}>选择仓库</button>
      {repo && <span style={{ marginLeft: 12, color: "#666" }}>{repo}</span>}

      {error && <p style={{ color: "crimson" }}>错误:{error}</p>}
      {info && <p style={{ color: "green" }}>{info}</p>}

      {repo && (
        <>
          <button onClick={() => refreshStatus(repo)} disabled={busy} style={{ marginTop: 12 }}>
            刷新
          </button>

          <h3 style={{ marginBottom: 4 }}>已暂存 ({staged.length})</h3>
          <ul style={{ listStyle: "none", padding: 0 }}>
            {staged.map((e) => <Row key={e.path} entry={e} staged />)}
            {staged.length === 0 && <li style={{ color: "#aaa" }}>(空)</li>}
          </ul>

          <h3 style={{ marginBottom: 4 }}>未暂存 ({unstaged.length})</h3>
          <ul style={{ listStyle: "none", padding: 0 }}>
            {unstaged.map((e) => <Row key={e.path} entry={e} staged={false} />)}
            {unstaged.length === 0 && <li style={{ color: "#aaa" }}>(空)</li>}
          </ul>

          <h3 style={{ marginBottom: 4 }}>提交</h3>
          <textarea
            value={message}
            onChange={(e) => setMessage(e.target.value)}
            placeholder="提交信息"
            rows={3}
            style={{ width: "100%", fontFamily: "inherit" }}
          />
          <button
            onClick={doCommit}
            disabled={busy || staged.length === 0 || message.trim() === ""}
            style={{ marginTop: 8 }}
          >
            提交 {staged.length} 个改动
          </button>
        </>
      )}
    </main>
  );
}
