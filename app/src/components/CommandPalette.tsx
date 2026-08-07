import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { rankCommands, rankBy, type Command, type JumpMode } from "../lib/commands";
import { cx } from "./ui/Button";
import { SearchIcon } from "./icons";
import { Glass } from "./ui/Glass";
import { useT } from "../lib/i18n";

/**
 * 命令面板(⌘K):所有动作可搜索、可键盘触发——M3「Fluid」的入口。
 *
 * 两级:① 命令模式(默认)列出所有动作;② 跳转子模式——激活带 jump 的命令后,
 * 改为对一批项(分支/提交/文件…)做二次模糊选择。两级共用同一套键盘交互。
 *
 * 交互:输入即过滤;↑↓ 移动(跳过灰条)、回车执行、Esc/Backspace(空输入)返回上一级或关闭。
 */
export function CommandPalette({ commands, onClose }: { commands: Command[]; onClose: () => void }) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const [jump, setJump] = useState<JumpMode | null>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const t = useT();

  // 把两级统一成 Entry,键盘/渲染逻辑只认 Entry。命令模式按 group 分区(原型版式):
  // 每组一个不可选 header 行 + 其命令行;跳转子模式则是扁平列表(按相关度排)。
  const entries: Entry[] = useMemo(() => {
    if (jump) {
      return rankBy(jump.items, (i) => i.label, query).map((r) => ({
        key: r.item.id,
        onActivate: () => {
          onClose();
          r.item.run();
        },
        node: <JumpRow label={r.item.label} hint={r.item.hint} indices={r.indices} />,
      }));
    }
    const ranked = rankCommands(commands, query);
    // 按 group 分桶,保留各组首次出现顺序(空 query = App 组装序;搜索 = 按最佳匹配组序)。
    const order: string[] = [];
    const byGroup = new Map<string, typeof ranked>();
    for (const r of ranked) {
      if (!byGroup.has(r.cmd.group)) { byGroup.set(r.cmd.group, []); order.push(r.cmd.group); }
      byGroup.get(r.cmd.group)!.push(r);
    }
    const out: Entry[] = [];
    for (const g of order) {
      out.push({ key: `header:${g}`, header: g, onActivate: () => {} });
      for (const r of byGroup.get(g)!) {
        out.push({
          key: r.cmd.id,
          disabled: r.cmd.disabled,
          onActivate: () => {
            if (r.cmd.disabled) return;
            if (r.cmd.jump) {
              setJump(r.cmd.jump); // 进入跳转子模式,不关面板
              setQuery("");
              return;
            }
            onClose();
            r.cmd.run();
          },
          node: <CommandRow icon={r.cmd.icon} title={r.cmd.title} subtitle={r.cmd.subtitle} indices={r.indices} />,
        });
      }
    }
    return out;
  }, [commands, query, jump, onClose]);

  // 从 from 出发沿 dir 找下一个可选项(跳过 header 与 disabled);到边界停在原地。
  function selectable(e: Entry) { return !e.header && !e.disabled; }
  function step(from: number, dir: 1 | -1): number {
    for (let i = from + dir; i >= 0 && i < entries.length; i += dir) {
      if (selectable(entries[i])) return i;
    }
    return from >= 0 && from < entries.length && selectable(entries[from]) ? from : -1;
  }
  const firstEnabled = () => entries.findIndex(selectable);

  // query 或模式变化 → 高亮落到第一个可用项
  useEffect(() => {
    setActive(firstEnabled());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, jump]);

  useEffect(() => {
    listRef.current?.querySelector<HTMLElement>(`[data-idx="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }, [active]);

  function activate(i: number) {
    entries[i]?.onActivate();
  }

  // Esc / 空输入 Backspace:在跳转子模式里返回命令模式,否则关闭面板。
  function back() {
    if (jump) {
      setJump(null);
      setQuery("");
    } else {
      onClose();
    }
  }

  function onKeyDown(e: React.KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((a) => step(a, 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => step(a, -1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      activate(active);
    } else if (e.key === "Escape") {
      e.preventDefault();
      back();
    } else if (e.key === "Backspace" && query === "" && jump) {
      e.preventDefault();
      setJump(null);
    }
  }

  return (
    <div className="fixed inset-0 z-[80] flex items-start justify-center" role="dialog" aria-modal="true" aria-label={t("action.commandPalette")}>
      <div className="overlay-in absolute inset-0 bg-[#05080d]/55" onClick={onClose} />
      <Glass className="panel-in relative mt-[14vh] w-[35rem] max-w-[92vw] overflow-hidden rounded-lg">
        {/* 搜索框 */}
        <div className="flex items-center gap-2 border-b border-line px-3">
          <SearchIcon width={15} height={15} className="shrink-0 text-fg-subtle" />
          {/* 跳转子模式:左侧面包屑,点/Backspace 返回命令模式 */}
          {jump && (
            <button
              onClick={() => setJump(null)}
              className="shrink-0 rounded bg-accent/15 px-1.5 py-0.5 text-[11px] text-accent-ink transition-colors hover:bg-accent/25"
              title={t("palette.back")}
            >
              {t("palette.backShort")}
            </button>
          )}
          <input
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder={jump ? jump.placeholder : t("palette.placeholder")}
            // index.css 的全局 :focus-visible 焦点环是无层级规则,工具类盖不住,只能内联 style 关掉
            style={{ outline: "none" }}
            className="w-full bg-transparent py-2.5 text-sm text-fg placeholder:text-fg-subtle"
          />
          <kbd className="shrink-0 rounded border border-line-strong px-1.5 py-0.5 font-mono text-[10px] text-fg-subtle">Esc</kbd>
        </div>

        {/* 结果列表 */}
        <ul ref={listRef} className="max-h-[50vh] overflow-y-auto px-1 py-1">
          {entries.length === 0 ? (
            <li className="px-3 py-6 text-center text-xs text-fg-subtle">{jump ? t("palette.noMatchItem") : t("palette.noMatchCmd")}</li>
          ) : (
            entries.map((e, i) =>
              e.header ? (
                <li key={e.key} className="px-3 pb-1 pt-3 text-[10px] font-semibold uppercase tracking-[0.06em] text-fg-subtle first:pt-1.5">
                  {e.header}
                </li>
              ) : (
                // 灰条(disabled)不参与 hover 高亮:鼠标移上去不抢选中
                <Row key={e.key} idx={i} active={i === active} disabled={e.disabled} onActivate={() => activate(i)} onHover={() => !e.disabled && setActive(i)}>
                  {e.node}
                </Row>
              ),
            )
          )}
        </ul>
      </Glass>
    </div>
  );
}

interface Entry {
  key: string;
  disabled?: boolean;
  /** 非空表示这是分区标题行(不可选,渲染为小标签)。 */
  header?: string;
  onActivate: () => void;
  node?: ReactNode;
}

function Row({
  idx,
  active,
  disabled,
  onActivate,
  onHover,
  children,
}: {
  idx: number;
  active: boolean;
  disabled?: boolean;
  onActivate: () => void;
  onHover: () => void;
  children: ReactNode;
}) {
  return (
    <li data-idx={idx}>
      <button
        disabled={disabled}
        onClick={onActivate}
        onMouseMove={onHover}
        // 选中行:accent 浅底圆角整条(对齐原型,无左条)
        className={cx(
          "flex w-full items-center gap-3 rounded-md px-3 py-2 text-left text-sm transition-colors",
          active && !disabled ? "bg-accent/15" : "",
          disabled ? "cursor-not-allowed opacity-40" : "text-fg",
        )}
      >
        {children}
      </button>
    </li>
  );
}

/** 命令行内容:左图标 + 标题(高亮)+ 副标题。分组由分区标题表达,行内不再挂 chip。 */
function CommandRow({ icon, title, subtitle, indices }: { icon?: ReactNode; title: string; subtitle?: string; indices: number[] }) {
  return (
    <>
      <span className="grid w-4 shrink-0 place-items-center text-fg-subtle">{icon}</span>
      <span className="min-w-0 flex-1 truncate">
        <Highlight text={title} indices={indices} />
        {subtitle && <span className="ml-2 text-xs text-fg-subtle">{subtitle}</span>}
      </span>
    </>
  );
}

/** 跳转项内容:label(mono、高亮)+ hint。 */
function JumpRow({ label, hint, indices }: { label: string; hint?: string; indices: number[] }) {
  return (
    <>
      <span className="min-w-0 flex-1 truncate font-mono text-[13px]">
        <Highlight text={label} indices={indices} />
      </span>
      {hint && <span className="shrink-0 text-[11px] text-fg-subtle">{hint}</span>}
    </>
  );
}

/** 把匹配到的字符着色加粗,其余原样。 */
function Highlight({ text, indices }: { text: string; indices: number[] }) {
  if (indices.length === 0) return <>{text}</>;
  const hit = new Set(indices);
  return (
    <>
      {[...text].map((ch, i) =>
        hit.has(i) ? (
          <span key={i} className="font-semibold text-accent">
            {ch}
          </span>
        ) : (
          <span key={i}>{ch}</span>
        ),
      )}
    </>
  );
}
