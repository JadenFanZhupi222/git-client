/**
 * 轻量内联图标。stroke 用 currentColor,所以颜色由父级 text-* 控制。
 * 不引第三方图标库,保持依赖干净、打包小。
 */
import type { SVGProps } from "react";

const base: SVGProps<SVGSVGElement> = {
  width: 16,
  height: 16,
  viewBox: "0 0 16 16",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.6,
  strokeLinecap: "round",
  strokeLinejoin: "round",
};

export const FolderIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M1.75 3.5h4l1.5 1.75h7v6.5a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1V3.5Z" />
  </svg>
);

export const RefreshIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9" />
    <path d="M13.5 2.5V5h-2.5" />
  </svg>
);

// 撤销:逆时针回转箭头(arrow-uturn-left 风格)。
export const UndoIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M2.5 6.5h7.5a3.5 3.5 0 0 1 0 7H6" />
    <path d="M5 3.5 2 6.5l3 3" />
  </svg>
);

// 重做:UndoIcon 的水平镜像(arrow-uturn-right 风格)。
export const RedoIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M13.5 6.5H6a3.5 3.5 0 0 0 0 7h4" />
    <path d="M11 3.5l3 3-3 3" />
  </svg>
);

// 操作日志:带回拨箭头的时钟(history 风格)。
export const HistoryIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M2.6 8a5.4 5.4 0 1 1 1.6 3.8" />
    <path d="M2.5 11.5V8.7h2.7" />
    <path d="M8 5.2V8l2 1.3" />
  </svg>
);

export const BranchIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <circle cx="4" cy="3.5" r="1.5" />
    <circle cx="4" cy="12.5" r="1.5" />
    <circle cx="12" cy="3.5" r="1.5" />
    <path d="M4 5v6M12 5v1a3 3 0 0 1-3 3H4" />
  </svg>
);

export const CommitIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <circle cx="8" cy="8" r="2.5" />
    <path d="M1.5 8h4M10.5 8h4" />
  </svg>
);

export const CheckIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M3 8.5 6.5 12 13 4.5" />
  </svg>
);

export const FileDiffIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M9 1.5H4a1 1 0 0 0-1 1v11a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V5.5L9 1.5Z" />
    <path d="M9 1.5v4h4M6 8h4M6 10.5h4" />
  </svg>
);

export const SunIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <circle cx="8" cy="8" r="3" />
    <path d="M8 1v1.5M8 13.5V15M1 8h1.5M13.5 8H15M3 3l1 1M12 12l1 1M13 3l-1 1M4 12l-1 1" />
  </svg>
);

export const MoonIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M13.5 9.5A5.5 5.5 0 0 1 6.5 2.5a5.5 5.5 0 1 0 7 7Z" />
  </svg>
);

export const FetchIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M8 2v7M5 6.5 8 9.5l3-3M3 12.5h10" />
  </svg>
);

/** 圆环 + 缺口 spinner(径向对称,旋转顺滑)。始终自旋。 */
export const SpinnerIcon = ({ className = "", ...p }: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p} className={`animate-spin ${className}`}>
    <circle cx="8" cy="8" r="6" opacity="0.25" />
    <path d="M8 2a6 6 0 0 1 6 6" />
  </svg>
);

export const PullIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M8 2v6.5M5.5 6 8 8.5 10.5 6M3.5 10.5v2a1 1 0 0 0 1 1h7a1 1 0 0 0 1-1v-2" />
  </svg>
);

export const PushIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M8 3v6.5M5.5 5.5 8 3l2.5 2.5M3.5 10.5v2a1 1 0 0 0 1 1h7a1 1 0 0 0 1-1v-2" />
  </svg>
);

export const StashIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M8 2 1.5 5.5 8 9l6.5-3.5L8 2Z" />
    <path d="M1.5 9 8 12.5 14.5 9" />
  </svg>
);

export const ChevronDownIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M4 6l4 4 4-4" />
  </svg>
);

// 更多/溢出:三点(水平省略号),收纳次要动作。
export const MoreIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p} strokeWidth={0} fill="currentColor">
    <circle cx="3.5" cy="8" r="1.3" />
    <circle cx="8" cy="8" r="1.3" />
    <circle cx="12.5" cy="8" r="1.3" />
  </svg>
);

export const SettingsIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <circle cx="8" cy="8" r="2.25" />
    <path d="M6.8 1.8h2.4l.45 1.7a5 5 0 0 1 1.1.65l1.7-.5 1.2 2.05-1.25 1.2a5 5 0 0 1 0 1.3l1.25 1.2-1.2 2.05-1.7-.5a5 5 0 0 1-1.1.65l-.45 1.7H6.8l-.45-1.7a5 5 0 0 1-1.1-.65l-1.7.5-1.2-2.05L3.6 8.2a5 5 0 0 1 0-1.3L2.35 5.7l1.2-2.05 1.7.5a5 5 0 0 1 1.1-.65l.45-1.7Z" />
  </svg>
);

// 玻璃/透明度:水滴轮廓,表示液态玻璃材质开关。
export const DropletIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M8 2.5C8 2.5 3.5 7 3.5 10a4.5 4.5 0 0 0 9 0C12.5 7 8 2.5 8 2.5Z" />
  </svg>
);

export const AlertIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M8 2.5 1.5 13.5h13L8 2.5Z" />
    <path d="M8 6.5v3.5M8 12h.01" />
  </svg>
);

export const IssueIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <circle cx="8" cy="8" r="6" />
    <path d="M8 4.75v4.5M8 11.75h.01" />
  </svg>
);

export const CloseIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M4 4l8 8M12 4l-8 8" />
  </svg>
);

export const SearchIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <circle cx="7" cy="7" r="4.5" />
    <path d="M10.5 10.5L14 14" />
  </svg>
);

export const PlusIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M8 3v10M3 8h10" />
  </svg>
);

export const MinusIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M3 8h10" />
  </svg>
);

export const MinimizeIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M3 8h10" />
  </svg>
);

export const MaximizeIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <rect x="3" y="3" width="10" height="10" rx="0.5" />
  </svg>
);

export const RestoreIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M5 5V3h8v8h-2" />
    <rect x="3" y="5" width="8" height="8" rx="0.5" />
  </svg>
);

export const TrashIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M2.5 4h11M6 4V2.5h4V4M5 4l.5 9a1 1 0 0 0 1 1h3a1 1 0 0 0 1-1L11 4" />
  </svg>
);

// 子模块:大盒里嵌一个小盒(nested package),提示「仓库里的仓库」。
export const SubmoduleIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <rect x="1.75" y="1.75" width="9" height="9" rx="1" />
    <rect x="6.75" y="6.75" width="7.5" height="7.5" rx="1" />
  </svg>
);

// 工作树:一个根分出多条线到各叶节点(worktree / 多检出)。
export const WorktreeIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <circle cx="3.5" cy="8" r="1.6" />
    <circle cx="12.5" cy="3.5" r="1.6" />
    <circle cx="12.5" cy="12.5" r="1.6" />
    <path d="M5 7.2 11 4M5 8.8l6 3.2" />
  </svg>
);

// 合并:一条支线汇入主线(merge into)。
export const MergeIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <circle cx="4" cy="3" r="1.6" />
    <circle cx="4" cy="13" r="1.6" />
    <circle cx="12" cy="8" r="1.6" />
    <path d="M4 4.6v6.8M4.2 6.2A5 5 0 0 0 10.4 8" />
  </svg>
);

// 云:远程仓库(remote)。
export const CloudIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M4.5 12.5a3 3 0 0 1-.3-5.98A4 4 0 0 1 12 6.2a2.8 2.8 0 0 1-.3 5.6z" />
  </svg>
);

// 追溯 blame:框 + 行(代码块逐行归属)。
export const BlameIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <rect x="2.5" y="2.5" width="11" height="11" rx="1.5" />
    <path d="M6 6h4M6 8.5h4M6 11h2" />
  </svg>
);

// 地球:语言切换(经线 + 纬线)。
export const GlobeIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p} strokeWidth={1.4}>
    <circle cx="8" cy="8" r="6.2" />
    <path d="M2 8h12M8 2a9 9 0 0 1 0 12M8 2a9 9 0 0 0 0 12" />
  </svg>
);

// 拖拽手柄:两列六点(drag-handle / grip),提示「可拖动」。
export const GripIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p} strokeWidth={0} fill="currentColor">
    <circle cx="6" cy="4" r="1.1" />
    <circle cx="10" cy="4" r="1.1" />
    <circle cx="6" cy="8" r="1.1" />
    <circle cx="10" cy="8" r="1.1" />
    <circle cx="6" cy="12" r="1.1" />
    <circle cx="10" cy="12" r="1.1" />
  </svg>
);
