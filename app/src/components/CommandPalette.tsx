import { useEffect, useMemo, useRef, useState } from "react";
import { rankCommands, type RankedCommand } from "../lib/commands";
import type { Command } from "../lib/commands";
import { cx } from "./ui/Button";
import { SearchIcon } from "./icons";

/**
 * 命令面板(⌘K):所有动作可搜索、可键盘触发——M3「Fluid」的入口。
 * 纯展示 + 键盘交互;命令列表与排序逻辑分别由 App 组装、commands.ts 负责。
 *
 * 交互:输入即过滤;↑↓ 移动高亮、回车执行、Esc 关闭;鼠标移入也更新高亮。
 * 关闭由父级控制(onClose),执行前先关面板再 run()。
 */
export function CommandPalette({ commands, onClose }: { commands: Command[]; onClose: () => void }) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const listRef = useRef<HTMLUListElement>(null);

  const results = useMemo(() => rankCommands(commands, query), [commands, query]);

  // query 变化 → 高亮回到第一项(避免停在越界下标)
  useEffect(() => {
    setActive(0);
  }, [query]);

  // 高亮项滚动进可视区
  useEffect(() => {
    listRef.current?.querySelector<HTMLElement>(`[data-idx="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }, [active]);

  function activate(i: number) {
    const r = results[i];
    if (!r || r.cmd.disabled) return;
    onClose();
    r.cmd.run();
  }

  function onKeyDown(e: React.KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((a) => Math.min(a + 1, results.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => Math.max(a - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      activate(active);
    } else if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }

  return (
    <div className="fixed inset-0 z-[80] flex items-start justify-center" role="dialog" aria-modal="true" aria-label="命令面板">
      <div className="absolute inset-0 bg-black/40" onClick={onClose} />
      <div className="relative mt-[12vh] w-[34rem] max-w-[92vw] overflow-hidden rounded-lg border border-line-strong bg-elevated shadow-2xl">
        {/* 搜索框 */}
        <div className="flex items-center gap-2 border-b border-line px-3">
          <SearchIcon width={15} height={15} className="shrink-0 text-fg-subtle" />
          <input
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder="输入命令…"
            // focus-visible:outline-none 专门盖掉 index.css 的全局键盘焦点环(否则输入框被一圈蓝框包住,难看)
            className="w-full bg-transparent py-2.5 text-sm text-fg placeholder:text-fg-subtle focus:outline-none focus-visible:outline-none"
          />
          <kbd className="shrink-0 rounded border border-line-strong px-1.5 py-0.5 font-mono text-[10px] text-fg-subtle">Esc</kbd>
        </div>

        {/* 结果列表 */}
        <ul ref={listRef} className="max-h-[50vh] overflow-y-auto py-1">
          {results.length === 0 ? (
            <li className="px-3 py-6 text-center text-xs text-fg-subtle">无匹配命令</li>
          ) : (
            results.map((r, i) => <Row key={r.cmd.id} r={r} idx={i} active={i === active} onActivate={() => activate(i)} onHover={() => setActive(i)} />)
          )}
        </ul>
      </div>
    </div>
  );
}

function Row({ r, idx, active, onActivate, onHover }: { r: RankedCommand; idx: number; active: boolean; onActivate: () => void; onHover: () => void }) {
  const { cmd, indices } = r;
  return (
    <li data-idx={idx}>
      <button
        disabled={cmd.disabled}
        onClick={onActivate}
        onMouseMove={onHover}
        // 选中行:accent 浅底 + 2px accent 左条(透明左条占位,切换时不抖动),明显区别于普通行
        className={cx(
          "flex w-full items-center gap-3 border-l-2 px-3 py-2 text-left text-sm transition-colors",
          active && !cmd.disabled ? "border-accent bg-accent/15" : "border-transparent",
          cmd.disabled ? "cursor-not-allowed opacity-40" : "text-fg",
        )}
      >
        <span className="min-w-0 flex-1 truncate">
          <Highlight text={cmd.title} indices={indices} />
          {cmd.subtitle && <span className="ml-2 text-xs text-fg-subtle">{cmd.subtitle}</span>}
        </span>
        <span className="shrink-0 rounded bg-canvas px-1.5 py-0.5 text-[10px] text-fg-subtle">{cmd.group}</span>
      </button>
    </li>
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
