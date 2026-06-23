/**
 * 命令面板(⌘K)的纯逻辑层:命令模型 + 模糊匹配 + 排序。
 *
 * 设计:UI(CommandPalette)只负责渲染与键盘交互;「能搜到什么、怎么排序」
 * 全在这里,纯函数、可单测(对齐 M1.0 起的前端测试网纪律)。
 * 命令本身在 App.tsx 组装(那里握有 repo / 各操作 handler)。
 */

import type { ReactNode } from "react";

/** 一条可执行命令。run() 触发动作;disabled 时灰显且不可执行(如未开仓库)。 */
export interface Command {
  /** 稳定唯一 id(用作 React key)。 */
  id: string;
  /** 行首图标(对齐原型命令面板)。 */
  icon?: ReactNode;
  /** 显示名,也是主要的搜索目标,如 "Fetch"、"切换到历史"。 */
  title: string;
  /** 右侧灰色说明,如快捷键或补充。 */
  subtitle?: string;
  /** 分组标签,显示在行尾,如 "远程"、"视图"、"撤销"。 */
  group: string;
  /** 额外可搜索词(中英文别名),空格分隔;title 没命中时回退匹配它。 */
  keywords?: string;
  /** 当前不可用:灰显 + 回车/点击不执行。 */
  disabled?: boolean;
  /** 若提供:激活该命令不直接 run,而是进入「跳转」子模式,对这些项做二次模糊选择
   *  (如「跳转到分支」→ 列出分支)。run 仍可作为空操作占位。 */
  jump?: JumpMode;
  /** 执行动作。面板会先关闭再调用。 */
  run: () => void;
}

/** 跳转子模式:换一个占位提示 + 一批可模糊选择的项(分支/提交/文件…)。 */
export interface JumpMode {
  placeholder: string;
  items: JumpItem[];
}

/** 跳转子模式里的一项。 */
export interface JumpItem {
  id: string;
  label: string;
  /** 右侧灰色补充(如「当前」「ahead 2」)。 */
  hint?: string;
  run: () => void;
}

/** 模糊匹配结果:score 越大越靠前;indices 是命中字符在文本中的下标(用于高亮)。 */
export interface FuzzyResult {
  score: number;
  indices: number[];
}

/**
 * 子序列模糊匹配:query 的字符须按顺序(可不连续)出现在 text 中。
 * 不匹配返回 null。评分偏好:连续命中、词首命中、整体靠前——和主流命令面板一致。
 */
export function fuzzyMatch(query: string, text: string): FuzzyResult | null {
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  if (q.length === 0) return { score: 0, indices: [] };
  if (q.length > t.length) return null;

  const indices: number[] = [];
  let score = 0;
  let ti = 0;
  let prev = -2; // 上一个命中下标,用于判断连续

  for (const ch of q) {
    let found = -1;
    for (let k = ti; k < t.length; k++) {
      if (t[k] === ch) {
        found = k;
        break;
      }
    }
    if (found === -1) return null;

    score += 1; // 基础分:每命中一个字符
    if (found === prev + 1) score += 5; // 连续命中
    const before = found > 0 ? t[found - 1] : "";
    if (found === 0 || before === " " || before === "-" || before === "_" || before === "/") {
      score += 8; // 词首命中(开头或分隔符之后)
    }

    indices.push(found);
    prev = found;
    ti = found + 1;
  }

  score -= indices[0]; // 命中起点越靠前越好(轻微)
  return { score, indices };
}

/** 排序后的一条结果:命令 + title 内的高亮下标(回退到 keywords 命中时为空)。 */
export interface RankedCommand {
  cmd: Command;
  indices: number[];
}

/**
 * 过滤 + 排序命令列表。
 * - 空 query:原样返回(保留 App 组装顺序,作为「默认菜单」)。
 * - 非空:优先 title 命中(可高亮),其次 keywords 命中(不高亮、分更低)。
 */
export function rankCommands(commands: Command[], query: string): RankedCommand[] {
  const q = query.trim();
  if (!q) return commands.map((cmd) => ({ cmd, indices: [] }));

  const scored: { cmd: Command; indices: number[]; score: number }[] = [];
  for (const cmd of commands) {
    const inTitle = fuzzyMatch(q, cmd.title);
    if (inTitle) {
      scored.push({ cmd, indices: inTitle.indices, score: inTitle.score + 100 });
      continue;
    }
    if (cmd.keywords) {
      const inKw = fuzzyMatch(q, cmd.keywords);
      if (inKw) scored.push({ cmd, indices: [], score: inKw.score });
    }
  }
  scored.sort((a, b) => b.score - a.score);
  return scored.map(({ cmd, indices }) => ({ cmd, indices }));
}

/**
 * 通用模糊排序:对任意项按其文本做 fuzzyMatch 过滤 + 排序(用于跳转子模式的分支/提交等)。
 * 空 query 原样返回;命中项带高亮下标。
 */
export function rankBy<T>(items: T[], text: (t: T) => string, query: string): { item: T; indices: number[] }[] {
  const q = query.trim();
  if (!q) return items.map((item) => ({ item, indices: [] }));
  const scored: { item: T; indices: number[]; score: number }[] = [];
  for (const item of items) {
    const m = fuzzyMatch(q, text(item));
    if (m) scored.push({ item, indices: m.indices, score: m.score });
  }
  scored.sort((a, b) => b.score - a.score);
  return scored.map(({ item, indices }) => ({ item, indices }));
}
