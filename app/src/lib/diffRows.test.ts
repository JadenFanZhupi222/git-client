import { describe, it, expect } from "vitest";
import {
  collapseContext,
  buildSbsRows,
  buildDiffRows,
  maxContentCols,
  maxSideCols,
  toRefs,
  type DiffChunk,
} from "./diffRows";
import { type DiffLineDto, type FileDiffDto, type HunkDto } from "../ipc";

// diff 行折叠 + 并排配对纯逻辑回归。
// collapseContext 锁:保留下标、顶住 ctx 行、隐藏阈值、首尾单侧、多改动块。
// buildSbsRows 锁:context 同行、连续删/增配对、不等数量留空、原始 li 透传。

// 简写造行:c=context, a=add, d=del。content 用占位,折叠只看 kind。
const c = (): DiffLineDto => ({ kind: "context", old_lineno: 0, new_lineno: 0, content: "x" });
const a = (): DiffLineDto => ({ kind: "add", old_lineno: null, new_lineno: 0, content: "+" });
const d = (): DiffLineDto => ({ kind: "del", old_lineno: 0, new_lineno: null, content: "-" });

/** 把 chunk 列表压扁成便于断言的形状:["lines:3", "fold:5", ...]。 */
function shape(chunks: DiffChunk[]): string[] {
  return chunks.map((ch) => `${ch.kind}:${ch.refs.length}`);
}

/** 所有 chunk 的 refs 摊平后,原始下标必须是 0..n-1 连续不漏(折叠不丢行)。 */
function flatLis(chunks: DiffChunk[]): number[] {
  return chunks.flatMap((ch) => ch.refs.map((r) => r.li));
}

describe("collapseContext", () => {
  it("无 context 时不折叠(全是改动)", () => {
    const chunks = collapseContext([d(), a(), d()]);
    expect(shape(chunks)).toEqual(["lines:3"]);
  });

  it("改动块间的长 context:保留上下各 ctx 行,中间折叠", () => {
    // d, [10 个 context], a   →  d + 3ctx | fold(4) | 3ctx + a
    const lines = [d(), ...Array.from({ length: 10 }, c), a()];
    const chunks = collapseContext(lines, { ctx: 3 });
    expect(shape(chunks)).toEqual(["lines:4", "fold:4", "lines:4"]);
    // 行不丢:0..11 连续
    expect(flatLis(chunks)).toEqual(Array.from({ length: 12 }, (_, i) => i));
  });

  it("context 太短(隐藏 < minFold)则整段保留不折", () => {
    // d, [6 个 context], a:保留 3+3=6,隐藏 0 → 不折
    const lines = [d(), ...Array.from({ length: 6 }, c), a()];
    expect(shape(collapseContext(lines, { ctx: 3 }))).toEqual(["lines:8"]);
    // d, [7 个 context], a:隐藏 1 < 默认 minFold 2 → 不折
    const lines2 = [d(), ...Array.from({ length: 7 }, c), a()];
    expect(shape(collapseContext(lines2, { ctx: 3 }))).toEqual(["lines:9"]);
    // 隐藏 2 = minFold → 折
    const lines3 = [d(), ...Array.from({ length: 8 }, c), a()];
    expect(shape(collapseContext(lines3, { ctx: 3 }))).toEqual(["lines:4", "fold:2", "lines:4"]);
  });

  it("hunk 顶部 context(前面无改动)只在下侧顶 ctx 行,顶部全折", () => {
    // [10 context], a:keepStart=0, keepEnd=3 → fold(7) | 3ctx + a
    const lines = [...Array.from({ length: 10 }, c), a()];
    const chunks = collapseContext(lines, { ctx: 3 });
    expect(shape(chunks)).toEqual(["fold:7", "lines:4"]);
    expect(flatLis(chunks)).toEqual(Array.from({ length: 11 }, (_, i) => i));
  });

  it("hunk 尾部 context(后面无改动)只在上侧顶 ctx 行,尾部全折", () => {
    // d, [10 context]:keepStart=3, keepEnd=0 → d + 3ctx | fold(7)
    const lines = [d(), ...Array.from({ length: 10 }, c)];
    const chunks = collapseContext(lines, { ctx: 3 });
    expect(shape(chunks)).toEqual(["lines:4", "fold:7"]);
  });

  it("多个改动块各自的 context 分别折叠", () => {
    // d [8c] a [8c] d
    const lines = [d(), ...Array.from({ length: 8 }, c), a(), ...Array.from({ length: 8 }, c), d()];
    const chunks = collapseContext(lines, { ctx: 3 });
    // 中段 = 上块尾 3ctx + a + 下块头 3ctx = 7
    expect(shape(chunks)).toEqual(["lines:4", "fold:2", "lines:7", "fold:2", "lines:4"]);
    expect(flatLis(chunks)).toEqual(Array.from({ length: lines.length }, (_, i) => i));
  });

  it("ctx=0:context 全折(只要够 minFold)", () => {
    const lines = [d(), ...Array.from({ length: 4 }, c), a()];
    expect(shape(collapseContext(lines, { ctx: 0 }))).toEqual(["lines:1", "fold:4", "lines:1"]);
  });

  it("折叠的 fold 段 refs 携带正确的原始下标", () => {
    const lines = [d(), ...Array.from({ length: 8 }, c), a()];
    const chunks = collapseContext(lines, { ctx: 3 });
    const fold = chunks.find((ch) => ch.kind === "fold")!;
    // d=0, context 1..8, 保留 1..3 与 6..8,折叠 4..5
    expect(fold.refs.map((r) => r.li)).toEqual([4, 5]);
  });
});

describe("buildSbsRows", () => {
  it("纯 context 行左右同行", () => {
    const rows = buildSbsRows(toRefs([c(), c()]));
    expect(rows).toHaveLength(2);
    expect(rows[0].left?.li).toBe(0);
    expect(rows[0].right?.li).toBe(0);
  });

  it("等量删/增成对:del[i] 配 add[i]", () => {
    const rows = buildSbsRows(toRefs([d(), d(), a(), a()]));
    expect(rows).toHaveLength(2);
    expect(rows[0].left?.li).toBe(0);
    expect(rows[0].right?.li).toBe(2);
    expect(rows[1].left?.li).toBe(1);
    expect(rows[1].right?.li).toBe(3);
  });

  it("删多增少:多余删行右侧留空", () => {
    const rows = buildSbsRows(toRefs([d(), d(), d(), a()]));
    expect(rows).toHaveLength(3);
    expect(rows[2].left?.li).toBe(2);
    expect(rows[2].right).toBeNull();
  });

  it("纯新增:左侧全空", () => {
    const rows = buildSbsRows(toRefs([a(), a()]));
    expect(rows.every((r) => r.left === null)).toBe(true);
    expect(rows.map((r) => r.right?.li)).toEqual([0, 1]);
  });

  it("context 夹在改动间:顺序与下标保持", () => {
    const rows = buildSbsRows(toRefs([c(), d(), a(), c()]));
    expect(rows.map((r) => [r.left?.li ?? null, r.right?.li ?? null])).toEqual([
      [0, 0],
      [1, 2],
      [3, 3],
    ]);
  });
});

// 造一个带内容长度的行,验证宽度统计。
const line = (kind: string, content: string): DiffLineDto => ({
  kind,
  old_lineno: kind === "add" ? null : 1,
  new_lineno: kind === "del" ? null : 1,
  content,
});
const hunk = (lines: DiffLineDto[]): HunkDto => ({ header: "@@ -1 +1 @@", lines });
const fileDiff = (hunks: HunkDto[]): FileDiffDto => ({
  path: "f",
  is_binary: false,
  too_large: false,
  is_lfs_pointer: false,
  lfs_size: "",
  is_image: false,
  old_image: null,
  new_image: null,
  hunks,
});

describe("buildDiffRows", () => {
  it("每个 hunk 先出一条 hunk 头", () => {
    const diff = fileDiff([hunk([line("add", "x")]), hunk([line("del", "y")])]);
    const rows = buildDiffRows(diff, new Set(), "unified");
    expect(rows.filter((r) => r.kind === "hunk")).toHaveLength(2);
    expect(rows[0]).toMatchObject({ kind: "hunk", hi: 0 });
  });

  it("unified:折叠块未展开出 fold 项,展开后出 uline 行", () => {
    // 一段够长的 context 会折叠
    const lines = [line("del", "d"), ...Array.from({ length: 8 }, () => line("context", "c")), line("add", "a")];
    const diff = fileDiff([hunk(lines)]);
    const collapsed = buildDiffRows(diff, new Set(), "unified");
    const fold = collapsed.find((r) => r.kind === "fold");
    expect(fold).toBeTruthy();
    // 展开该 fold 后,fold 项消失,行变多
    const expanded = buildDiffRows(diff, new Set([fold!.kind === "fold" ? fold!.foldKey : ""]), "unified");
    expect(expanded.find((r) => r.kind === "fold")).toBeUndefined();
    expect(expanded.filter((r) => r.kind === "uline").length).toBeGreaterThan(
      collapsed.filter((r) => r.kind === "uline").length,
    );
  });

  it("split:行出 pair 项,不出 uline", () => {
    const diff = fileDiff([hunk([line("del", "d"), line("add", "a")])]);
    const rows = buildDiffRows(diff, new Set(), "split");
    expect(rows.some((r) => r.kind === "pair")).toBe(true);
    expect(rows.some((r) => r.kind === "uline")).toBe(false);
  });
});

describe("宽度统计", () => {
  it("maxContentCols 取所有行最长内容", () => {
    const diff = fileDiff([hunk([line("context", "ab"), line("add", "abcde"), line("del", "abc")])]);
    expect(maxContentCols(diff)).toBe(5);
  });

  it("maxSideCols:左算删+context,右算增+context", () => {
    const diff = fileDiff([
      hunk([line("del", "delxx"), line("add", "ad"), line("context", "ctx")]),
    ]);
    // 左:del(5) / context(3) → 5;右:add(2) / context(3) → 3
    expect(maxSideCols(diff)).toEqual({ left: 5, right: 3 });
  });
});
