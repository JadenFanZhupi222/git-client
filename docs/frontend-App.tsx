// app/src/App.tsx
import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getHeadCommit, type CommitDto, type IpcError } from "./ipc";

export default function App() {
  const [commit, setCommit] = useState<CommitDto | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  async function pickRepoAndLoad() {
    setError(null);
    const dir = await open({ directory: true, title: "选择一个 git 仓库" });
    if (typeof dir !== "string") return;

    setLoading(true);
    try {
      const c = await getHeadCommit(dir);
      setCommit(c);
    } catch (e) {
      // 后端返回的是结构化 IpcError
      const err = e as IpcError;
      setError(err.message ?? String(e));
      setCommit(null);
    } finally {
      setLoading(false);
    }
  }

  return (
    <main style={{ fontFamily: "system-ui", padding: 32, maxWidth: 640 }}>
      <h1>Git 客户端 · 阶段 0</h1>
      <p style={{ color: "#666" }}>
        点下面的按钮选一个本地 git 仓库,读取它的 HEAD 提交。
      </p>
      <button onClick={pickRepoAndLoad} disabled={loading}>
        {loading ? "读取中…" : "选择仓库并读取 HEAD"}
      </button>

      {error && (
        <p style={{ color: "crimson", marginTop: 16 }}>错误:{error}</p>
      )}

      {commit && (
        <div style={{ marginTop: 24, padding: 16, border: "1px solid #ddd", borderRadius: 8 }}>
          <div><strong>SHA:</strong> {commit.short_id}</div>
          <div><strong>信息:</strong> {commit.summary}</div>
          <div><strong>作者:</strong> {commit.author_name}</div>
          <div><strong>时间:</strong> {new Date(commit.timestamp * 1000).toLocaleString()}</div>
        </div>
      )}
    </main>
  );
}
