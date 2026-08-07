import type { Lang } from "../i18n";

export const windowLabels: Record<Lang, {
  minimize: string;
  maximize: string;
  restore: string;
  close: string;
}> = {
  zh: { minimize: "最小化窗口", maximize: "最大化窗口", restore: "还原窗口", close: "关闭窗口" },
  en: {
    minimize: "Minimize window",
    maximize: "Maximize window",
    restore: "Restore window",
    close: "Close window",
  },
};
