import { useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useBlame } from "../lib/queries";
import { formatRelative } from "../lib/time";
import { FileDiffIcon, HistoryIcon, CloseIcon, BlameIcon } from "../components/icons";
import { Button } from "../components/ui/Button";
import { IconButton } from "../components/ui/IconButton";
import { Glass } from "../components/ui/Glass";
import { EmptyHint } from "../components/ui/EmptyHint";
import { LineHistoryPanel } from "../components/LineHistoryPanel";
import { useT } from "../lib/i18n";
import type { BlameLineDto, IpcError } from "../ipc";

/** 把绝对路径转成仓库根相对路径(正斜杠);不在仓库内返回 null。 */
function toRepoRelative(repo: string, abs: string): string | null {
  const norm = (s: string) => s.replace(/\\/g, "/").replace(/\/+$/, "");
  const r = norm(repo);
  const a = norm(abs);
  if (a.toLowerCase().startsWith(r.toLowerCase() + "/")) return a.slice(r.length + 1);
  return null;
}

/** 年龄热度色(age 0→4):新→旧,从朱红渐隐到 fg-subtle。颜色只用 token + color-mix。 */
function heatColor(age: number): string {
  switch (age) {
    case 0: return "var(--color-accent)";
    case 1: return "color-mix(in oklab, var(--color-accent) 68%, var(--color-fg-subtle))";
    case 2: return "color-mix(in oklab, var(--color-accent) 40%, var(--color-fg-subtle))";
    case 3: return "color-mix(in oklab, var(--color-accent) 20%, var(--color-fg-subtle))";
    default: return "var(--color-fg-subtle)";
  }
}

/** 文件名尾段(basename),用于衬线编辑性标题。 */
function baseName(p: string): string {
  return p.replace(/[/\\]+$/, "").split(/[/\\]/).pop() ?? p;
}

export function BlameView({ repo }: { repo: string }) {
  const t = useT();
  const [file, setFile] = useState<string | null>(null);
  const [pickError, setPickError] = useState<string | null>(null);
  // 行选择:anchor 是起点、focus 是当前端点(shift-点扩范围);范围 = min..max。
  const [sel, setSel] = useState<{ anchor: number; focus: number } | null>(null);
  const [historyRange, setHistoryRange] = useState<{ start: number; end: number } | null>(null);
  const q = useBlame(repo, file);

  // 切仓库清空选择
  useEffect(() => { setFile(null); setPickError(null); }, [repo]);
  // 切文件清空行选择
  useEffect(() => { setSel(null); }, [file]);

  // 点行:普通点 → 单行选中;shift-点 → 从 anchor 扩到该行。
  function clickLine(lineNo: number, shift: boolean) {
    setSel((prev) => (shift && prev ? { anchor: prev.anchor, focus: lineNo } : { anchor: lineNo, focus: lineNo }));
  }
  const selRange = sel ? { start: Math.min(sel.anchor, sel.focus), end: Math.max(sel.anchor, sel.focus) } : null;

  async function pick() {
    setPickError(null);
    const picked = await open({ multiple: false, directory: false, defaultPath: repo, title: t("blame.dialogTitle") });
    if (typeof picked !== "string") return;
    const rel = toRepoRelative(repo, picked);
    if (!rel) { setPickError(t("blame.notInRepo")); return; }
    setFile(rel);
  }

  const lines = q.data ?? [];
  const queryErr = (q.error as IpcError | null)?.message ?? null;

  // 派生:每行年龄档(按提交时间在 [min,max] 区间的位置分 5 档,新=0)+ 概览统计。
  const meta = useMemo(() => computeBlameMeta(lines), [lines]);

  // 浮动玻璃工具栏高度 → 内容顶部留白,blame 行从栏底穿过(满汉折射)。
  const barRef = useRef<HTMLDivElement>(null);
  const [barH, setBarH] = useState(38);
  useEffect(() => {
    const el = barRef.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => setBarH(el.offsetHeight));
    ro.observe(el);
    setBarH(el.offsetHeight);
    return () => ro.disconnect();
  }, []);

  return (
    <div className="relative flex h-full flex-col">
      {/* 浮动液态玻璃工具栏:blame 图标 + mono 文件路径 + 统计 + 年龄热度图例;blame 行从其下穿过显折射。
          定位放外层普通 div(.glass 自带 position:relative 会压过 absolute 工具类)。 */}
      <div className="absolute inset-x-0 top-0 z-10">
        <Glass>
          <div ref={barRef}>
            <div className="flex shrink-0 items-center gap-2 border-b border-line px-3 py-1.5">
              <Button variant="ghost" size="sm" onClick={pick}>
                <FileDiffIcon width={13} height={13} /> {t("blame.pick")}
              </Button>
              {file && (
                <>
                  <BlameIcon width={13} height={13} className="shrink-0 text-accent" />
                  <span className="truncate font-mono text-xs text-fg" title={file}>{file}</span>
                </>
              )}
              {file && meta && (
                <span className="shrink-0 font-mono text-[11px] text-fg-subtle">
                  {t("blame.stat", { lines: meta.lineCount, commits: meta.commitCount })}
                </span>
              )}
              {file && meta && <HeatLegend t={t} />}
              {selRange && (
                <div className="ml-auto flex shrink-0 items-center gap-1.5">
                  <Button
                    variant="secondary"
                    size="chip"
                    onClick={() => setHistoryRange(selRange)}
                    title={t("blame.lineHistoryTitle")}
                  >
                    <HistoryIcon width={12} height={12} />
                    {selRange.start === selRange.end
                      ? t("blame.lineHistoryOne", { n: selRange.start })
                      : t("blame.lineHistoryRange", { start: selRange.start, end: selRange.end })}
                  </Button>
                  <IconButton aria-label={t("blame.clearSel")} title={t("blame.clearSel")} onClick={() => setSel(null)}>
                    <CloseIcon width={13} height={13} />
                  </IconButton>
                </div>
              )}
            </div>
            {pickError && <p className="border-b border-line px-3 py-1.5 text-xs text-danger">{pickError}</p>}
          </div>
        </Glass>
      </div>

      {/* 内容 */}
      {!file ? (
        <EmptyHint
          icon={<HistoryIcon width={24} height={24} />}
          action={<Button variant="secondary" size="sm" onClick={pick}><FileDiffIcon width={13} height={13} /> {t("blame.pick")}</Button>}
        >
          {t("blame.empty")}
        </EmptyHint>
      ) : q.isLoading ? (
        <Center>{t("blame.loading")}</Center>
      ) : queryErr ? (
        // 超大 / 二进制文件等:后端返回友好错误,居中显示(而不是误报「空文件」)
        <Center>{queryErr}</Center>
      ) : lines.length === 0 ? (
        <Center>{t("blame.emptyFile")}</Center>
      ) : (
        <div className="fade-in flex-1 overflow-auto" style={{ paddingTop: barH }}>
          {/* 滚动体顶部:编辑性头 —— 衬线文件名 + 作者说明句 */}
          {meta && (
            <div className="border-b border-line px-6 pb-4 pt-5">
              <h1 className="serif text-[24px] font-normal leading-none text-fg">{baseName(file)}</h1>
              <p className="mt-2 max-w-[60ch] text-[12.5px] leading-relaxed text-fg-muted">{blameSentence(t, meta)}</p>
            </div>
          )}

          <div className="font-mono text-[12px] leading-5">
            {lines.map((l, i) => {
              const prev = lines[i - 1];
              const newGroup = !prev || prev.commit_id !== l.commit_id;
              const uncommitted = l.commit_id === "";
              const selected = !!selRange && l.line_no >= selRange.start && l.line_no <= selRange.end;
              const age = meta?.ageOf.get(l.commit_id) ?? 4;
              const groupOdd = (meta?.groupIndexOf.get(l.line_no) ?? 0) % 2 === 1;
              return (
                <div
                  key={i}
                  onClick={(e) => clickLine(l.line_no, e.shiftKey)}
                  className={[
                    "flex cursor-pointer select-none items-stretch",
                    newGroup ? "border-t border-line" : "",
                    selected ? "bg-accent/15" : groupOdd ? "bg-fg/[0.025] hover:bg-elevated" : "hover:bg-elevated",
                  ].join(" ")}
                >
                  {/* 左 gutter(252px):年龄热度脊 + sha + 作者 + 相对时间。
                      续行隐去文字、热度脊降到 .3 透明,让 commit 块成条可读。 */}
                  <div
                    className={`flex w-[252px] shrink-0 items-center gap-2 overflow-hidden px-2 ${uncommitted ? "text-fg-subtle" : "text-fg-muted"}`}
                    title={uncommitted ? t("blame.uncommitted") : `${l.commit_id}\n${l.author_name} · ${formatRelative(l.timestamp)}`}
                  >
                    {/* 年龄热度脊(3px):首行满色,续行 .3 透明 */}
                    <span
                      className="h-4 w-[3px] shrink-0 rounded-full"
                      style={{ background: heatColor(age), opacity: newGroup ? 1 : 0.3 }}
                    />
                    {newGroup &&
                      (uncommitted ? (
                        <span>· {t("blame.uncommitted")}</span>
                      ) : (
                        <>
                          <span className="shrink-0" style={{ color: heatColor(age) }}>{l.short_id}</span>
                          <span className="min-w-0 flex-1 truncate">{l.author_name}</span>
                          <span className="shrink-0 text-fg-subtle">{formatRelative(l.timestamp)}</span>
                        </>
                      ))}
                  </div>
                  <span className="w-10 shrink-0 select-none border-l border-line/60 px-1.5 text-right text-fg-subtle">{l.line_no}</span>
                  <span className="flex-1 whitespace-pre pr-3 text-fg">{l.content || " "}</span>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {historyRange && file && (
        <LineHistoryPanel repo={repo} file={file} range={historyRange} onClose={() => setHistoryRange(null)} />
      )}
    </div>
  );
}

type BlameMeta = {
  lineCount: number;
  commitCount: number;
  spanDays: number;
  topAuthors: string[];
  ageOf: Map<string, number>; // commit_id → 年龄档 0..4
  groupIndexOf: Map<number, number>; // line_no → 该行所属 commit 组的序号(隔组淡底)
};

/** 计算 blame 概览:年龄分档、提交数、跨天、主要作者、commit 分组序号。 */
function computeBlameMeta(lines: BlameLineDto[]): BlameMeta | null {
  if (lines.length === 0) return null;
  const committed = lines.filter((l) => l.commit_id !== "");
  const times = committed.map((l) => l.timestamp);
  const minT = times.length ? Math.min(...times) : 0;
  const maxT = times.length ? Math.max(...times) : 0;
  const span = Math.max(1, maxT - minT);

  // 年龄档:t=maxT(最新)→0,t=minT(最旧)→4。
  const ageOf = new Map<string, number>();
  for (const l of committed) {
    if (ageOf.has(l.commit_id)) continue;
    const frac = (maxT - l.timestamp) / span;
    ageOf.set(l.commit_id, Math.min(4, Math.max(0, Math.floor(frac * 5))));
  }

  // 作者按出现行数排序,取前二(真实名,不翻译)。
  const authorCount = new Map<string, number>();
  for (const l of committed) authorCount.set(l.author_name, (authorCount.get(l.author_name) ?? 0) + 1);
  const topAuthors = [...authorCount.entries()].sort((a, b) => b[1] - a[1]).map(([n]) => n);

  // 分组序号:相邻同 commit 为一组,组号递增(供隔组淡底)。
  const groupIndexOf = new Map<number, number>();
  let gi = -1;
  let prevId: string | null = null;
  for (const l of lines) {
    if (l.commit_id !== prevId) { gi++; prevId = l.commit_id; }
    groupIndexOf.set(l.line_no, gi);
  }

  return {
    lineCount: lines.length,
    commitCount: new Set(committed.map((l) => l.commit_id)).size,
    spanDays: Math.max(1, Math.ceil((maxT - minT) / 86400)),
    topAuthors,
    ageOf,
    groupIndexOf,
  };
}

/** 作者说明句:按作者数选单/双人模板。 */
function blameSentence(t: (k: "blame.sentence1" | "blame.sentence2", p?: Record<string, string | number>) => string, m: BlameMeta): string {
  const [a, b] = m.topAuthors;
  const base = { lines: m.lineCount, commits: m.commitCount, days: m.spanDays };
  if (a && b) return t("blame.sentence2", { ...base, a, b });
  return t("blame.sentence1", { ...base, a: a ?? "—" });
}

/** 年龄热度图例:新 → 旧,朱红渐变到 fg-subtle。 */
function HeatLegend({ t }: { t: (k: "blame.legendNew" | "blame.legendOld") => string }) {
  return (
    <span className="ml-1 flex shrink-0 items-center gap-1.5 text-[10px] text-fg-subtle">
      <span>{t("blame.legendNew")}</span>
      <span className="flex items-center gap-0.5">
        {[0, 1, 2, 3, 4].map((age) => (
          <span key={age} className="h-1.5 w-2.5 rounded-sm" style={{ background: heatColor(age) }} />
        ))}
      </span>
      <span>{t("blame.legendOld")}</span>
    </span>
  );
}

function Center({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-1 items-center justify-center p-4 text-center text-sm text-fg-subtle">
      {children}
    </div>
  );
}
