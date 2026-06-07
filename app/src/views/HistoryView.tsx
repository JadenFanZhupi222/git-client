import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { cherryPick, revert, type CommitDto, type GraphRowDto, type FileChangeDto, type IpcError } from "../ipc";
import { useGraph, useCommitSearch, useCommitFiles, useCommitDiff, useCurrentBranch, invalidateHistory, invalidateWorktree, qk } from "../lib/queries";
import { CommitGraph } from "../components/CommitGraph";
import { CommitLines } from "../components/CommitLines";
import { CommitFileList } from "../components/CommitFileList";
import { CommitDetail } from "../components/CommitDetail";
import { DiffView } from "../components/DiffView";
import { Resizer, useResizableWidth } from "../components/Resizer";
import { useToast } from "../components/Toast";
import { BranchIcon, CommitIcon, FileDiffIcon, SearchIcon, CloseIcon } from "../components/icons";

const PAGE = 50;
const SEARCH_LIMIT = 200;

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
  const [busy, setBusy] = useState(false);
  const [searchInput, setSearchInput] = useState("");
  const [query, setQuery] = useState(""); // debounce 后的查询
  const qc = useQueryClient();
  const toast = useToast();

  // 输入防抖:停止输入 300ms 后才真正查询,避免每个字符一次 IPC。
  useEffect(() => {
    const t = setTimeout(() => setQuery(searchInput), 300);
    return () => clearTimeout(t);
  }, [searchInput]);
  const searching = query.trim().length > 0;

  // 图谱从 HEAD 整段计算(skip=0,limit 递增),保证泳道一致。失效/limit 变化自动重取。
  const graphQ = useGraph(repo, limit);
  const rows = graphQ.data ?? [];
  const searchQ = useCommitSearch(repo, query, SEARCH_LIMIT);
  const branchQ = useCurrentBranch(repo);
  const filesQ = useCommitFiles(repo, selected?.id ?? null);
  const diffQ = useCommitDiff(repo, selected?.id ?? null, selectedFile);

  const hasMore = rows.length === limit;
  const errMsg = (e: unknown) => (e as IpcError | null)?.message ?? null;
  const error = errMsg(graphQ.error) ?? errMsg(filesQ.error) ?? errMsg(diffQ.error);

  // 切仓库:重置分页、选择与搜索
  useEffect(() => { setLimit(PAGE); setSelected(null); setSelectedFile(null); setSearchInput(""); setQuery(""); }, [repo]);

  function selectCommit(c: CommitDto) {
    setSelected(c);
    setSelectedFile(null);
  }

  async function doCherryPick(commit: CommitDto) {
    setBusy(true);
    try {
      await cherryPick(repo, commit.id);
      invalidateHistory(qc, repo);
      invalidateWorktree(qc, repo);
      qc.invalidateQueries({ queryKey: qk.repoState(repo) });
      toast({ kind: "success", title: `已拣选 ${commit.short_id} 到当前分支` });
    } catch (e) {
      const err = e as IpcError;
      // 冲突也进入 cherry-pick 中 → 刷新让「更改」页出现冲突与横幅
      invalidateWorktree(qc, repo);
      qc.invalidateQueries({ queryKey: qk.repoState(repo) });
      toast({
        kind: "error",
        title: err.code === "MERGE_CONFLICT" ? "拣选有冲突,请到「更改」页解决" : (err.message ?? String(e)),
      });
    } finally {
      setBusy(false);
    }
  }

  async function doRevert(commit: CommitDto) {
    setBusy(true);
    try {
      await revert(repo, commit.id);
      invalidateHistory(qc, repo);
      invalidateWorktree(qc, repo);
      qc.invalidateQueries({ queryKey: qk.repoState(repo) });
      toast({ kind: "success", title: `已回滚 ${commit.short_id}` });
    } catch (e) {
      const err = e as IpcError;
      // 冲突进入 reverting 中 → 刷新让「更改」页出现冲突与横幅
      invalidateWorktree(qc, repo);
      qc.invalidateQueries({ queryKey: qk.repoState(repo) });
      toast({
        kind: "error",
        title: err.code === "MERGE_CONFLICT" ? "回滚有冲突,请到「更改」页解决" : (err.message ?? String(e)),
      });
    } finally {
      setBusy(false);
    }
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
        searchInput={searchInput}
        onSearchChange={setSearchInput}
        searching={searching}
        searchResults={searchQ.data ?? []}
        searchLoading={searchQ.isFetching}
      />

      {/* 中间列:提交详情(上)+ 改动文件(下) */}
      <MidColumn
        commit={selected}
        files={filesQ.data ?? []}
        selectedFile={selectedFile}
        onSelectFile={setSelectedFile}
        onCherryPick={selected ? () => doCherryPick(selected) : undefined}
        onRevert={selected ? () => doRevert(selected) : undefined}
        busy={busy}
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

/** 图谱列(含可拖拽宽度 + 提交搜索)。搜索时切扁平匹配列表,清空回到图谱。 */
function GraphColumn({
  branch, rows, selectedId, onSelect, onLoadMore, loading, firstLoad, hasMore, error,
  searchInput, onSearchChange, searching, searchResults, searchLoading,
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
  searchInput: string;
  onSearchChange: (v: string) => void;
  searching: boolean;
  searchResults: CommitDto[];
  searchLoading: boolean;
}) {
  const col = useResizableWidth("history.graphW", 320, 220, 640);
  return (
    <>
      <div className="flex shrink-0 flex-col overflow-hidden" style={{ width: col.w }}>
        <ColumnHead icon={<BranchIcon width={13} height={13} />}>
          {branch ? <span className="font-mono normal-case tracking-normal text-fg">{branch}</span> : "提交历史"}
        </ColumnHead>
        {/* 搜索框:按 message / 作者 / SHA 过滤 */}
        <div className="flex shrink-0 items-center gap-1.5 border-b border-line px-2.5 py-1.5">
          <SearchIcon width={13} height={13} className="shrink-0 text-fg-subtle" />
          <input
            value={searchInput}
            onChange={(e) => onSearchChange(e.target.value)}
            placeholder="搜索提交(信息 / 作者 / SHA)"
            className="min-w-0 flex-1 bg-transparent text-xs text-fg placeholder:text-fg-subtle focus:outline-none"
          />
          {searchInput && (
            <button onClick={() => onSearchChange("")} title="清除" className="shrink-0 text-fg-subtle hover:text-fg">
              <CloseIcon width={12} height={12} />
            </button>
          )}
        </div>
        {error && <p className="border-b border-line px-3 py-1.5 text-xs text-danger">{error}</p>}
        {searching ? (
          <SearchList results={searchResults} loading={searchLoading} selectedId={selectedId} onSelect={onSelect} />
        ) : (
          <CommitGraph
            rows={rows}
            selectedId={selectedId}
            onSelect={onSelect}
            onLoadMore={onLoadMore}
            loading={firstLoad || loading}
            hasMore={hasMore}
          />
        )}
      </div>
      <Resizer onDown={col.onDown} />
    </>
  );
}

/** 搜索结果:扁平提交列表(无泳道)。 */
function SearchList({
  results, loading, selectedId, onSelect,
}: {
  results: CommitDto[];
  loading: boolean;
  selectedId: string | null;
  onSelect: (c: CommitDto) => void;
}) {
  if (loading && results.length === 0) {
    return <div className="p-3 text-xs text-fg-subtle">搜索中…</div>;
  }
  if (results.length === 0) {
    return <div className="p-3 text-xs text-fg-subtle">没有匹配的提交</div>;
  }
  return (
    <div className="fade-in overflow-y-auto">
      <div className="px-3 py-1.5 text-[11px] text-fg-subtle">{results.length} 条匹配{results.length >= SEARCH_LIMIT ? "(已截断)" : ""}</div>
      {results.map((c) => {
        const on = selectedId === c.id;
        return (
          <div
            key={c.id}
            onClick={() => onSelect(c)}
            className={`flex cursor-pointer items-center gap-2 border-l-2 px-3 py-2 transition-colors ${
              on ? "border-accent-emphasis bg-overlay" : "border-transparent hover:bg-elevated"
            }`}
          >
            <span className="h-2 w-2 shrink-0 rounded-full bg-fg-subtle" />
            <div className="min-w-0 flex-1">
              <CommitLines commit={c} />
            </div>
          </div>
        );
      })}
    </div>
  );
}

/** 中间列:提交详情 + 改动文件(含可拖拽宽度) */
function MidColumn({
  commit, files, selectedFile, onSelectFile, onCherryPick, onRevert, busy,
}: {
  commit: CommitDto | null;
  files: FileChangeDto[];
  selectedFile: string | null;
  onSelectFile: (path: string) => void;
  onCherryPick?: () => void;
  onRevert?: () => void;
  busy?: boolean;
}) {
  const col = useResizableWidth("history.midW", 288, 200, 640);
  const list = files;
  return (
    <>
      <div className="flex shrink-0 flex-col overflow-hidden" style={{ width: col.w }}>
        <div className="flex shrink-0 items-center gap-1.5 border-b border-line px-3 py-2 text-[11px] font-semibold uppercase tracking-wide text-fg-muted">
          <CommitIcon width={13} height={13} />
          <span>提交详情</span>
          {commit && (onCherryPick || onRevert) && (
            <div className="ml-auto flex items-center gap-1">
              {onCherryPick && (
                <button
                  onClick={onCherryPick}
                  disabled={busy}
                  title="把此提交拣选(cherry-pick)到当前分支"
                  className="rounded border border-line-strong bg-elevated px-1.5 py-0.5 text-[11px] normal-case tracking-normal text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-40"
                >
                  Cherry-pick
                </button>
              )}
              {onRevert && (
                <button
                  onClick={onRevert}
                  disabled={busy}
                  title="回滚此提交(生成一个抵消其改动的新提交)"
                  className="rounded border border-line-strong bg-elevated px-1.5 py-0.5 text-[11px] normal-case tracking-normal text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-40"
                >
                  Revert
                </button>
              )}
            </div>
          )}
        </div>
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
