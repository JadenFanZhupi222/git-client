import { useEffect, useState } from "react";
import { type CommitDto, type GraphRowDto, type FileChangeDto } from "../ipc";
import { useGraph, useCommitFiles, useCommitDiff, useCurrentBranch } from "../lib/queries";
import { CommitGraph } from "../components/CommitGraph";
import { CommitFileList } from "../components/CommitFileList";
import { CommitDetail } from "../components/CommitDetail";
import { DiffView } from "../components/DiffView";
import { Resizer, useResizableWidth } from "../components/Resizer";
import { BranchIcon, CommitIcon, FileDiffIcon } from "../components/icons";

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
  const [limit, setLimit] = useState(PAGE);
  const [selected, setSelected] = useState<CommitDto | null>(null);
  const [selectedFile, setSelectedFile] = useState<string | null>(null);

  // 图谱从 HEAD 整段计算(skip=0,limit 递增),保证泳道一致。失效/limit 变化自动重取。
  const graphQ = useGraph(repo, limit);
  const rows = graphQ.data ?? [];
  const branchQ = useCurrentBranch(repo);
  const filesQ = useCommitFiles(repo, selected?.id ?? null);
  const diffQ = useCommitDiff(repo, selected?.id ?? null, selectedFile);

  const hasMore = rows.length === limit;
  const error =
    (graphQ.error as { message?: string } | null)?.message ??
    (filesQ.error as { message?: string } | null)?.message ??
    (diffQ.error as { message?: string } | null)?.message ??
    null;

  // 切仓库:重置分页与选择
  useEffect(() => { setLimit(PAGE); setSelected(null); setSelectedFile(null); }, [repo]);

  function selectCommit(c: CommitDto) {
    setSelected(c);
    setSelectedFile(null);
  }

  return (
    <div className="flex h-full">
      {/* 提交图谱 */}
      <GraphColumn
        branch={branchQ.data ?? null}
        rows={rows}
        selectedId={selected?.id ?? null}
        onSelect={selectCommit}
        onLoadMore={() => setLimit((l) => l + PAGE)}
        loading={graphQ.isFetching}
        firstLoad={graphQ.isLoading}
        hasMore={hasMore}
        error={error}
      />

      {/* 中间列:提交详情(上)+ 改动文件(下) */}
      <MidColumn
        commit={selected}
        files={filesQ.data ?? []}
        selectedFile={selectedFile}
        onSelectFile={setSelectedFile}
      />

      {/* Diff */}
      <main className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <ColumnHead>
          {selectedFile ? <span className="font-mono normal-case tracking-normal text-fg">{selectedFile}</span> : "Diff"}
        </ColumnHead>
        <DiffView diff={diffQ.data ?? null} loading={diffQ.isLoading} hasFile={!!selectedFile} />
      </main>
    </div>
  );
}

/** 图谱列(含可拖拽宽度) */
function GraphColumn({
  branch, rows, selectedId, onSelect, onLoadMore, loading, firstLoad, hasMore, error,
}: {
  branch: string | null;
  rows: GraphRowDto[];
  selectedId: string | null;
  onSelect: (c: CommitDto) => void;
  onLoadMore: () => void;
  loading: boolean;
  firstLoad: boolean;
  hasMore: boolean;
  error: string | null;
}) {
  const col = useResizableWidth("history.graphW", 320, 220, 640);
  return (
    <>
      <div className="flex shrink-0 flex-col overflow-hidden" style={{ width: col.w }}>
        <ColumnHead icon={<BranchIcon width={13} height={13} />}>
          {branch ? <span className="font-mono normal-case tracking-normal text-fg">{branch}</span> : "提交历史"}
        </ColumnHead>
        {error && <p className="border-b border-line px-3 py-1.5 text-xs text-danger">{error}</p>}
        <CommitGraph
          rows={rows}
          selectedId={selectedId}
          onSelect={onSelect}
          onLoadMore={onLoadMore}
          loading={firstLoad || loading}
          hasMore={hasMore}
        />
      </div>
      <Resizer onDown={col.onDown} />
    </>
  );
}

/** 中间列:提交详情 + 改动文件(含可拖拽宽度) */
function MidColumn({
  commit, files, selectedFile, onSelectFile,
}: {
  commit: CommitDto | null;
  files: FileChangeDto[];
  selectedFile: string | null;
  onSelectFile: (path: string) => void;
}) {
  const col = useResizableWidth("history.midW", 288, 200, 640);
  const list = files;
  return (
    <>
      <div className="flex shrink-0 flex-col overflow-hidden" style={{ width: col.w }}>
        <ColumnHead icon={<CommitIcon width={13} height={13} />}>提交详情</ColumnHead>
        <div className="max-h-[45%] shrink-0 overflow-hidden border-b border-line">
          <CommitDetail commit={commit} />
        </div>
        <ColumnHead icon={<FileDiffIcon width={13} height={13} />}>改动文件{commit && ` (${list.length})`}</ColumnHead>
        <div className="min-h-0 flex-1 overflow-hidden">
          {commit
            ? <CommitFileList files={list} selected={selectedFile} onSelect={onSelectFile} />
            : <div className="p-3 text-xs text-fg-subtle">选择一个提交</div>}
        </div>
      </div>
      <Resizer onDown={col.onDown} />
    </>
  );
}
