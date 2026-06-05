import { useEffect, useState } from "react";
import { getStatus, stageFile, unstageFile, commit, type StatusDto, type FileEntryDto, type IpcError } from "../ipc";

export function ChangesView({ repo }: { repo: string }) {
  const [status, setStatus] = useState<StatusDto | null>(null);
  const [message, setMessage] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function refresh() {
    setError(null);
    try { setStatus(await getStatus(repo)); }
    catch (e) { setError((e as IpcError).message ?? String(e)); }
  }
  useEffect(() => { refresh(); /* eslint-disable-next-line */ }, [repo]);

  async function run(action: () => Promise<void>) {
    setBusy(true); setError(null);
    try { await action(); await refresh(); }
    catch (e) { setError((e as IpcError).message ?? String(e)); }
    finally { setBusy(false); }
  }

  async function doCommit() {
    setBusy(true); setError(null);
    try {
      const sha = await commit(repo, message);
      setInfo(`已提交 ${sha.slice(0, 7)}`); setMessage(""); await refresh();
    } catch (e) { setError((e as IpcError).message ?? String(e)); }
    finally { setBusy(false); }
  }

  const staged = status?.entries.filter((e) => e.staged) ?? [];
  const unstaged = status?.entries.filter((e) => !e.staged) ?? [];

  const Row = ({ entry, isStaged }: { entry: FileEntryDto; isStaged: boolean }) => (
    <li className="flex items-center gap-2 px-3 py-1 hover:bg-[#161b22]">
      <span className="text-xs text-[#8b949e] w-20">{entry.state}</span>
      <span className="flex-1 font-mono text-sm text-[#c9d1d9] truncate">{entry.path}</span>
      <button className="text-xs text-[#58a6ff] disabled:opacity-40" disabled={busy}
        onClick={() => run(() => (isStaged ? unstageFile(repo, entry.path) : stageFile(repo, entry.path)))}>
        {isStaged ? "取消暂存" : "暂存"}
      </button>
    </li>
  );

  return (
    <div className="p-4 max-w-2xl">
      <button className="text-sm text-[#58a6ff]" disabled={busy} onClick={refresh}>刷新</button>
      {error && <p className="text-[#f85149] mt-2">错误:{error}</p>}
      {info && <p className="text-[#3fb950] mt-2">{info}</p>}

      <h3 className="text-[#e6edf3] font-semibold mt-4 mb-1">已暂存 ({staged.length})</h3>
      <ul>{staged.map((e) => <Row key={e.path} entry={e} isStaged />)}{staged.length === 0 && <li className="text-[#6e7681] px-3">(空)</li>}</ul>

      <h3 className="text-[#e6edf3] font-semibold mt-4 mb-1">未暂存 ({unstaged.length})</h3>
      <ul>{unstaged.map((e) => <Row key={e.path} entry={e} isStaged={false} />)}{unstaged.length === 0 && <li className="text-[#6e7681] px-3">(空)</li>}</ul>

      <h3 className="text-[#e6edf3] font-semibold mt-4 mb-1">提交</h3>
      <textarea className="w-full bg-[#0d1117] border border-[#21262d] rounded text-[#e6edf3] p-2 text-sm font-mono"
        rows={3} placeholder="提交信息" value={message} onChange={(e) => setMessage(e.target.value)} />
      <button className="mt-2 px-3 py-1.5 text-sm rounded bg-[#238636] text-white disabled:opacity-40"
        disabled={busy || staged.length === 0 || message.trim() === ""} onClick={doCommit}>
        提交 {staged.length} 个改动
      </button>
    </div>
  );
}
