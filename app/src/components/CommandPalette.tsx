import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { rankCommands, rankBy, type Command, type JumpMode } from "../lib/commands";
import { cx } from "./ui/Button";
import { SearchIcon } from "./icons";
import { Glass } from "./ui/Glass";

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

  // 把两级统一成 Entry,键盘/渲染逻辑只认 Entry。
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
    return rankCommands(commands, query).map((r) => ({
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
      node: <CommandRow title={r.cmd.title} subtitle={r.cmd.subtitle} group={r.cmd.group} indices={r.indices} />,
    }));
  }, [commands, query, jump, onClose]);

  // 从 from 出发沿 dir 找下一个可用项;到边界停在原地。
  function step(from: number, dir: 1 | -1): number {
    for (let i = from + dir; i >= 0 && i < entries.length; i += dir) {
      if (!entries[i].disabled) return i;
    }
    return from >= 0 && from < entries.length && !entries[from]?.disabled ? from : -1;
  }
  const firstEnabled = () => entries.findIndex((e) => !e.disabled);

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
    <div className="fixed inset-0 z-[80] flex items-start justify-center" role="dialog" aria-modal="true" aria-label="命令面板">
      <div className="absolute inset-0 bg-black/40" onClick={onClose} />
      <Glass className="relative mt-[12vh] w-[34rem] max-w-[92vw] overflow-hidden rounded-lg">
        {/* 搜索框 */}
        <div className="flex items-center gap-2 border-b border-line px-3">
          <SearchIcon width={15} height={15} className="shrink-0 text-fg-subtle" />
          {/* 跳转子模式:左侧面包屑,点/Backspace 返回命令模式 */}
          {jump && (
            <button
              onClick={() => setJump(null)}
              className="shrink-0 rounded bg-accent/15 px-1.5 py-0.5 text-[11px] text-accent transition-colors hover:bg-accent/25"
              title="返回命令(Esc)"
            >
              ‹ 命令
            </button>
          )}
          <input
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder={jump ? jump.placeholder : "输入命令…"}
            // index.css 的全局 :focus-visible 焦点环是无层级规则,工具类盖不住,只能内联 style 关掉
            style={{ outline: "none" }}
            className="w-full bg-transparent py-2.5 text-sm text-fg placeholder:text-fg-subtle"
          />
          <kbd className="shrink-0 rounded border border-line-strong px-1.5 py-0.5 font-mono text-[10px] text-fg-subtle">Esc</kbd>
        </div>

        {/* 结果列表 */}
        <ul ref={listRef} className="max-h-[50vh] overflow-y-auto py-1">
          {entries.length === 0 ? (
            <li className="px-3 py-6 text-center text-xs text-fg-subtle">{jump ? "无匹配项" : "无匹配命令"}</li>
          ) : (
            entries.map((e, i) => (
              // 灰条(disabled)不参与 hover 高亮:鼠标移上去不抢选中
              <Row key={e.key} idx={i} active={i === active} disabled={e.disabled} onActivate={() => activate(i)} onHover={() => !e.disabled && setActive(i)}>
                {e.node}
              </Row>
            ))
          )}
        </ul>
      </Glass>
    </div>
  );
}

interface Entry {
  key: string;
  disabled?: boolean;
  onActivate: () => void;
  node: ReactNode;
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
        // 选中行:accent 浅底 + 2px accent 左条(透明左条占位,切换时不抖动)
        className={cx(
          "flex w-full items-center gap-3 border-l-2 px-3 py-2 text-left text-sm transition-colors",
          active && !disabled ? "border-accent bg-accent/15" : "border-transparent",
          disabled ? "cursor-not-allowed opacity-40" : "text-fg",
        )}
      >
        {children}
      </button>
    </li>
  );
}

/** 命令行内容:标题(高亮)+ 副标题 + 分组 chip。 */
function CommandRow({ title, subtitle, group, indices }: { title: string; subtitle?: string; group: string; indices: number[] }) {
  return (
    <>
      <span className="min-w-0 flex-1 truncate">
        <Highlight text={title} indices={indices} />
        {subtitle && <span className="ml-2 text-xs text-fg-subtle">{subtitle}</span>}
      </span>
      <span className="shrink-0 rounded bg-canvas px-1.5 py-0.5 text-[10px] text-fg-subtle">{group}</span>
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
