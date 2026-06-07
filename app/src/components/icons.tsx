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

export const AlertIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M8 2.5 1.5 13.5h13L8 2.5Z" />
    <path d="M8 6.5v3.5M8 12h.01" />
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

export const TrashIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M2.5 4h11M6 4V2.5h4V4M5 4l.5 9a1 1 0 0 0 1 1h3a1 1 0 0 0 1-1L11 4" />
  </svg>
);
