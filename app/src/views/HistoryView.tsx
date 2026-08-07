import { lazy, Suspense, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { cherryPick, revert, type CommitDto, type GraphRowDto, type FileChangeDto, type IpcError } from "../ipc";
import { useGraph, useCommitSearch, usePickaxe, useCommitFiles, useCommitDiff, useCurrentBranch, invalidateHistory, invalidateWorktree, qk } from "../lib/queries";
import { useListKeyboardNav, isTypingTarget } from "../lib/listNav";
import { CommitGraph } from "../components/CommitGraph";
import { CommitLines } from "../components/CommitLines";
import { TagManager } from "../components/TagManager";
import { ResetMenu } from "../components/ResetMenu";
import { ComparePanel } from "../components/ComparePanel";
import { CommitContextMenu } from "../components/CommitContextMenu";
import { Button } from "../components/ui/Button";
import { IconButton } from "../components/ui/IconButton";
import { Glass } from "../components/ui/Glass";
import { Spine } from "../components/ui/Spine";
import { CommitFileList } from "../components/CommitFileList";
import { EmptyHint } from "../components/ui/EmptyHint";
import { CommitDetail } from "../components/CommitDetail";
import { DiffView } from "../components/DiffView";
import { Resizer, useResizableWidth } from "../components/Resizer";
import { useToast } from "../components/Toast";
import { BranchIcon, CommitIcon, FileDiffIcon, SearchIcon, CloseIcon } from "../components/icons";
import { useT } from "../lib/i18n";
import { readHistorySelection, writeHistorySelection, type HistorySelection } from "../lib/historySelection";
import type { MessageKey } from "../lib/locales/zh";

const PAGE = 50;
const SEARCH_LIMIT = 200;
const RebaseEditor = lazy(() => import("../components/RebaseEditor").then((module) => ({ default: module.RebaseEditor })));
const ReflogPanel = lazy(() => import("../components/ReflogPanel").then((module) => ({ default: module.ReflogPanel })));
const FileHistoryPanel = lazy(() => import("../components/FileHistoryPanel").then((module) => ({ default: module.FileHistoryPanel })));

/** 栏头:小标题 + 可选图标,统一三栏顶部观感 */
function ColumnHead({ icon, children }: { icon?: React.ReactNode; children: React.ReactNode }) {
  return (
    <div className="flex min-h-8 shrink-0 items-center gap-1.5 border-b border-line px-3 py-1.5 text-xs font-semibold text-fg-muted">
      {icon}
      {children}
    </div>
  );
}

function PaneError({ message, onRetry, fill = false }: { message: string; onRetry: () => void; fill?: boolean }) {
  const t = useT();
  return (
    <div
      role="alert"
      className={`flex items-center gap-2 border-line px-3 py-2 text-xs text-danger ${fill ? "flex-1 justify-center text-center" : "border-b"}`}
    >
      <span className="min-w-0 break-words">{message}</span>
      <Button variant="secondary" size="chip" onClick={onRetry} className="shrink-0">
        {t("common.retry")}
      </Button>
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
  const [searchMode, setSearchMode] = useState<SearchMode>("message"); // 信息 / 内容-S / 正则-G
  const [rebaseOpen, setRebaseOpen] = useState(false);
  const [reflogOpen, setReflogOpen] = useState(false);
  const [historyFile, setHistoryFile] = useState<string | null>(null); // 文件历史面板:当前查看的文件
  const [compareWith, setCompareWith] = useState<CommitDto | null>(null); // 比较模式的第二个提交
  const [menu, setMenu] = useState<{ commit: CommitDto; x: number; y: number } | null>(null);
  const [focusedPane, setFocusedPane] = useState<"commits" | "files">("commits"); // 键盘焦点在哪个列表
  const [kbMode, setKbMode] = useState(false); // 最近一次交互是否来自键盘(决定是否显示聚焦环)
  const restoredSelection = useRef<HistorySelection | null>(null);
  const selectionReady = useRef(false);
  const compactLayout = useCompactHistoryLayout();
  const t = useT();
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
  // 三种搜索后端,按模式只激活一个;结果都是 CommitDto[],喂同一个列表。
  const searchQ = useCommitSearch(repo, query, SEARCH_LIMIT, searchMode === "message");
  const pickaxeQ = usePickaxe(repo, query, searchMode === "regex", SEARCH_LIMIT, searchMode !== "message");
  const activeSearchQ = searchMode === "message" ? searchQ : pickaxeQ;
  const branchQ = useCurrentBranch(repo);
  const filesQ = useCommitFiles(repo, selected?.id ?? null);
  const diffQ = useCommitDiff(repo, selected?.id ?? null, selectedFile);

  const hasMore = rows.length === limit;
  const errMsg = (error: unknown) => {
    if (!error) return null;
    return (error as IpcError | null)?.message ?? (error instanceof Error ? error.message : t("common.lazyLoadFailed"));
  };
  const graphError = errMsg(activeSearchQ.error) ?? errMsg(graphQ.error);
  const filesError = errMsg(filesQ.error);
  const diffError = errMsg(diffQ.error);
  // 选中提交的已有标签(从图谱行的 refs 派生;窗口外/搜索结果无行则为空,仍可新建)
  const selectedTags = selected
    ? (rows.find((r) => r.commit.id === selected.id)?.refs.filter((x) => x.kind === "tag").map((x) => x.name) ?? [])
    : [];

  // 交互式变基范围:从选中提交(最旧)到 HEAD(最新)。仅当选中提交在图谱窗口内可用。
  const selIdx = selected ? rows.findIndex((r) => r.commit.id === selected.id) : -1;
  const rebaseCommits = selIdx >= 0 ? rows.slice(0, selIdx + 1).map((r) => r.commit).reverse() : [];
  const rebaseBase = selected ? (selected.parents[0] ?? null) : null;
  const refresh = () => {
    invalidateHistory(qc, repo);
    invalidateWorktree(qc, repo);
    qc.invalidateQueries({ queryKey: qk.repoState(repo) });
  };
  const afterRebase = () => { setRebaseOpen(false); refresh(); };

  // 切仓库:重置分页、选择与搜索
  useEffect(() => {
    restoredSelection.current = readHistorySelection(repo);
    selectionReady.current = false;
    setLimit(PAGE);
    setSelected(null);
    setSelectedFile(null);
    setSearchInput("");
    setQuery("");
    setSearchMode("message");
    setRebaseOpen(false);
    setCompareWith(null);
    setMenu(null);
    setFocusedPane("commits");
  }, [repo]);

  // Cmd/Ctrl+点击第二个提交 → 进入比较模式;普通点击 → 单选并退出比较。
  function selectCommit(c: CommitDto, opts?: { compare?: boolean }) {
    if (opts?.compare && selected && selected.id !== c.id) {
      setCompareWith(c);
      return;
    }
    restoredSelection.current = null;
    selectionReady.current = true;
    setSelected(c);
    setSelectedFile(null);
    setCompareWith(null);
    setFocusedPane("commits"); // 选提交把键盘焦点带回提交列表
  }

  // 点文件:选中并把键盘焦点移到文件列表
  function selectFile(path: string) {
    setSelectedFile(path);
    setFocusedPane("files");
  }

  // 比较两端按时间定序:旧 = from,新 = to(diff 读作 旧→新)。
  const comparing = !!(selected && compareWith);
  const cmpOlderFirst = comparing && selected!.timestamp <= compareWith!.timestamp;
  const cmpFrom = comparing ? (cmpOlderFirst ? selected! : compareWith!) : null;
  const cmpTo = comparing ? (cmpOlderFirst ? compareWith! : selected!) : null;

  // 键盘导航。两个可导航列表:提交列表(commits)与改动文件列表(files);j/k 作用于「聚焦的」那个。
  // 弹层/比较/右键菜单打开时整体让出键盘。
  const modalsOpen = comparing || rebaseOpen || reflogOpen || !!historyFile || !!menu;
  const files = filesQ.data ?? [];

  // Arrive ready to work: restore a valid previous commit, otherwise select HEAD.
  useEffect(() => {
    if (selected || rows.length === 0) return;
    const saved = restoredSelection.current;
    const commit = rows.find((row) => row.commit.id === saved?.commitId)?.commit
      ?? rows.find((row) => row.refs.some((ref) => ref.kind === "head"))?.commit
      ?? rows[0].commit;
    selectionReady.current = true;
    setSelected(commit);
    setSelectedFile(null);
    setFocusedPane("commits");
  }, [rows, selected]);

  // Once files arrive, restore the prior file when possible or open the first diff.
  useEffect(() => {
    if (!selected || selectedFile || filesQ.isLoading || files.length === 0) return;
    const saved = restoredSelection.current;
    const file = saved?.commitId === selected.id && saved.file && files.some((item) => item.path === saved.file)
      ? saved.file
      : files[0].path;
    restoredSelection.current = null;
    setSelectedFile(file);
  }, [files, filesQ.isLoading, selected, selectedFile]);

  useEffect(() => {
    if (!selectionReady.current || !selected) return;
    writeHistorySelection(repo, { commitId: selected.id, file: selectedFile });
  }, [repo, selected, selectedFile]);

  // ① 提交列表:搜索态=搜索结果,否则=图谱行。
  const navList: CommitDto[] = searching ? (activeSearchQ.data ?? []) : rows.map((r) => r.commit);
  const navIndex = selected ? navList.findIndex((c) => c.id === selected.id) : -1;
  useListKeyboardNav({
    count: navList.length,
    index: navIndex,
    enabled: !modalsOpen && focusedPane === "commits",
    onSelect: (i) => selectCommit(navList[i]),
  });

  // ② 改动文件列表。
  const fileIndex = selectedFile ? files.findIndex((f) => f.path === selectedFile) : -1;
  useListKeyboardNav({
    count: files.length,
    index: fileIndex,
    enabled: !modalsOpen && focusedPane === "files",
    onSelect: (i) => selectFile(files[i].path),
  });

  // 输入模态:键盘导航键 → 进入键盘模式(显示聚焦环);任何鼠标按下 → 退出(隐藏环)。
  // 这样鼠标点选不会留下蓝色聚焦环,只有真正用键盘时才提示「焦点在哪个面板」。
  useEffect(() => {
    const NAV_KEYS = ["j", "k", "g", "G", "h", "l", "Tab", "Enter", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End"];
    const onKey = (e: KeyboardEvent) => {
      if (isTypingTarget(document.activeElement)) return;
      if (NAV_KEYS.includes(e.key)) setKbMode(true);
    };
    const onPointer = () => setKbMode(false);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("pointerdown", onPointer, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("pointerdown", onPointer, true);
    };
  }, []);

  // 面板间切焦点:Tab 来回切;l/→/Enter 从提交进文件,h/← 从文件回提交。
  // 与 j/k/g/G 用的是不相交的键,两个 window 监听不打架。
  useEffect(() => {
    if (modalsOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      if (isTypingTarget(document.activeElement)) return;
      const hasFiles = files.length > 0;
      if (e.key === "Tab") {
        e.preventDefault();
        setFocusedPane((p) => (p === "commits" ? (hasFiles ? "files" : "commits") : "commits"));
      } else if ((e.key === "l" || e.key === "ArrowRight" || e.key === "Enter") && focusedPane === "commits" && hasFiles) {
        e.preventDefault();
        setFocusedPane("files");
      } else if ((e.key === "h" || e.key === "ArrowLeft") && focusedPane === "files") {
        e.preventDefault();
        setFocusedPane("commits");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [modalsOpen, files.length, focusedPane]);

  async function doCherryPick(commit: CommitDto) {
    setBusy(true);
    try {
      await cherryPick(repo, commit.id);
      invalidateHistory(qc, repo);
      invalidateWorktree(qc, repo);
      qc.invalidateQueries({ queryKey: qk.repoState(repo) });
      toast({ kind: "success", title: t("history.cherryPicked", { short: commit.short_id }) });
    } catch (e) {
      const err = e as IpcError;
      // 冲突也进入 cherry-pick 中 → 刷新让「更改」页出现冲突与横幅
      invalidateWorktree(qc, repo);
      qc.invalidateQueries({ queryKey: qk.repoState(repo) });
      toast({
        kind: "error",
        title: err.code === "MERGE_CONFLICT" ? t("history.cherryConflict") : (err.message ?? String(e)),
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
      toast({ kind: "success", title: t("history.reverted", { short: commit.short_id }) });
    } catch (e) {
      const err = e as IpcError;
      // 冲突进入 reverting 中 → 刷新让「更改」页出现冲突与横幅
      invalidateWorktree(qc, repo);
      qc.invalidateQueries({ queryKey: qk.repoState(repo) });
      toast({
        kind: "error",
        title: err.code === "MERGE_CONFLICT" ? t("history.revertConflict") : (err.message ?? String(e)),
      });
    } finally {
      setBusy(false);
    }
  }

  const renderMidColumn = (stacked = false) => (
    <MidColumn
      commit={selected}
      files={filesQ.data ?? []}
      filesLoading={filesQ.isLoading}
      filesError={filesError}
      onRetryFiles={() => { void filesQ.refetch(); }}
      selectedFile={selectedFile}
      focused={focusedPane === "files" && kbMode}
      onSelectFile={selectFile}
      onFileHistory={setHistoryFile}
      onCherryPick={selected ? () => doCherryPick(selected) : undefined}
      onRevert={selected ? () => doRevert(selected) : undefined}
      onRebase={selIdx >= 0 ? () => setRebaseOpen(true) : undefined}
      repo={repo}
      tags={selectedTags}
      onTagsChanged={() => invalidateHistory(qc, repo)}
      onResetDone={() => {
        invalidateHistory(qc, repo);
        invalidateWorktree(qc, repo);
        qc.invalidateQueries({ queryKey: qk.repoState(repo) });
      }}
      busy={busy}
      stacked={stacked}
    />
  );

  const renderDiffPane = (wide = false) => (
    <main className={`flex min-w-0 flex-1 flex-col overflow-hidden ${wide ? "min-w-[560px]" : ""}`}>
      <ColumnHead>
        {selectedFile ? <span className="truncate font-mono normal-case tracking-normal text-fg" title={selectedFile}>{selectedFile}</span> : "Diff"}
      </ColumnHead>
      {diffQ.isLoading && <span role="status" className="sr-only">{t("history.loadingDiff")}</span>}
      {diffError
        ? <PaneError message={diffError} onRetry={() => { void diffQ.refetch(); }} fill />
        : <DiffView diff={diffQ.data ?? null} loading={diffQ.isLoading} hasFile={!!selectedFile} repo={repo} />}
    </main>
  );

  return (
    <div className="flex h-full min-w-0 overflow-hidden">
      {/* 本视图写操作(cherry-pick / revert)进行中的非阻塞信号:与 App 顶栏同款进度条。
          拖放拣选丢下后到 toast 之间不再「静默」。 */}
      {busy && (
        <div className="pointer-events-none fixed inset-x-0 top-0 z-[70] h-0.5 overflow-hidden bg-accent/15">
          <div className="progress-bar h-full w-1/3 bg-accent" />
        </div>
      )}
      {/* 提交图谱 */}
      <GraphColumn
        branch={branchQ.data ?? null}
        rows={rows}
        selectedId={selected?.id ?? null}
        compareId={compareWith?.id ?? null}
        focused={focusedPane === "commits" && kbMode}
        onSelect={selectCommit}
        onContext={(c, x, y) => setMenu({ commit: c, x, y })}
        onCherryPick={doCherryPick}
        onLoadMore={() => setLimit((l) => l + PAGE)}
        loading={graphQ.isFetching}
        firstLoad={graphQ.isLoading}
        hasMore={hasMore}
        error={graphError}
        onRetryError={() => { void (searching ? activeSearchQ.refetch() : graphQ.refetch()); }}
        searchInput={searchInput}
        onSearchChange={setSearchInput}
        searchMode={searchMode}
        onSearchModeChange={setSearchMode}
        searching={searching}
        searchResults={activeSearchQ.data ?? []}
        searchLoading={searchInput.trim() !== query.trim() || activeSearchQ.isFetching}
        onOpenReflog={() => setReflogOpen(true)}
        compact={compactLayout}
      />

      {comparing && cmpFrom && cmpTo ? (
        /* 比较模式:横幅 + 两提交的改动文件/diff(占据中+右区域) */
        <main className="flex min-w-0 flex-1 flex-col overflow-hidden">
          <div className="flex shrink-0 items-center gap-2 border-b border-line bg-accent/5 px-3 py-2 text-xs">
            <span className="text-fg-muted">{t("history.compareLabel")}</span>
            <span className="font-mono text-accent">{cmpFrom.short_id}</span>
            <span className="truncate text-fg-subtle" title={cmpFrom.summary}>{cmpFrom.summary}</span>
            <span className="shrink-0 text-fg-subtle">→</span>
            <span className="font-mono text-accent">{cmpTo.short_id}</span>
            <span className="truncate text-fg-subtle" title={cmpTo.summary}>{cmpTo.summary}</span>
            <IconButton aria-label={t("history.exitCompare")} title={t("history.exitCompare")} onClick={() => setCompareWith(null)} className="ml-auto shrink-0">
              <CloseIcon width={14} height={14} />
            </IconButton>
          </div>
          <ComparePanel repo={repo} from={cmpFrom.id} to={cmpTo.id} />
        </main>
      ) : (
        compactLayout ? (
          <section className="flex min-w-0 flex-1 flex-col overflow-hidden">
            <div className="h-[42%] min-h-[240px] max-h-[420px] shrink-0 border-b border-line">
              {renderMidColumn(true)}
            </div>
            {renderDiffPane()}
          </section>
        ) : (
          <>
            {renderMidColumn()}
            {renderDiffPane(true)}
          </>
        )
      )}

      <Suspense fallback={null}>
        {rebaseOpen && selected && rebaseCommits.length > 0 && (
          <RebaseEditor
            repo={repo}
            commits={rebaseCommits}
            base={rebaseBase}
            onClose={() => setRebaseOpen(false)}
            onConflict={afterRebase}
            onDone={afterRebase}
          />
        )}

        {reflogOpen && (
          <ReflogPanel
            repo={repo}
            onClose={() => setReflogOpen(false)}
            onReset={() => {
              invalidateHistory(qc, repo);
              invalidateWorktree(qc, repo);
              qc.invalidateQueries({ queryKey: qk.repoState(repo) });
            }}
          />
        )}

        {historyFile && (
          <FileHistoryPanel repo={repo} file={historyFile} onClose={() => setHistoryFile(null)} />
        )}
      </Suspense>

      {menu && (
        <CommitContextMenu
          repo={repo}
          commit={menu.commit}
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          onCherryPick={() => doCherryPick(menu.commit)}
          onRevert={() => doRevert(menu.commit)}
          onRebase={() => setRebaseOpen(true)}
          onChanged={refresh}
          selectedShort={selected && selected.id !== menu.commit.id ? selected.short_id : undefined}
          onCompareWithSelected={
            selected && selected.id !== menu.commit.id ? () => setCompareWith(menu.commit) : undefined
          }
        />
      )}
    </div>
  );
}

/** 搜索模式:提交信息(git2) / 内容 -S / 正则 -G(后两者 = pickaxe)。 */
type SearchMode = "message" | "content" | "regex";
const SEARCH_PLACEHOLDER: Record<SearchMode, MessageKey> = {
  message: "history.searchMessage",
  content: "history.searchContent",
  regex: "history.searchRegex",
};
const SEARCH_MODE_LABEL: Record<SearchMode, MessageKey> = { message: "history.modeMessage", content: "history.modeContent", regex: "history.modeRegex" };
const SEARCH_MODES = ["message", "content", "regex"] as const;

function historyColumnDefault(column: "graph" | "detail"): number {
  const viewport = typeof window === "undefined" ? 1240 : window.innerWidth;
  if (viewport >= 1600) {
    const ratio = column === "graph" ? 0.23 : 0.18;
    const min = column === "graph" ? 360 : 320;
    const max = column === "graph" ? 480 : 400;
    return Math.round(Math.min(max, Math.max(min, viewport * ratio)));
  }
  if (viewport >= 1200) return column === "graph" ? 340 : 300;
  return column === "graph" ? 280 : 240;
}

function useCompactHistoryLayout(): boolean {
  const query = "(max-width: 1639px)";
  const read = () => typeof window !== "undefined"
    && (typeof window.matchMedia === "function" ? window.matchMedia(query).matches : window.innerWidth < 1640);
  const [compact, setCompact] = useState(read);
  useEffect(() => {
    if (typeof window === "undefined") return;
    if (typeof window.matchMedia !== "function") {
      const onResize = () => setCompact(read());
      window.addEventListener("resize", onResize);
      return () => window.removeEventListener("resize", onResize);
    }
    const media = window.matchMedia(query);
    const onChange = () => setCompact(media.matches);
    media.addEventListener("change", onChange);
    setCompact(media.matches);
    return () => media.removeEventListener("change", onChange);
  }, []);
  return compact;
}

/** 图谱列(含可拖拽宽度 + 提交搜索)。搜索时切扁平匹配列表,清空回到图谱。 */
function GraphColumn({
  branch, rows, selectedId, compareId, focused, onSelect, onContext, onCherryPick, onLoadMore, loading, firstLoad, hasMore, error,
  onRetryError, searchInput, onSearchChange, searchMode, onSearchModeChange, searching, searchResults, searchLoading, onOpenReflog, compact,
}: {
  branch: string | null;
  rows: GraphRowDto[];
  selectedId: string | null;
  compareId: string | null;
  focused: boolean;
  onSelect: (c: CommitDto, opts?: { compare?: boolean }) => void;
  onContext: (c: CommitDto, x: number, y: number) => void;
  onCherryPick: (c: CommitDto) => void;
  onLoadMore: () => void;
  loading: boolean;
  firstLoad: boolean;
  hasMore: boolean;
  error: string | null;
  onRetryError: () => void;
  searchInput: string;
  onSearchChange: (v: string) => void;
  searchMode: SearchMode;
  onSearchModeChange: (m: SearchMode) => void;
  searching: boolean;
  searchResults: CommitDto[];
  searchLoading: boolean;
  onOpenReflog: () => void;
  compact: boolean;
}) {
  const t = useT();
  const col = useResizableWidth("history.graphW", historyColumnDefault("graph"), 320, compact ? 400 : 480);
  // 拖放拣选的一次性发现性提示(localStorage 记忆,用过即不再出现)。仅图谱模式且有数据时显示。
  const [dragHintDismissed, setDragHintDismissed] = useState(() => localStorage.getItem("hint.dragCherryPick") === "1");
  const showDragHint = !searching && rows.length > 0 && !dragHintDismissed;
  function dismissDragHint() { localStorage.setItem("hint.dragCherryPick", "1"); setDragHintDismissed(true); }
  // 浮动玻璃工具栏的实测高度 → 传给滚动体当顶部留白,让提交从栏底穿过(满汉折射)。
  // Cover the header and compact two-row search initially; the layout effect measures the exact height before paint.
  const barRef = useRef<HTMLDivElement>(null);
  const [barH, setBarH] = useState(100);
  useLayoutEffect(() => {
    const el = barRef.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => setBarH(el.offsetHeight));
    ro.observe(el);
    setBarH(el.offsetHeight);
    return () => ro.disconnect();
  }, []);
  const onSearchModeKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const current = SEARCH_MODES.indexOf(searchMode);
    const next = event.key === "Home"
      ? 0
      : event.key === "End"
        ? SEARCH_MODES.length - 1
        : (current + (event.key === "ArrowLeft" || event.key === "ArrowUp" ? -1 : 1) + SEARCH_MODES.length) % SEARCH_MODES.length;
    const mode = SEARCH_MODES[next];
    const group = event.currentTarget;
    onSearchModeChange(mode);
    requestAnimationFrame(() => group.querySelector<HTMLButtonElement>(`[data-search-mode="${mode}"]`)?.focus());
  };
  return (
    <>
      <div className={`relative flex shrink-0 flex-col overflow-hidden ${focused ? "ring-1 ring-inset ring-accent/50" : ""}`} style={{ width: col.w }}>
        {/* 滚动体铺满整列;提交从下方的浮动玻璃工具栏底下穿过 */}
        <div className="min-h-0 flex-1">
          {searching ? (
            <SearchList results={searchResults} loading={searchLoading} selectedId={selectedId} onSelect={onSelect} onContext={onContext} topInset={barH} />
          ) : (
            <CommitGraph
              rows={rows}
              selectedId={selectedId}
              compareId={compareId}
              scrollToId={selectedId}
              onSelect={onSelect}
              onContext={onContext}
              onCherryPick={onCherryPick}
              onLoadMore={onLoadMore}
              loading={firstLoad || loading}
              hasMore={hasMore}
              topInset={barH}
            />
          )}
        </div>
        {/* 浮动液态玻璃工具栏:栏头 + 搜索 + 模式,悬于滚动体之上,提交从其下穿过显折射。
            注意:定位放在外层普通 div —— `.glass` 自带 position:relative(unlayered)会压过
            Tailwind 的 absolute 工具类,直接给 Glass 加 absolute 不生效(会沉到底部)。 */}
        <div className="absolute inset-x-0 top-0 z-10">
          <Glass>
            <div ref={barRef}>
            <ColumnHead icon={<BranchIcon width={13} height={13} />}>
              {branch ? <span className="font-mono normal-case tracking-normal text-fg">{branch}</span> : t("history.commitHistory")}
              <Button
                variant="secondary"
                size="chip"
                onClick={onOpenReflog}
                title={t("history.reflogTitle")}
                className="ml-auto normal-case tracking-normal"
              >
                Reflog
              </Button>
            </ColumnHead>
            {/* Keep search primary and scope on a compact second row so narrow columns do not truncate either control. */}
            <div className="flex shrink-0 flex-col gap-1 border-b border-line px-2.5 py-1">
              <label htmlFor="history-commit-search" className="sr-only">{t("history.searchLabel")}</label>
              <div className="flex h-8 w-full items-center gap-2 rounded-md border border-line-strong bg-elevated/70 px-2 transition-colors focus-within:border-accent focus-within:ring-2 focus-within:ring-accent/15">
                <SearchIcon width={13} height={13} className="shrink-0 text-fg-muted" />
                <input
                  id="history-commit-search"
                  value={searchInput}
                  aria-busy={searchLoading}
                  onChange={(e) => onSearchChange(e.target.value)}
                  placeholder={t(SEARCH_PLACEHOLDER[searchMode])}
                  className="min-w-0 flex-1 bg-transparent text-[12.5px] text-fg placeholder:text-fg-muted focus-visible:!outline-none"
                />
                {searchInput && (
                  <IconButton aria-label={t("history.clearSearch")} onClick={() => onSearchChange("")} title={t("history.clear")} className="shrink-0">
                    <CloseIcon width={12} height={12} />
                  </IconButton>
                )}
              </div>
              <div
                role="radiogroup"
                aria-label={t("history.searchScope")}
                onKeyDown={onSearchModeKeyDown}
                className="grid h-6 w-52 shrink-0 grid-cols-3 place-items-center gap-0.5 self-start rounded-md border border-line-strong bg-elevated/55 p-0.5"
              >
                {SEARCH_MODES.map((m) => (
                  <button
                    key={m}
                    data-search-mode={m}
                    role="radio"
                    aria-checked={searchMode === m}
                    tabIndex={searchMode === m ? 0 : -1}
                    onClick={() => onSearchModeChange(m)}
                    title={t(SEARCH_PLACEHOLDER[m])}
                    className={`grid h-full w-full min-w-0 place-items-center whitespace-nowrap rounded px-1 text-center text-[11px] font-medium leading-none transition-colors ${
                      searchMode === m ? "bg-accent/15 text-accent-ink" : "text-fg-muted hover:bg-overlay hover:text-fg"
                    }`}
                  >
                    {t(SEARCH_MODE_LABEL[m])}
                  </button>
                ))}
              </div>
            </div>
            {searchLoading && <span role="status" className="sr-only">{t("history.searching")}</span>}
            {error && <PaneError message={error} onRetry={onRetryError} />}
            {showDragHint && (
              <div className="flex items-center gap-2 border-b border-line px-2.5 py-1.5 text-[11px] text-fg-muted">
                <span aria-hidden className="shrink-0 font-mono leading-none text-fg-subtle">⠿</span>
                <span className="min-w-0 flex-1 leading-snug">
                  {t("history.dragHintBefore")}<span className="text-accent">{t("history.dragHintHead")}</span>{t("history.dragHintAfter")}
                </span>
                <IconButton aria-label={t("history.dragHintDismissAria")} title={t("history.dragHintDismiss")} onClick={dismissDragHint} className="shrink-0">
                  <CloseIcon width={12} height={12} />
                </IconButton>
              </div>
            )}
            </div>
          </Glass>
        </div>
      </div>
      <Resizer
        value={col.w}
        min={col.min}
        max={col.max}
        label={t("common.resizePanel", { panel: t("history.commitHistory") })}
        onDown={col.onDown}
        onKeyDown={col.onKeyDown}
        onReset={col.reset}
      />
    </>
  );
}

/** 搜索结果:扁平提交列表(无泳道)。 */
function SearchList({
  results, loading, selectedId, onSelect, onContext, topInset = 0,
}: {
  results: CommitDto[];
  loading: boolean;
  selectedId: string | null;
  onSelect: (c: CommitDto) => void;
  onContext: (c: CommitDto, x: number, y: number) => void;
  /** 顶部留白(px):同 CommitGraph,给浮动玻璃工具栏让位。 */
  topInset?: number;
}) {
  const t = useT();
  const boxRef = useRef<HTMLDivElement>(null);
  // 键盘选中变化 → 把该行滚进可视区(block nearest:已可见则不动)。
  useEffect(() => {
    if (!selectedId) return;
    boxRef.current?.querySelector<HTMLElement>(`[data-id="${selectedId}"]`)?.scrollIntoView({ block: "nearest" });
  }, [selectedId]);

  if (loading && results.length === 0) {
    return <div role="status" className="p-3 text-xs text-fg-subtle" style={{ paddingTop: topInset + 12 }}>{t("history.searching")}</div>;
  }
  if (results.length === 0) {
    return <div className="p-3 text-xs text-fg-subtle" style={{ paddingTop: topInset + 12 }}>{t("history.noMatch")}</div>;
  }
  const selectedInResults = results.some((result) => result.id === selectedId);
  return (
    <div ref={boxRef} role="listbox" aria-label={t("history.searchResults")} className="fade-in h-full overflow-y-auto" style={{ paddingTop: topInset }}>
      <div className="px-3 py-1.5 text-[11px] text-fg-muted">{t("history.matchCount", { n: results.length })}{results.length >= SEARCH_LIMIT ? t("history.truncated") : ""}</div>
      {results.map((c, index) => {
        const on = selectedId === c.id;
        return (
          <div
            key={c.id}
            data-id={c.id}
            role="option"
            aria-selected={on}
            aria-label={`${c.summary}, ${c.author_name}, ${c.short_id}`}
            tabIndex={on || (!selectedInResults && index === 0) ? 0 : -1}
            onClick={() => onSelect(c)}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                onSelect(c);
              }
            }}
            onContextMenu={(e) => { e.preventDefault(); onSelect(c); onContext(c, e.clientX, e.clientY); }}
            className={`relative flex cursor-pointer items-center gap-2 px-3 py-2 transition-colors ${
              on ? "bg-accent/10" : "hover:bg-elevated"
            }`}
          >
            {on && <Spine />}
            <span className="h-2 w-2 shrink-0 rounded-full bg-fg-subtle" />
            <CommitLines commit={c} selected={on} />
          </div>
        );
      })}
    </div>
  );
}

/** 中间列:提交详情 + 改动文件(含可拖拽宽度) */
function MidColumn({
  repo, commit, files, filesLoading, filesError, onRetryFiles, selectedFile, focused, onSelectFile, onFileHistory, onCherryPick, onRevert, onRebase, onResetDone, tags, onTagsChanged, busy,
  stacked = false,
}: {
  repo: string;
  commit: CommitDto | null;
  files: FileChangeDto[];
  filesLoading: boolean;
  filesError: string | null;
  onRetryFiles: () => void;
  selectedFile: string | null;
  focused: boolean;
  onSelectFile: (path: string) => void;
  onFileHistory: (path: string) => void;
  onCherryPick?: () => void;
  onRevert?: () => void;
  onRebase?: () => void;
  onResetDone: () => void;
  tags: string[];
  onTagsChanged: () => void;
  busy?: boolean;
  stacked?: boolean;
}) {
  const t = useT();
  const col = useResizableWidth("history.midW", historyColumnDefault("detail"), 360, 400);
  const list = files;
  const content = (
      <div
        className={`flex h-full shrink-0 flex-col overflow-hidden ${focused ? "ring-1 ring-inset ring-accent/50" : ""}`}
        style={{ width: stacked ? "100%" : col.w }}
      >
        <div className="flex min-h-8 shrink-0 flex-wrap items-center gap-1.5 border-b border-line px-3 py-1.5 text-xs font-semibold text-fg-muted">
          <CommitIcon width={13} height={13} className="shrink-0" />
          <span className="whitespace-nowrap">{t("history.commitDetail")}</span>
          {commit && (
            <div className="ml-auto flex shrink-0 items-center gap-1">
              <ResetMenu repo={repo} commitId={commit.id} label={commit.short_id} onDone={onResetDone} />
              {onCherryPick && (
                <Button
                  variant="secondary"
                  size="chip"
                  onClick={onCherryPick}
                  disabled={busy}
                  title={t("history.cherryPickTitle")}
                  className="shrink-0 whitespace-nowrap normal-case tracking-normal"
                >
                  Cherry-pick
                </Button>
              )}
              {onRevert && (
                <Button
                  variant="secondary"
                  size="chip"
                  onClick={onRevert}
                  disabled={busy}
                  title={t("history.revertTitle")}
                  className="shrink-0 whitespace-nowrap normal-case tracking-normal"
                >
                  Revert
                </Button>
              )}
              {onRebase && (
                <Button
                  variant="secondary"
                  size="chip"
                  onClick={onRebase}
                  disabled={busy}
                  title={t("history.rebaseTitle")}
                  className="shrink-0 whitespace-nowrap normal-case tracking-normal"
                >
                  {t("history.rebase")}
                </Button>
              )}
            </div>
          )}
        </div>
        {commit && (
          <div className="shrink-0 border-b border-line px-3 py-1.5">
            <TagManager repo={repo} commit={commit} tags={tags} onChanged={onTagsChanged} />
          </div>
        )}
        <div className="max-h-[45%] shrink-0 overflow-hidden border-b border-line">
          <CommitDetail repo={repo} commit={commit} />
        </div>
        <ColumnHead icon={<FileDiffIcon width={13} height={13} />}>
          {t("history.changedFiles")}{commit && ` (${list.length})`}
          {commit && list.length > 0 && (() => {
            const add = list.reduce((s, f) => s + f.additions, 0);
            const del = list.reduce((s, f) => s + f.deletions, 0);
            return (
              <span className="ml-auto flex shrink-0 items-center gap-1.5 font-mono text-[10px] normal-case tracking-normal">
                {add > 0 && <span className="text-success">+{add}</span>}
                {del > 0 && <span className="text-danger">−{del}</span>}
              </span>
            );
          })()}
        </ColumnHead>
        {filesLoading && <span role="status" className="sr-only">{t("history.loadingFiles")}</span>}
        {filesError && <PaneError message={filesError} onRetry={onRetryFiles} />}
        <div className="min-h-0 flex-1 overflow-hidden">
          {commit && !filesError
            ? <CommitFileList files={list} selected={selectedFile} onSelect={onSelectFile} onFileHistory={onFileHistory} />
            : !commit
              ? <EmptyHint icon={<CommitIcon width={24} height={24} />}>{t("history.selectCommit")}</EmptyHint>
              : null}
        </div>
      </div>
  );
  if (stacked) return content;
  return (
    <>
      {content}
      <Resizer
        value={col.w}
        min={col.min}
        max={col.max}
        label={t("common.resizePanel", { panel: t("history.commitDetail") })}
        onDown={col.onDown}
        onKeyDown={col.onKeyDown}
        onReset={col.reset}
      />
    </>
  );
}
