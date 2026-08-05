import { lazy, useEffect, useRef, useState, type ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import type { Tab } from "./components/TabBar";
import { Sidebar } from "./components/Sidebar";
import { useQueryClient } from "@tanstack/react-query";
import { setUpstream, fetchRemote, pullRemote, pushRemote, undo, redo, checkoutBranch, initRepo, type IpcError } from "./ipc";
import { FolderIcon, SunIcon, MoonIcon, FetchIcon, PullIcon, PushIcon, SpinnerIcon, ChevronDownIcon, CheckIcon, UndoIcon, RedoIcon, HistoryIcon, SearchIcon, MoreIcon, DropletIcon, CloudIcon, PlusIcon, FileDiffIcon, BlameIcon, SubmoduleIcon, WorktreeIcon, BranchIcon, SettingsIcon } from "./components/icons";
import { BranchSwitcher } from "./components/BranchSwitcher";
import { SyncBadge } from "./components/SyncBadge";
import { StashMenu } from "./components/StashMenu";
import { CommandPalette } from "./components/CommandPalette";
import type { Command } from "./lib/commands";
import { useToast } from "./components/Toast";
import { Glass } from "./components/ui/Glass";
import { LaunchGraph } from "./components/LaunchGraph";
import { useRepoWatch, useCurrentBranch, useAheadBehind, useRemotes, useRemoteList, useUndoState, useBranches, useRefs, useSubmodules, useWorktrees, useSparseCheckout, invalidateHistory, invalidateWorktree, qk } from "./lib/queries";
import { applyTheme, applyGlassMode, getStoredTheme, type Theme } from "./lib/theme";
import { getStoredGlassPref, setStoredGlassPref } from "./lib/transparency";
import { useT, useLang, toggleLang, nextLangLabel } from "./lib/i18n";
import { GlobeIcon } from "./components/icons";
import { checkForAppUpdate } from "./lib/updater";
import { buildCreateChangeRequestUrl, buildFindChangeRequestUrl } from "./lib/hosting";
import { LazyBoundary } from "./components/LazyBoundary";
import { APP_SETTINGS_ENTRY_POINTS, settingsSectionForEntryPoint, type SettingsEntryPoint, type SettingsSection } from "./lib/settings";
import { ChangesView } from "./views/ChangesView";
import { HistoryView } from "./views/HistoryView";

const CompareView = lazy(() => import("./views/CompareView").then((m) => ({ default: m.CompareView })));
const BlameView = lazy(() => import("./views/BlameView").then((m) => ({ default: m.BlameView })));
const SubmodulesView = lazy(() => import("./views/SubmodulesView").then((m) => ({ default: m.SubmodulesView })));
const WorktreesView = lazy(() => import("./views/WorktreesView").then((m) => ({ default: m.WorktreesView })));
const SparseCheckoutView = lazy(() => import("./views/SparseCheckoutView").then((m) => ({ default: m.SparseCheckoutView })));
const RemoteManager = lazy(() => import("./components/RemoteManager").then((m) => ({ default: m.RemoteManager })));
const GithubCreatePrDialog = lazy(() => import("./components/GithubCreatePrDialog").then((m) => ({ default: m.GithubCreatePrDialog })));
const GitlabCreateMrDialog = lazy(() => import("./components/GitlabCreateMrDialog").then((m) => ({ default: m.GitlabCreateMrDialog })));
const SettingsPanel = lazy(() => import("./components/SettingsPanel").then((m) => ({ default: m.SettingsPanel })));
const GithubPrPanel = lazy(() => import("./components/GithubPrPanel").then((m) => ({ default: m.GithubPrPanel })));
const GitlabMrPanel = lazy(() => import("./components/GitlabMrPanel").then((m) => ({ default: m.GitlabMrPanel })));
const CloneDialog = lazy(() => import("./components/CloneDialog").then((m) => ({ default: m.CloneDialog })));
const OpLogPanel = lazy(() => import("./components/OpLogPanel").then((m) => ({ default: m.OpLogPanel })));

/** 把 git fetch 的原始摘要提炼成简洁细节:优先取 "->" 更新行。 */
function fetchDetail(summary: string): string | undefined {
  if (summary === "已是最新") return undefined;
  const lines = summary.split("\n").map((l) => l.trim()).filter(Boolean);
  const updates = lines.filter((l) => l.includes("->"));
  return (updates.length ? updates : lines.slice(0, 1)).join("\n") || undefined;
}

export default function App() {
  const [repo, setRepo] = useState<string | null>(null);
  // 上次打开的仓库:不自动跳入(保留启动屏作为每次开 app 的第一印象),
  // 而是在启动屏给一个「继续上次」快捷入口(Linear/Things 式「跳回上次」)。
  const lastRepo = localStorage.getItem("repo.last");
  const [tab, setTab] = useState<Tab>("changes");
  const [theme, setTheme] = useState<Theme>(getStoredTheme);
  const [fetching, setFetching] = useState(false);
  const [pulling, setPulling] = useState(false);
  const [pushing, setPushing] = useState(false);
  const [pullRebase, setPullRebase] = useState(() => localStorage.getItem("pull.rebase") === "1");
  const [sideCollapsed, setSideCollapsed] = useState(() => localStorage.getItem("sidebar.collapsed") === "1");
  const [pullMenu, setPullMenu] = useState(false);
  const [selectedRemote, setSelectedRemote] = useState<string | null>(null);
  const [remoteMenu, setRemoteMenu] = useState(false);
  const [upMenu, setUpMenu] = useState(false);
  const [undoing, setUndoing] = useState(false);
  const [opLogOpen, setOpLogOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [moreMenu, setMoreMenu] = useState(false);
  const [remoteMgrOpen, setRemoteMgrOpen] = useState(false);
  const [githubCreatePrOpen, setGithubCreatePrOpen] = useState(false);
  const [githubPrOpen, setGithubPrOpen] = useState(false);
  const [settingsSection, setSettingsSection] = useState<SettingsSection | null>(null);
  const [gitlabCreateMrOpen, setGitlabCreateMrOpen] = useState(false);
  const [gitlabMrOpen, setGitlabMrOpen] = useState(false);
  const [cloneOpen, setCloneOpen] = useState(false);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const moreMenuTriggerRef = useRef<HTMLButtonElement>(null);
  const toast = useToast();
  const qc = useQueryClient();
  const t = useT();
  const lang = useLang();

  function openSettingsFor(entryPoint: SettingsEntryPoint) {
    setSettingsSection(settingsSectionForEntryPoint(entryPoint));
  }

  // 顶层读统一走 query;一处监听文件变化 → 失效(各 view 据此自动重取)
  useRepoWatch(repo);
  const branch = useCurrentBranch(repo ?? "").data ?? null;
  const sync = useAheadBehind(repo ?? "").data ?? null;
  const remotes = useRemotes(repo ?? "").data ?? [];
  const remoteInfos = useRemoteList(repo ?? "", !!repo).data ?? [];
  const undoState = useUndoState(repo ?? "").data ?? null;
  const canUndo = undoState?.can_undo ?? null;
  const canRedo = undoState?.can_redo ?? null;
  const branches = useBranches(repo ?? "", !!repo).data ?? [];
  const refs = useRefs(repo ?? "", !!repo).data ?? [];
  const submodules = useSubmodules(repo ?? "").data ?? [];
  const hasSubmodules = submodules.length > 0;
  const worktrees = useWorktrees(repo ?? "").data ?? [];
  const hasWorktrees = worktrees.length > 1; // 只有主工作树时不显示标签
  const sparsePatterns = useSparseCheckout(repo ?? "").data ?? [];
  const hasSparse = sparsePatterns.length > 0; // 未开启稀疏检出时不显示标签

  const busy = fetching || pulling || pushing || undoing;
  // 同步提示:落后 → 建议 Pull;领先 → 建议 Push(无上游时 sync 为 null,不提示)
  const canPull = !!sync && sync.behind > 0;
  const canPush = !!sync && sync.ahead > 0;

  function toggleTheme() {
    const next: Theme = theme === "dark" ? "light" : "dark";
    applyTheme(next);
    setTheme(next);
  }

  async function doFetch() {
    if (!repo) return;
    setFetching(true);
    try {
      const r = await fetchRemote(repo, selectedRemote ?? undefined);
      // refs 变化会触发文件监听 → 各视图自动重载;这里只用 toast 反馈结果。
      toast({
        kind: "success",
        title: r.summary === "已是最新" ? t("toast.upToDate") : t("toast.fetched"),
        detail: fetchDetail(r.summary),
      });
    } catch (e) {
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setFetching(false);
      invalidateHistory(qc, repo); // 兜底:packed-refs 时 watcher 可能不触发
    }
  }

  function setPullMode(rebase: boolean) {
    setPullRebase(rebase);
    localStorage.setItem("pull.rebase", rebase ? "1" : "0");
  }

  async function doPull(rebase: boolean) {
    if (!repo) return;
    setPullMenu(false);
    setPulling(true);
    try {
      const r = await pullRemote(repo, rebase, selectedRemote ?? undefined);
      // 成功后工作区/HEAD 变化触发文件监听 → 图谱自动前进。
      const upToDate = /up to date|已是最新/i.test(r.summary);
      toast({ kind: "success", title: upToDate ? t("toast.upToDate") : rebase ? t("toast.pulledRebase") : t("toast.pulledMerge") });
    } catch (e) {
      // 冲突时工作区已留下冲突标记(rebase 会停在中途),可到「更改」页查看。
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setPulling(false);
      invalidateHistory(qc, repo);
    }
  }

  async function doPush() {
    if (!repo) return;
    setPushing(true);
    try {
      const r = await pushRemote(repo, selectedRemote ?? undefined);
      // push 成功后远程跟踪分支前进 → watcher(ref)刷新底栏/角标。
      const upToDate = /up-to-date|up to date|已是最新/i.test(r.summary);
      toast({
        kind: "success",
        title: upToDate ? t("toast.upToDate") : r.set_upstream ? t("toast.pushedUpstream") : t("toast.pushed"),
      });
    } catch (e) {
      // 落后远程时会抛 PUSH_REJECTED:提示用户先 Pull 再推。
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setPushing(false);
      invalidateHistory(qc, repo);
    }
  }

  async function doSetUpstream(upstream: string) {
    if (!repo) return;
    setUpMenu(false);
    try {
      await setUpstream(repo, upstream);
      toast({ kind: "success", title: t("toast.upstreamSet", { name: upstream }) });
      qc.invalidateQueries({ queryKey: qk.aheadBehind(repo) });
    } catch (e) {
      // 远程跟踪分支不存在时会失败——提示用户先 Fetch/Push。
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    }
  }

  // 撤销/重做共用:沿操作时间线移动 HEAD(reset --soft),成功后刷新历史 + 工作区。
  async function doNav(dir: "undo" | "redo") {
    if (!repo) return;
    setUndoing(true);
    try {
      const info = await (dir === "undo" ? undo(repo) : redo(repo));
      // soft(撤销提交)内容回暂存区;hard(撤销 reset 等)已忠实还原工作区。
      const effect = info.worktree_restored ? t("toast.effectRestored") : t("toast.effectToStage");
      toast({
        kind: "success",
        title: dir === "undo" ? t("toast.undone", { label: info.label }) : t("toast.redone", { label: info.label }),
        detail: dir === "undo" ? t("toast.undoDetail", { short: info.target_short, effect }) : t("toast.redoDetail", { short: info.target_short, effect }),
      });
    } catch (e) {
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setUndoing(false);
      // 动了 HEAD + 暂存区:历史(含可否再撤销/重做)与工作区都要刷新。
      invalidateHistory(qc, repo);
      invalidateWorktree(qc, repo);
    }
  }

  // 命令面板「跳转到分支」用:切换分支 + 反馈 + 失效。
  async function doCheckout(name: string) {
    if (!repo) return;
    try {
      await checkoutBranch(repo, name);
      toast({ kind: "success", title: t("toast.checkedOut", { name }) });
      qc.invalidateQueries({ queryKey: ["branches", repo] });
      invalidateHistory(qc, repo);
      invalidateWorktree(qc, repo);
    } catch (e) {
      // 脏工作区切换失败等 → 提示
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    }
  }

  async function pickRepo() {
    const dir = await open({ directory: true, title: t("dialog.pickRepo") });
    if (typeof dir === "string") setRepo(dir);
  }

  // 新建:选一个文件夹 → git init → 打开它。
  async function doInit() {
    const dir = await open({ directory: true, title: t("dialog.initFolder") });
    if (typeof dir !== "string") return;
    try {
      await initRepo(dir);
      toast({ kind: "success", title: t("toast.repoInit"), detail: dir });
      setRepo(dir);
    } catch (e) {
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    }
  }

  // 切仓库时重置远程选择(回到默认)
  async function doCheckUpdate() {
    if (checkingUpdate) return;
    setCheckingUpdate(true);
    try {
      await checkForAppUpdate({ check, relaunch, toast });
    } finally {
      setCheckingUpdate(false);
    }
  }

  async function openCreateChangeRequest() {
    const link = buildCreateChangeRequestUrl(remoteInfos, branch, selectedRemote);
    if (!link) {
      toast({
        kind: "error",
        title: "无法打开 PR/MR 页面",
        detail: branch ? "当前仓库没有可识别的 GitHub/GitLab/Bitbucket 远程地址" : "当前仓库还没有本地分支",
      });
      return;
    }

    try {
      await openUrl(link.url);
      toast({ kind: "success", title: `已打开 ${link.provider} 创建页面`, detail: `远程: ${link.remoteName}` });
    } catch (e) {
      toast({ kind: "error", title: (e as Error).message ?? String(e) });
    }
  }

  async function openExistingChangeRequests() {
    const link = buildFindChangeRequestUrl(remoteInfos, branch, selectedRemote);
    if (!link) {
      toast({
        kind: "error",
        title: "无法查找已有 PR",
        detail: branch ? "当前版本只支持 GitHub 远程仓库" : "当前仓库还没有本地分支",
      });
      return;
    }

    try {
      await openUrl(link.url);
      toast({ kind: "success", title: "已打开 GitHub PR 搜索", detail: `远程: ${link.remoteName}` });
    } catch (e) {
      toast({ kind: "error", title: (e as Error).message ?? String(e) });
    }
  }

  useEffect(() => { setSelectedRemote(null); }, [repo]);

  // 记住当前仓库,供下次启动屏「继续上次」用。
  useEffect(() => { if (repo) localStorage.setItem("repo.last", repo); }, [repo]);

  // 动态标签(子模块/工作树)随仓库出现/消失;切到不再可用的标签时退回「更改」,免得停在空标签。
  useEffect(() => {
    if ((tab === "submodules" && !hasSubmodules) || (tab === "worktrees" && !hasWorktrees) || (tab === "sparse" && !hasSparse)) setTab("changes");
  }, [tab, hasSubmodules, hasWorktrees, hasSparse]);

  // 全局 ⌘K / Ctrl+K 开关命令面板(M3 键盘优先入口)
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && (e.key === "k" || e.key === "K")) {
        e.preventDefault();
        setPaletteOpen((o) => !o);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const isMac = navigator.platform.toLowerCase().includes("mac");
  const modLabel = isMac ? "⌘K" : "Ctrl K";

  // 命令面板的命令清单。每次渲染重建,保证 run() 闭包拿到最新 state(如 selectedRemote),
  // 避免 useMemo 漏依赖导致的陈旧闭包;十来条命令,重建成本可忽略。
  const commands: Command[] = [];
  const views: { id: Tab; label: string }[] = [
    { id: "changes", label: t("nav.changes") },
    { id: "history", label: t("nav.history") },
    { id: "compare", label: t("nav.compare") },
    { id: "blame", label: t("nav.blame") },
  ];
  if (hasSubmodules) views.push({ id: "submodules", label: t("nav.submodules") });
  if (hasWorktrees) views.push({ id: "worktrees", label: t("nav.worktrees") });
  if (hasSparse) views.push({ id: "sparse", label: t("nav.sparse") });
  const VIEW_ICON: Record<string, ReactNode> = {
    changes: <FileDiffIcon width={15} height={15} />,
    history: <HistoryIcon width={15} height={15} />,
    compare: <FileDiffIcon width={15} height={15} />,
    blame: <BlameIcon width={15} height={15} />,
    submodules: <SubmoduleIcon width={15} height={15} />,
    worktrees: <WorktreeIcon width={15} height={15} />,
    sparse: <FolderIcon width={15} height={15} />,
  };
  for (const v of views) {
    commands.push({
      id: `view:${v.id}`,
      icon: VIEW_ICON[v.id],
      title: t("cmd.goToView", { name: v.label }),
      group: t("group.views"),
      keywords: `view tab ${v.id} ${v.label}`,
      disabled: !repo || tab === v.id,
      run: () => setTab(v.id),
    });
  }
  commands.push({
    id: "lang:toggle",
    icon: <GlobeIcon width={15} height={15} />,
    title: t("cmd.switchLang"),
    subtitle: t("cmd.switchLang.sub"),
    group: t("group.appearance"),
    keywords: "language lang 语言 中文 english 切换",
    run: toggleLang,
  });
  commands.push({
    id: "app:settings",
    icon: <SettingsIcon width={15} height={15} />,
    title: t("cmd.settings"),
    subtitle: t("cmd.settings.sub"),
    group: t("group.panel"),
    keywords: "settings preferences credentials deepseek github gitlab",
    run: () => setSettingsSection("deepseek"),
  });
  commands.push({
    id: "jump:branch",
    icon: <BranchIcon width={15} height={15} />,
    title: t("cmd.jumpBranch"),
    subtitle: t("cmd.jumpBranch.sub"),
    group: t("group.jump"),
    keywords: "checkout switch branch 切换 分支 跳转",
    disabled: !repo || busy || branches.length === 0,
    run: () => {},
    jump: {
      placeholder: t("cmd.jumpBranch"),
      items: branches.map((b) => ({
        id: b.name,
        label: b.name,
        hint: b.is_head ? t("cmd.jumpBranch.current") : undefined,
        run: () => {
          if (!b.is_head) doCheckout(b.name);
        },
      })),
    },
  });
  commands.push({
    id: "repo:pick",
    icon: <FolderIcon width={15} height={15} />,
    title: repo ? t("cmd.switchRepo") : t("cmd.pickRepo"),
    group: t("group.repo"),
    keywords: "open repo folder 打开 仓库 切换",
    run: pickRepo,
  });
  commands.push({
    id: "repo:clone",
    icon: <CloudIcon width={15} height={15} />,
    title: t("cmd.clone"),
    subtitle: t("cmd.clone.sub"),
    group: t("group.repo"),
    keywords: "clone 克隆 远程 url git",
    run: () => setCloneOpen(true),
  });
  commands.push({
    id: "repo:init",
    icon: <PlusIcon width={15} height={15} />,
    title: t("cmd.init"),
    subtitle: t("cmd.init.sub"),
    group: t("group.repo"),
    keywords: "init 新建 初始化 仓库 create",
    run: doInit,
  });
  commands.push({
    id: "theme:toggle",
    icon: theme === "dark" ? <SunIcon width={15} height={15} /> : <MoonIcon width={15} height={15} />,
    title: theme === "dark" ? t("action.toLight") : t("action.toDark"),
    group: t("group.appearance"),
    keywords: "theme dark light 主题 暗色 浅色 切换",
    run: toggleTheme,
  });
  commands.push({
    id: "glass:toggle",
    icon: <DropletIcon width={15} height={15} />,
    title: getStoredGlassPref() === "reduced" ? t("cmd.glassOn") : t("cmd.glassReduce"),
    group: t("group.appearance"),
    keywords: "glass transparency 玻璃 透明 实底 无障碍",
    run: () => {
      const next = getStoredGlassPref() === "reduced" ? "auto" : "reduced";
      setStoredGlassPref(next);
      applyGlassMode();
    },
  });
  commands.push({
    id: "app:update",
    icon: <CloudIcon width={15} height={15} />,
    title: checkingUpdate ? "正在检查更新..." : "检查更新",
    subtitle: "下载并安装新版本",
    group: "应用",
    keywords: "update updater upgrade release 更新 升级 版本",
    disabled: checkingUpdate,
    run: doCheckUpdate,
  });
  commands.push(
    { id: "remote:fetch", icon: <FetchIcon width={15} height={15} />, title: t("cmd.fetch"), subtitle: t("cmd.fetch.sub"), group: t("group.remote"), keywords: "拉取 远程 fetch", disabled: !repo || busy, run: doFetch },
    { id: "remote:pull-merge", icon: <PullIcon width={15} height={15} />, title: t("cmd.pullMerge"), group: t("group.remote"), keywords: "拉取 合并 merge pull", disabled: !repo || busy, run: () => doPull(false) },
    { id: "remote:pull-rebase", icon: <PullIcon width={15} height={15} />, title: t("cmd.pullRebase"), group: t("group.remote"), keywords: "拉取 变基 rebase pull", disabled: !repo || busy, run: () => doPull(true) },
    { id: "remote:push", icon: <PushIcon width={15} height={15} />, title: t("cmd.push"), subtitle: t("cmd.push.sub"), group: t("group.remote"), keywords: "推送 push", disabled: !repo || busy, run: doPush },
    { id: "remote:manage", icon: <CloudIcon width={15} height={15} />, title: t("cmd.manageRemote"), subtitle: t("cmd.manageRemote.sub"), group: t("group.remote"), keywords: "远程 remote 管理 添加 删除 重命名 add remove rename", disabled: !repo, run: () => setRemoteMgrOpen(true) },
  );
  commands.push(
    { id: "undo", icon: <UndoIcon width={15} height={15} />, title: canUndo ? t("cmd.undoLabel", { label: canUndo.label }) : t("cmd.undo"), group: t("group.undo"), keywords: "undo 撤销 回退", disabled: !repo || busy || !canUndo, run: () => doNav("undo") },
    { id: "redo", icon: <RedoIcon width={15} height={15} />, title: canRedo ? t("cmd.redoLabel", { label: canRedo.label }) : t("cmd.redo"), group: t("group.undo"), keywords: "redo 重做 前进", disabled: !repo || busy || !canRedo, run: () => doNav("redo") },
  );
  commands.push(
    { id: "remote:create-pr", icon: <PlusIcon width={15} height={15} />, title: "打开 PR/MR 页面", subtitle: "基于当前分支创建 Pull Request / Merge Request", group: "协作", keywords: "pull request merge request pr mr github gitlab bitbucket 协作 评审 合并请求", disabled: !repo || !branch || busy, run: openCreateChangeRequest },
    { id: "remote:find-pr", icon: <SearchIcon width={15} height={15} />, title: "查找当前分支 PR", subtitle: "在 GitHub 打开当前分支的 open PR 搜索", group: "协作", keywords: "find pull request existing pr github current branch 查找 已有 当前分支", disabled: !repo || !branch || busy, run: openExistingChangeRequests },
    { id: "github:list-prs", icon: <CloudIcon width={15} height={15} />, title: "查看当前分支 GitHub PR", subtitle: "在客户端内查看 PR、review 和状态检查", group: "协作", keywords: "github pull request pr review status checks api 查看 评审", disabled: !repo || !branch || busy, run: () => setGithubPrOpen(true) },
    { id: "github:create-pr", icon: <PlusIcon width={15} height={15} />, title: "创建 GitHub PR", subtitle: "在客户端内通过 GitHub API 创建 Pull Request", group: "协作", keywords: "github api create pull request pr new draft token 创建 新建", disabled: !repo || !branch || busy, run: () => setGithubCreatePrOpen(true) },
    { id: "github:token", icon: <SettingsIcon width={15} height={15} />, title: t("cmd.settings.github"), subtitle: t("cmd.settings.github.sub"), group: t("group.panel"), keywords: "github token pat auth credential", run: () => openSettingsFor(APP_SETTINGS_ENTRY_POINTS.githubCommand) },
    { id: "gitlab:list-mrs", icon: <CloudIcon width={15} height={15} />, title: "查看当前分支 GitLab MR", subtitle: "在客户端内查看 open Merge Request", group: "协作", keywords: "gitlab merge request mr api 查看 合并请求", disabled: !repo || !branch || busy, run: () => setGitlabMrOpen(true) },
    { id: "gitlab:create-mr", icon: <PlusIcon width={15} height={15} />, title: "创建 GitLab MR", subtitle: "在客户端内通过 GitLab API 创建 Merge Request", group: "协作", keywords: "gitlab api create merge request mr new draft token 创建 新建", disabled: !repo || !branch || busy, run: () => setGitlabCreateMrOpen(true) },
    { id: "gitlab:token", icon: <SettingsIcon width={15} height={15} />, title: t("cmd.settings.gitlab"), subtitle: t("cmd.settings.gitlab.sub"), group: t("group.panel"), keywords: "gitlab token pat auth credential", run: () => openSettingsFor(APP_SETTINGS_ENTRY_POINTS.gitlabCommand) },
  );
  commands.push({
    id: "panel:oplog",
    icon: <HistoryIcon width={15} height={15} />,
    title: t("cmd.opLog"),
    subtitle: t("cmd.opLog.sub"),
    group: t("group.appearance"), // 对齐原型:操作日志归「外观」组(与主题/语言/玻璃同组)
    keywords: "operation log history 操作 日志 时间线",
    disabled: !repo,
    run: () => setOpLogOpen(true),
  });

  // 仓库路径只显示尾部目录名,完整路径放 title 悬浮
  const repoName = repo?.replace(/[/\\]+$/, "").split(/[/\\]/).pop() ?? null;

  return (
    <div className={`flex h-screen flex-col text-fg ${repo ? "bg-canvas" : "launch-ambient"}`}>
      {busy && <TopProgress />}
      {/* 顶栏:轻、紧凑、左标题右仓库 */}
      <Glass as="header" className="relative z-20 flex h-11 shrink-0 items-center gap-3 border-b border-line px-3">
        <div className="flex min-w-0 items-center gap-2">
          <BranchMark onHome={repo ? () => setRepo(null) : undefined} title={t("header.home")} />
          {repo ? (
            <span className="max-w-[12rem] truncate font-mono text-sm font-medium text-fg" title={repo}>{repoName}</span>
          ) : (
            <span className="text-sm font-semibold">{t("header.appName")}</span>
          )}
          {repo && (
            <div className="flex min-w-0 items-center gap-2 text-xs">
              <span className="text-fg-subtle">/</span>
              <BranchSwitcher repo={repo} branch={branch} direction="down" />
              <SyncBadge sync={sync} />
            </div>
          )}
        </div>

        <div className="ml-auto flex items-center gap-2">
          {repo && (
            <div className="flex items-center gap-1.5">
              {remotes.length > 1 && (
                <div className="relative">
                  <button
                    onClick={() => setRemoteMenu((o) => !o)}
                    disabled={busy}
                    title={t("header.selectRemote")}
                    className="flex items-center gap-1 rounded-md border border-line-strong bg-elevated px-2 py-1 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50"
                  >
                    <span className="max-w-[8rem] truncate font-mono">{selectedRemote ?? remotes[0]}</span>
                    <ChevronDownIcon width={11} height={11} />
                  </button>
                  {remoteMenu && (
                    <>
                      <div className="fixed inset-0 z-40" onClick={() => setRemoteMenu(false)} />
                      <div className="absolute right-0 top-full z-50 mt-1 w-44 overflow-hidden rounded-md border border-line-strong bg-elevated text-xs menu-in popover">
                        {remotes.map((r) => (
                          <button
                            key={r}
                            onClick={() => { setSelectedRemote(r); setRemoteMenu(false); }}
                            className="flex w-full items-center gap-2 px-2.5 py-1.5 text-left text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
                          >
                            <span className="grid w-3.5 shrink-0 place-items-center text-accent">
                              {(selectedRemote ?? remotes[0]) === r ? <CheckIcon width={12} height={12} /> : null}
                            </span>
                            <span className="truncate font-mono">{r}</span>
                          </button>
                        ))}
                      </div>
                    </>
                  )}
                </div>
              )}
              {/* 撤销/重做:成对收进中性托盘(↩↪ 一眼是「撤销/重做」而非「返回」;不用 accent
                  免被当成主导航键)。任一可用即显示,另一侧不可用时变暗;标签在 title/aria-label。
                  进行中状态由顶部 TopProgress(busy)统一信号,这里不放内联 spinner。 */}
              {(canUndo || canRedo) && (
                <div className="flex items-center gap-0.5 rounded-lg border border-line bg-elevated/60 p-0.5">
                  <button
                    onClick={() => doNav("undo")}
                    disabled={busy || !canUndo}
                    aria-label={canUndo ? t("undo.aria", { label: canUndo.label }) : t("action.undo")}
                    title={canUndo ? t("undo.title", { label: canUndo.label, short: canUndo.target_short, effect: canUndo.worktree_restored ? t("undo.effectRestore") : t("undo.effectStage") }) : t("undo.none")}
                    className="grid h-7 w-7 place-items-center rounded-md text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-35 disabled:hover:bg-transparent disabled:hover:text-fg-muted"
                  >
                    <UndoIcon width={14} height={14} />
                  </button>
                  <button
                    onClick={() => doNav("redo")}
                    disabled={busy || !canRedo}
                    aria-label={canRedo ? t("redo.aria", { label: canRedo.label }) : t("action.redo")}
                    title={canRedo ? t("redo.title", { label: canRedo.label, short: canRedo.target_short }) : t("redo.none")}
                    className="grid h-7 w-7 place-items-center rounded-md text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-35 disabled:hover:bg-transparent disabled:hover:text-fg-muted"
                  >
                    <RedoIcon width={14} height={14} />
                  </button>
                </div>
              )}
              {/* 同步操作托盘:Fetch · Pull · Push 收成一组,读作一个单元(双层贝塞尔托盘)。
                  各按钮无独立边框,托盘承载边框;可 Pull/Push 时用强调色文字 + ↓N/↑N 角标提示。 */}
              <div className="flex items-center gap-0.5 rounded-lg border border-line bg-elevated/60 p-0.5">
                <button
                  onClick={doFetch}
                  disabled={busy}
                  title={t("tray.fetchTitle")}
                  className="flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs text-fg transition-colors hover:bg-overlay disabled:opacity-50"
                >
                  {fetching ? (
                    <SpinnerIcon width={13} height={13} />
                  ) : (
                    <FetchIcon width={13} height={13} />
                  )}
                  {fetching ? "Fetch…" : "Fetch"}
                </button>
                <div className="relative flex items-stretch">
                  <button
                    onClick={() => doPull(pullRebase)}
                    disabled={busy}
                    title={canPull ? t("tray.pullBehind", { n: sync!.behind }) : pullRebase ? t("tray.pullRebaseTitle") : t("tray.pullMergeTitle")}
                    className={`flex items-center gap-1.5 rounded-l-md px-2.5 py-1 text-xs transition-colors hover:bg-overlay disabled:opacity-50 ${
                      canPull ? "text-accent" : "text-fg"
                    }`}
                  >
                    {pulling ? <SpinnerIcon width={13} height={13} /> : <PullIcon width={13} height={13} />}
                    {pulling ? "Pull…" : pullRebase ? t("cmd.pullRebase") : "Pull"}
                    {canPull && !pulling && (
                      <span className="rounded-full bg-accent/15 px-1 font-mono text-[10px] font-semibold text-accent">
                        ↓{sync!.behind}
                      </span>
                    )}
                  </button>
                  <button
                    onClick={() => setPullMenu((o) => !o)}
                    disabled={busy}
                    title={t("pull.selectMode")}
                    className="grid place-items-center rounded-r-md px-1 text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50"
                  >
                    <ChevronDownIcon width={11} height={11} />
                  </button>
                  {pullMenu && (
                    <>
                      <div className="fixed inset-0 z-40" onClick={() => setPullMenu(false)} />
                      <div className="absolute right-0 top-full z-50 mt-1.5 w-44 overflow-hidden rounded-md border border-line-strong bg-elevated text-xs menu-in popover">
                        <PullModeItem active={!pullRebase} onClick={() => { setPullMode(false); doPull(false); }}>
                          {t("pull.merge")}
                        </PullModeItem>
                        <PullModeItem active={pullRebase} onClick={() => { setPullMode(true); doPull(true); }}>
                          {t("pull.rebase")}
                        </PullModeItem>
                      </div>
                    </>
                  )}
                </div>
                <button
                  onClick={doPush}
                  disabled={busy}
                  title={canPush ? t("tray.pushAhead", { n: sync!.ahead }) : t("tray.pushTitle")}
                  className={`flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs transition-colors hover:bg-overlay disabled:opacity-50 ${
                    canPush ? "text-success" : "text-fg"
                  }`}
                >
                  {pushing ? (
                    <SpinnerIcon width={13} height={13} />
                  ) : (
                    <PushIcon width={13} height={13} />
                  )}
                  {pushing ? "Push…" : "Push"}
                  {canPush && !pushing && (
                    <span className="rounded-full bg-success/15 px-1 font-mono text-[10px] font-semibold text-success">
                      ↑{sync!.ahead}
                    </span>
                  )}
                </button>
              </div>
              <StashMenu repo={repo} />
            </div>
          )}
          <button
            onClick={() => setPaletteOpen(true)}
            title={t("action.commandPalette")}
            aria-label={t("action.commandPalette")}
            data-testid="command-palette"
            className="hidden items-center gap-1.5 rounded-md border border-line-strong bg-elevated px-2 py-1 text-fg-subtle transition-colors hover:bg-overlay hover:text-fg hover:border-fg-subtle sm:flex"
          >
            <SearchIcon width={13} height={13} />
            <kbd className="font-mono text-[10px]">{modLabel}</kbd>
          </button>

          {/* 语言切换:中 / English(主题键旁,与命令面板「切换语言」同源) */}
          <button
            onClick={toggleLang}
            title={t("action.langTitle")}
            aria-label={t("action.langTitle")}
            className="grid h-7 min-w-7 place-items-center rounded-md border border-line-strong bg-elevated px-1.5 font-mono text-[11px] font-semibold text-fg-muted transition-colors hover:bg-overlay hover:text-fg hover:border-fg-subtle"
          >
            {nextLangLabel(lang)}
          </button>

          {/* 溢出菜单:次要外观/会话动作(操作日志 / 主题 / 玻璃)收纳于此,给顶栏减负。 */}
          <div className="relative">
            <button
              ref={moreMenuTriggerRef}
              onClick={() => setMoreMenu((o) => !o)}
              title={t("action.more")}
              aria-label={t("action.more")}
              className="grid h-7 w-7 place-items-center rounded-md border border-line-strong bg-elevated text-fg-muted transition-colors hover:bg-overlay hover:text-fg hover:border-fg-subtle"
            >
              <MoreIcon width={15} height={15} />
            </button>
            {moreMenu && (
              <>
                <div className="fixed inset-0 z-40" onClick={() => setMoreMenu(false)} />
                <div className="absolute right-0 top-full z-50 mt-1.5 w-48 overflow-hidden rounded-md border border-line-strong bg-elevated text-xs menu-in popover">
                  {repo && (
                    <MoreItem icon={<HistoryIcon width={14} height={14} />} onClick={() => { setMoreMenu(false); setOpLogOpen(true); }}>
                      {t("action.opLog")}
                    </MoreItem>
                  )}
                  {repo && (
                    <MoreItem icon={<CloudIcon width={14} height={14} />} onClick={() => { setMoreMenu(false); setRemoteMgrOpen(true); }}>
                      {t("action.manageRemote")}
                    </MoreItem>
                  )}
                  <MoreItem icon={<SettingsIcon width={14} height={14} />} onClick={() => { setMoreMenu(false); openSettingsFor("moreMenu"); }}>
                    {t("action.settings")}
                  </MoreItem>
                  <MoreItem
                    icon={theme === "dark" ? <SunIcon width={14} height={14} /> : <MoonIcon width={14} height={14} />}
                    onClick={() => { toggleTheme(); setMoreMenu(false); }}
                  >
                    {theme === "dark" ? t("action.toLight") : t("action.toDark")}
                  </MoreItem>
                  <MoreItem
                    icon={<DropletIcon width={14} height={14} />}
                    onClick={() => {
                      const next = getStoredGlassPref() === "reduced" ? "auto" : "reduced";
                      setStoredGlassPref(next);
                      applyGlassMode();
                      setMoreMenu(false);
                    }}
                  >
                    {getStoredGlassPref() === "reduced" ? t("action.glassOn") : t("action.glassReduce")}
                  </MoreItem>
                </div>
              </>
            )}
          </div>

          <button
            onClick={pickRepo}
            title={repo ? t("action.switchRepo") : t("action.pickRepo")}
            aria-label={repo ? t("action.switchRepo") : t("action.pickRepo")}
            data-testid="pick-repo"
            className="flex items-center gap-1.5 rounded-md border border-line-strong bg-elevated px-2.5 py-1 text-xs text-fg transition-colors hover:bg-overlay hover:border-fg-subtle"
          >
            <FolderIcon width={14} height={14} />
            {/* 窄屏(< lg)只留图标,免顶栏溢出;宽屏带文字 */}
            <span className="hidden lg:inline">{repo ? t("action.switchRepo") : t("action.pickRepo")}</span>
          </button>
        </div>
      </Glass>

      {/* 主体 */}
      {repo ? (
        <div className="flex min-h-0 flex-1" data-testid="repo-shell">
          <Sidebar
            active={tab}
            onChange={setTab}
            collapsed={sideCollapsed}
            onToggleCollapse={() => { const n = !sideCollapsed; setSideCollapsed(n); localStorage.setItem("sidebar.collapsed", n ? "1" : "0"); }}
            hasSubmodules={hasSubmodules}
            hasWorktrees={hasWorktrees}
            hasSparse={hasSparse}
          />
          <div className="min-h-0 min-w-0 flex-1">
            <LazyBoundary
              loading={<LazyFallback />}
              message={t("common.lazyLoadFailed")}
              retryLabel={t("common.reload")}
            >
              {tab === "changes" ? <ChangesView repo={repo} /> : tab === "history" ? <HistoryView repo={repo} /> : tab === "compare" ? <CompareView repo={repo} /> : tab === "submodules" ? <SubmodulesView repo={repo} /> : tab === "worktrees" ? <WorktreesView repo={repo} /> : tab === "sparse" ? <SparseCheckoutView repo={repo} /> : <BlameView repo={repo} />}
            </LazyBoundary>
          </div>
        </div>
      ) : (
        <EmptyState onPick={pickRepo} onClone={() => setCloneOpen(true)} onInit={doInit} lastRepo={lastRepo} onResume={setRepo} />
      )}

      {/* 底部状态栏:分支 + 仓库路径,IDE 风格 */}
      {repo && (
        <footer className="flex h-6 shrink-0 items-center gap-3 border-t border-line bg-elevated px-3 text-[11px] text-fg-muted">
          {!sync && branch && remotes.length > 0 && (
            <div className="relative">
              <button
                onClick={() => setUpMenu((o) => !o)}
                title={t("action.setUpstreamTitle")}
                className="rounded px-1 text-fg-subtle transition-colors hover:bg-overlay hover:text-fg"
              >
                {t("action.setUpstream")}
              </button>
              {upMenu && (
                <>
                  <div className="fixed inset-0 z-40" onClick={() => setUpMenu(false)} />
                  <div className="absolute bottom-full left-0 z-50 mb-1 w-48 overflow-hidden rounded-md border border-line-strong bg-elevated text-xs menu-in popover">
                    <div className="border-b border-line px-2.5 py-1 text-[10px] uppercase tracking-wide text-fg-subtle">{t("action.setAsUpstream")}</div>
                    {remotes.map((r) => (
                      <button
                        key={r}
                        onClick={() => doSetUpstream(`${r}/${branch}`)}
                        className="block w-full truncate px-2.5 py-1.5 text-left font-mono text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
                      >
                        {r}/{branch}
                      </button>
                    ))}
                  </div>
                </>
              )}
            </div>
          )}
          <span className="ml-auto truncate font-mono text-fg-subtle" title={repo}>
            {repo}
          </span>
        </footer>
      )}

      {paletteOpen && <CommandPalette commands={commands} onClose={() => setPaletteOpen(false)} />}

      <LazyBoundary
        loading={<LazyFallback overlay />}
        message={t("common.lazyLoadFailed")}
        retryLabel={t("common.reload")}
      >
      {repo && opLogOpen && (
        <OpLogPanel
          repo={repo}
          onClose={() => setOpLogOpen(false)}
          onJumped={() => {
            invalidateHistory(qc, repo);
            invalidateWorktree(qc, repo);
          }}
        />
      )}

      {repo && remoteMgrOpen && (
        <RemoteManager repo={repo} onClose={() => setRemoteMgrOpen(false)} />
      )}

      {settingsSection && (
        <SettingsPanel
          initialSection={settingsSection}
          returnFocusRef={moreMenuTriggerRef}
          onClose={() => setSettingsSection(null)}
        />
      )}

      {githubCreatePrOpen && (
        <GithubCreatePrDialog
          remotes={remoteInfos}
          branch={branch}
          preferredRemote={selectedRemote}
          branches={branches}
          refs={refs}
          onClose={() => setGithubCreatePrOpen(false)}
          onCreated={() => setGithubPrOpen(true)}
          onConfigureToken={() => {
            setGithubCreatePrOpen(false);
            openSettingsFor(APP_SETTINGS_ENTRY_POINTS.githubCreatePrDialog);
          }}
        />
      )}

      {gitlabCreateMrOpen && (
        <GitlabCreateMrDialog
          remotes={remoteInfos}
          branch={branch}
          preferredRemote={selectedRemote}
          branches={branches}
          refs={refs}
          onClose={() => setGitlabCreateMrOpen(false)}
          onCreated={() => setGitlabMrOpen(true)}
          onConfigureToken={() => {
            setGitlabCreateMrOpen(false);
            openSettingsFor(APP_SETTINGS_ENTRY_POINTS.gitlabCreateMrDialog);
          }}
        />
      )}

      {githubPrOpen && (
        <GithubPrPanel
          remotes={remoteInfos}
          branch={branch}
          preferredRemote={selectedRemote}
          onClose={() => setGithubPrOpen(false)}
          onConfigureToken={() => {
            setGithubPrOpen(false);
            openSettingsFor(APP_SETTINGS_ENTRY_POINTS.githubPrPanel);
          }}
          onConfigureCredential={(kind) => {
            setGithubPrOpen(false);
            setSettingsSection(kind);
          }}
        />
      )}

      {gitlabMrOpen && (
        <GitlabMrPanel
          remotes={remoteInfos}
          branch={branch}
          preferredRemote={selectedRemote}
          onClose={() => setGitlabMrOpen(false)}
          onConfigureToken={() => {
            setGitlabMrOpen(false);
            openSettingsFor(APP_SETTINGS_ENTRY_POINTS.gitlabMrPanel);
          }}
        />
      )}

      {cloneOpen && (
        <CloneDialog
          onClose={() => setCloneOpen(false)}
          onCloned={(path) => { setCloneOpen(false); setRepo(path); }}
        />
      )}
      </LazyBoundary>
    </div>
  );
}

function LazyFallback({ overlay = false }: { overlay?: boolean }) {
  return (
    <div
      data-testid="lazy-loading"
      className={overlay
        ? "fixed inset-0 z-50 grid place-items-center bg-black/20 text-fg-muted"
        : "grid h-full min-h-24 place-items-center text-fg-muted"}
    >
      <SpinnerIcon width={16} height={16} />
    </div>
  );
}

/** Pull 方式菜单项:左侧打勾表示当前模式 */
function PullModeItem({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      onClick={onClick}
      className="flex w-full items-center gap-2 px-2.5 py-1.5 text-left text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
    >
      <span className="grid w-3.5 shrink-0 place-items-center text-accent">
        {active ? <CheckIcon width={12} height={12} /> : null}
      </span>
      {children}
    </button>
  );
}

/** 溢出菜单项:左侧图标 + 文字,统一观感 */
function MoreItem({ icon, onClick, children }: { icon?: React.ReactNode; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      onClick={onClick}
      className="flex w-full items-center gap-2.5 px-2.5 py-1.5 text-left text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
    >
      {icon && <span className="grid w-4 shrink-0 place-items-center text-fg-subtle">{icon}</span>}
      <span className="flex-1">{children}</span>
    </button>
  );
}

/** 顶部不确定态进度条:非阻塞的全局加载信号(fetch 等后台操作进行时显示) */
function TopProgress() {
  return (
    <div className="pointer-events-none fixed inset-x-0 top-0 z-[70] h-0.5 overflow-hidden bg-accent/15">
      <div className="progress-bar h-full w-1/3 bg-accent" />
    </div>
  );
}

/** 顶栏左侧的小标记,纯装饰 */
/** 主页标:朱红 git-graph 小标。给了 onHome(有仓库时)即可点击回启动屏;否则纯装饰。 */
function BranchMark({ onHome, title }: { onHome?: () => void; title?: string }) {
  const mark = (
    <svg width={13} height={13} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round">
      <circle cx="4" cy="3.5" r="1.5" />
      <circle cx="4" cy="12.5" r="1.5" />
      <circle cx="12" cy="3.5" r="1.5" />
      <path d="M4 5v6M12 5v1a3 3 0 0 1-3 3H4" />
    </svg>
  );
  if (!onHome) {
    return <span className="grid h-5 w-5 place-items-center rounded bg-accent/15 text-accent">{mark}</span>;
  }
  return (
    <button
      onClick={onHome}
      title={title}
      aria-label={title}
      className="grid h-5 w-5 place-items-center rounded bg-accent/15 text-accent transition-colors hover:bg-accent/25"
    >
      {mark}
    </button>
  );
}

/** 没选仓库时的启动屏 —— 编辑性封面(Paper & Ink)。
 *  纸面左对齐单栏,Instrument Serif 巨字「Strata」+ 斜体朱红副题,右侧缓行书脊图谱,
 *  逐元素电影级入场(hero-rise stagger),右上角语言切换。 */
function EmptyState({ onPick, onClone, onInit, lastRepo, onResume }: { onPick: () => void; onClone: () => void; onInit: () => void; lastRepo: string | null; onResume: (r: string) => void }) {
  const t = useT();
  const lang = useLang();
  const lastName = lastRepo?.replace(/[/\\]+$/, "").split(/[/\\]/).pop() ?? null;
  const isMac = navigator.platform.toLowerCase().includes("mac");
  return (
    <div className="relative flex flex-1 items-center justify-start overflow-hidden px-[clamp(40px,8vw,128px)]">
      {/* 签名背景:极淡缓行的「活的图谱」(书脊母题),垫在最底,呼应产品本体 */}
      <LaunchGraph />
      {/* 右侧竖直渐隐分隔线,把图谱栏与正文轻轻分开 */}
      <div className="pointer-events-none absolute inset-y-0 right-[calc(6%+150px)] w-px bg-gradient-to-b from-line to-transparent" />
      {/* 细噪点叠层:给纸面一层物理颗粒感 */}
      <div className="grain-overlay" />

      {/* 右上角语言切换 */}
      <button
        onClick={toggleLang}
        title={t("action.langTitle")}
        className="absolute right-8 top-7 z-20 flex items-center gap-2 rounded-full border border-line-strong bg-elevated/50 px-3.5 py-1.5 font-mono text-xs font-semibold text-fg-muted transition-colors hover:bg-elevated hover:text-fg"
      >
        <GlobeIcon width={14} height={14} />
        {nextLangLabel(lang)}
      </button>

      <div className="relative z-10 w-full max-w-[560px]">
        {/* 眉签行:玻璃方牌(朱红 git-graph 标记)+ 朱红短横 + mono 眉签 */}
        <div className="hero-rise flex items-center gap-3.5" style={{ animationDelay: "0ms" }}>
          <Glass className="grid h-11 w-11 place-items-center rounded-[13px] text-accent">
            <svg width={22} height={22} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.5} strokeLinecap="round" strokeLinejoin="round">
              <circle cx="4" cy="3.5" r="1.6" />
              <circle cx="4" cy="12.5" r="1.6" />
              <circle cx="12" cy="3.5" r="1.6" />
              <path d="M4 5.1v5.8M12 5.1v1.2a3 3 0 0 1-3 3H4" />
            </svg>
          </Glass>
          <span className="h-px w-9 bg-accent" />
          <span className="font-mono text-[11px] uppercase tracking-[0.22em] text-fg-subtle">{t("launch.eyebrow")}</span>
        </div>

        {/* 巨字 Strata(Instrument Serif,品牌字,不翻译) */}
        <h1 className="hero-rise serif mt-7 text-[108px] font-normal leading-[0.9] tracking-[-0.01em] text-fg" style={{ animationDelay: "120ms" }}>
          Strata
        </h1>

        {/* 斜体朱红副题 */}
        <p className="hero-rise serif mt-2.5 text-[31px] italic leading-[1.12] text-accent" style={{ animationDelay: "180ms" }}>
          {t("launch.sub")}
        </p>

        {/* 正文 */}
        <p className="hero-rise mt-5 max-w-[30rem] text-[15px] leading-[1.6] text-fg-muted text-pretty" style={{ animationDelay: "230ms" }}>
          {t("launch.body")}
        </p>

        {/* CTA:朱红药丸(内嵌圆形箭头)+ 描边 克隆 / 新建 */}
        <div className="hero-rise mt-9 flex flex-wrap items-center gap-2.5" style={{ animationDelay: "300ms" }}>
          <button
            onClick={onPick}
            className="group flex items-center gap-3 rounded-full bg-accent py-3 pl-6 pr-3.5 text-sm font-semibold text-white shadow-[0_14px_38px_-12px_color-mix(in_oklab,var(--color-accent)_55%,transparent)] transition-[transform,opacity] duration-500 ease-[cubic-bezier(0.32,0.72,0,1)] hover:opacity-95 active:scale-[0.98]"
          >
            {t("launch.pick")}
            <span className="grid h-[30px] w-[30px] place-items-center rounded-full bg-white/[0.18] transition-transform duration-500 ease-[cubic-bezier(0.32,0.72,0,1)] group-hover:translate-x-0.5">
              <svg width={15} height={15} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.6} strokeLinecap="round" strokeLinejoin="round">
                <path d="M3.5 8h9M8.5 4l4 4-4 4" />
              </svg>
            </span>
          </button>
          <button
            onClick={onClone}
            className="group flex items-center gap-1.5 rounded-full border border-line-strong bg-transparent px-4 py-2.5 text-[13px] text-fg-muted transition-colors hover:bg-elevated hover:text-fg"
          >
            <CloudIcon width={13} height={13} className="shrink-0 text-fg-subtle transition-colors group-hover:text-accent" />
            {t("launch.clone")}
          </button>
          <button
            onClick={onInit}
            className="group flex items-center gap-1.5 rounded-full border border-line-strong bg-transparent px-4 py-2.5 text-[13px] text-fg-muted transition-colors hover:bg-elevated hover:text-fg"
          >
            <PlusIcon width={13} height={13} className="shrink-0 text-fg-subtle transition-colors group-hover:text-accent" />
            {t("launch.init")}
          </button>
        </div>

        {/* 分隔线后:继续上次 + ⌘K 提示 */}
        <div className="hero-rise mt-7 flex items-center gap-4 border-t border-line pt-[22px]" style={{ animationDelay: "360ms" }}>
          {lastRepo && lastName ? (
            <button
              onClick={() => onResume(lastRepo)}
              title={lastRepo}
              data-testid="resume-repo"
              className="group -ml-2 flex max-w-full items-center gap-2 rounded-lg px-2 py-1.5 text-xs text-fg-muted transition-colors hover:text-fg"
            >
              <HistoryIcon width={13} height={13} className="shrink-0 text-fg-subtle transition-colors group-hover:text-accent" />
              <span className="shrink-0">{t("launch.resume")}</span>
              <span className="truncate font-mono text-fg-subtle">{lastName} →</span>
            </button>
          ) : (
            <span className="text-xs text-fg-subtle">{t("launch.paletteHint")}</span>
          )}
          <span className="ml-auto text-xs text-fg-subtle">
            <kbd className="rounded-md border border-line-strong bg-elevated/60 px-1.5 py-0.5 font-mono text-[11px] text-fg-muted">{isMac ? "⌘K" : "Ctrl K"}</kbd> {t("launch.paletteHint")}
          </span>
        </div>
      </div>
    </div>
  );
}
