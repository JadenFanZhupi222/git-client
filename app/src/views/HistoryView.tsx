import { useEffect, useState } from "react";
import { getLog, getCommitFiles, getCurrentBranch, type CommitDto, type FileChangeDto, type IpcError } from "../ipc";
import { CommitList } from "../components/CommitList";
import { CommitFileList } from "../components/CommitFileList";

const PAGE = 50;

export function HistoryView({ repo }: { repo: string }) {
  const [commits, setCommits] = useState<CommitDto[]>([]);
  const [branch, setBranch] = useState<string | null>(null);
  const [selected, setSelected] = useState<CommitDto | null>(null);
  const [files, setFiles] = useState<FileChangeDto[]>([]);
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [hasMore, setHasMore] = useState(true);
  const [error, setError] = useState<string | null>(null);

  async function loadPage(skip: number) {
    setLoading(true); setError(null);
    try {
      const page = await getLog(repo, PAGE, skip);
      setCommits((prev) => (skip === 0 ? page : [...prev, ...page]));
      setHasMore(page.length === PAGE); // 不足一页 = 历史到底
    } catch (e) { setError((e as IpcError).message ?? String(e)); }
    finally { setLoading(false); }
  }

  useEffect(() => {
    setCommits([]); setSelected(null); setFiles([]); setSelectedFile(null);
    setHasMore(true); setError(null);
    loadPage(0);
    getCurrentBranch(repo).then(setBranch).catch(() => setBranch(null));
    // eslint-disable-next-line
  }, [repo]);

  async function selectCommit(c: CommitDto) {
    setSelected(c); setSelectedFile(null); setFiles([]);
    try { setFiles(await getCommitFiles(repo, c.id)); }
    catch (e) { setError((e as IpcError).message ?? String(e)); }
  }

  return (
    <div className="flex h-[calc(100vh-6.5rem)]">
      <aside className="w-72 border-r border-[#21262d] overflow-hidden flex flex-col">
        {error && <p className="text-[#f85149] text-xs p-2">{error}</p>}
        <CommitList commits={commits} branch={branch} selectedId={selected?.id ?? null}
          onSelect={selectCommit} onLoadMore={() => loadPage(commits.length)} loading={loading} hasMore={hasMore} />
      </aside>
      <div className="w-64 border-r border-[#21262d] overflow-hidden flex flex-col">
        <div className="px-3 py-2 text-[11px] uppercase tracking-wide text-[#8b949e] border-b border-[#21262d]">改动文件</div>
        {selected
          ? <CommitFileList files={files} selected={selectedFile} onSelect={setSelectedFile} />
          : <div className="p-3 text-xs text-[#6e7681]">选择一个提交</div>}
      </div>
      <main className="flex-1 overflow-auto p-4">
        <div className="text-[11px] uppercase tracking-wide text-[#8b949e] mb-2">Diff</div>
        <div className="text-sm text-[#6e7681]">
          {selectedFile ? `${selectedFile} 的行级 diff 将在 1b-2 显示` : "选择一个文件查看 diff(1b-2)"}
        </div>
      </main>
    </div>
  );
}
