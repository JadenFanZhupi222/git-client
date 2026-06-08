import { describe, it, expect } from "vitest";
import { fuzzyMatch, rankCommands, type Command } from "./commands";

// 命令面板纯逻辑回归。锁三件事:① 子序列命中/不命中;② 评分偏好(连续、词首、靠前);
// ③ rankCommands 的 title 优先 / keywords 回退 / 空 query 原样返回。

describe("fuzzyMatch", () => {
  it("空 query → 命中且无高亮下标", () => {
    const r = fuzzyMatch("", "Fetch");
    expect(r).not.toBeNull();
    expect(r!.indices).toEqual([]);
  });

  it("按顺序的子序列命中,记录下标", () => {
    const r = fuzzyMatch("fh", "Fetch");
    expect(r).not.toBeNull();
    expect(r!.indices).toEqual([0, 4]); // F...h
  });

  it("顺序不符 → 不命中", () => {
    expect(fuzzyMatch("hf", "Fetch")).toBeNull();
  });

  it("query 比文本长 → 不命中", () => {
    expect(fuzzyMatch("fetchh", "Fetch")).toBeNull();
  });

  it("大小写不敏感", () => {
    expect(fuzzyMatch("FET", "fetch")).not.toBeNull();
  });

  it("连续命中比分散命中得分高", () => {
    const contiguous = fuzzyMatch("fet", "fetch")!; // f-e-t 连续
    const scattered = fuzzyMatch("fch", "fetch")!; // f..c.h 分散
    expect(contiguous.score).toBeGreaterThan(scattered.score);
  });

  it("词首命中比词中命中得分高", () => {
    const atStart = fuzzyMatch("h", "history")!; // 开头
    const inMiddle = fuzzyMatch("h", "fetch")!; // 词中
    expect(atStart.score).toBeGreaterThan(inMiddle.score);
  });
});

describe("rankCommands", () => {
  const cmd = (id: string, title: string, keywords?: string): Command => ({
    id,
    title,
    group: "测试",
    keywords,
    run: () => {},
  });
  const list = [cmd("fetch", "Fetch", "拉取 远程"), cmd("push", "Push", "推送 远程"), cmd("hist", "切换到历史", "history log")];

  it("空 query → 原样返回全部、无高亮", () => {
    const r = rankCommands(list, "  ");
    expect(r.map((x) => x.cmd.id)).toEqual(["fetch", "push", "hist"]);
    expect(r.every((x) => x.indices.length === 0)).toBe(true);
  });

  it("title 命中带高亮下标", () => {
    const r = rankCommands(list, "push");
    expect(r[0].cmd.id).toBe("push");
    expect(r[0].indices.length).toBeGreaterThan(0);
  });

  it("title 不命中但 keywords 命中 → 收录,无高亮", () => {
    const r = rankCommands(list, "推送");
    expect(r.map((x) => x.cmd.id)).toContain("push");
    expect(r.find((x) => x.cmd.id === "push")!.indices).toEqual([]);
  });

  it("title 命中排在 keywords 命中之前", () => {
    // "log" 在 hist 的 keywords 里;给一个 title 含 "lo" 的命令应排更前
    const withTitle = [...list, cmd("clone", "Clone 仓库")];
    const r = rankCommands(withTitle, "lo");
    expect(r[0].cmd.id).toBe("clone"); // title 命中 +100 基线
  });

  it("完全无关 query → 空结果", () => {
    expect(rankCommands(list, "zzzzz")).toEqual([]);
  });
});
