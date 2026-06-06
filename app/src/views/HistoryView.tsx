import { useEffect, useState } from "react";
import {
  getLog, getCommitFiles, getCurrentBranch, getCommitFileDiff, onRepoChanged,
  type CommitDto, type FileChangeDto, type FileDiffDto, type IpcError,
} from "../ipc";
import { CommitList } from "../components/CommitList";
import { CommitFileList } from "../components/CommitFileList";
import { DiffView } from "../components/DiffView";
import { BranchIcon, FileDiffIcon } from "../components/icons";

const PAGE = 50;

/** 栏头:小标题 + 可选图标,统一三栏顶部观感 */
function ColumnHead({ icon, children }: { icon?: React.ReactNode; children: React.ReactNode }) {
  return (
    <div className="flex shrink-0 items-center gap-1.5 border-b border-line px-3 py-2 text-[11px] font-semibold uppercase tracking-wide text-fg-muted">
      {icon}
      {children}
    </div>
  );
}

export function HistoryView({ repo }: { repo: string }) {
  const [commits, setCommits] = useState<CommitDto[]>([]);
  const [branch, setBranch] = useState<string | null>(null);
  const [selected, setSelected] = useState<CommitDto | null>(null);
  const [files, setFiles] = useState<FileChangeDto[]>([]);
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [diff, setDiff] = useState<FileDiffDto | null>(null);
  const [diffLoading, setDiffLoading] = useState(false);
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

  // 仅 "ref"(新提交/切分支)影响历史:重载首页 + 分支,不被工作区改动打断
  useEffect(() => {
    let un: (() => void) | undefined;
    onRepoChanged((kind) => {
      if (kind !== "ref") return;
      loadPage(0);
      getCurrentBranch(repo).then(setBranch).catch(() => setBranch(null));
    }).then((u) => { un = u; });
    return () => un?.();
    // eslint-disable-next-line
  }, [repo]);

  async function selectCommit(c: CommitDto) {
    setSelected(c); setSelectedFile(null); setFiles([]); setDiff(null);
    try { setFiles(await getCommitFiles(repo, c.id)); }
    catch (e) { setError((e as IpcError).message ?? String(e)); }
  }

  async function selectFile(path: string) {
    if (!selected) return;
    setSelectedFile(path); setDiff(null); setDiffLoading(true);
    try { setDiff(await getCommitFileDiff(repo, selected.id, path)); }
    catch (e) { setError((e as IpcError).message ?? String(e)); }
    finally { setDiffLoading(false); }
  }

  return (
    <div className="flex h-full">
      {/* 提交列表 */}
      <aside className="flex w-80 shrink-0 flex-col overflow-hidden border-r border-line">
        <ColumnHead icon={<BranchIcon width={13} height={13} />}>
          {branch ? <span className="font-mono normal-case tracking-normal text-fg">{branch}</span> : "提交历史"}
        </ColumnHead>
        {error && <p className="border-b border-line px-3 py-1.5 text-xs text-danger">{error}</p>}
        <CommitList
          commits={commits}
          branch={branch}
          selectedId={selected?.id ?? null}
          onSelect={selectCommit}
          onLoadMore={() => loadPage(commits.length)}
          loading={loading}
          hasMore={hasMore}
        />
      </aside>

      {/* 改动文件 */}
      <div className="flex w-64 shrink-0 flex-col overflow-hidden border-r border-line">
        <ColumnHead icon={<FileDiffIcon width={13} height={13} />}>改动文件{selected && ` (${files.length})`}</ColumnHead>
        {selected
          ? <CommitFileList files={files} selected={selectedFile} onSelect={selectFile} />
          : <div className="p-3 text-xs text-fg-subtle">选择一个提交</div>}
      </div>

      {/* Diff */}
      <main className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <ColumnHead>
          {selectedFile ? <span className="font-mono normal-case tracking-normal text-fg">{selectedFile}</span> : "Diff"}
        </ColumnHead>
        <DiffView diff={diff} loading={diffLoading} hasFile={!!selectedFile} />
      </main>
    </div>
  );
}
