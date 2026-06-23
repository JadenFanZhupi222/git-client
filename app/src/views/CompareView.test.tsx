import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { CompareView } from "./CompareView";
import * as ipc from "../ipc";

// 锁住已修 bug:首次进入比较页,两端 = 当前分支(而非空 / 被首个分支冒充)。
// 历史上这里因「设默认」与「切仓库清空」两个 effect 互相覆盖,from 永空,
// enabled=false 不发起比较。合并成单 effect 后修复,本测试防回归。
vi.mock("../ipc", () => ({
  listRefs: vi.fn().mockResolvedValue([
    { name: "main", kind: "local" },
    { name: "dev", kind: "local" },
    { name: "origin/main", kind: "remote" },
  ]),
  getCurrentBranch: vi.fn().mockResolvedValue("main"),
  compareFiles: vi.fn().mockResolvedValue([]),
  compareFileDiff: vi.fn().mockResolvedValue(null),
}));

function wrap(ui: ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{ui}</QueryClientProvider>;
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("CompareView 默认两端", () => {
  it("当前分支解析后,from 与 to 选择器都显示当前分支", async () => {
    render(wrap(<CompareView repo="/repo" />));
    // ref 药丸内核是原生 <select>(role=combobox);第一个=from,第二个=to。
    // 用 role 而非 aria-label 查询,避免依赖当前语言(label 已 i18n 化)。
    await waitFor(() => {
      const combos = screen.getAllByRole("combobox");
      expect(combos).toHaveLength(2);
      expect(combos[0]).toHaveValue("main");
      expect(combos[1]).toHaveValue("main");
    });
  });

  it("默认即以 (当前分支 → 当前分支) 发起一次比较查询", async () => {
    render(wrap(<CompareView repo="/repo" />));
    await waitFor(() => {
      expect(ipc.compareFiles).toHaveBeenCalledWith("/repo", "main", "main");
    });
  });
});
