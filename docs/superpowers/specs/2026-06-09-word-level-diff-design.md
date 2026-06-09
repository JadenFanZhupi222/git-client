# M5.1 词级 diff · 设计

> 里程碑:M5 · 更深的 diff 与历史(第一刀)。
> 目标:在现有 unified DiffView 的增删行里,行内高亮**真正改动的那几个词**,而不是整行红 / 整行绿。
> 日期:2026-06-09。

## 背景与现状

- `DiffView`(`app/src/components/DiffView.tsx`)只有 unified 视图:旧/新双列行号 + 整行增删着色,行内容是纯字符串,无行内细节。
- 行级 diff 的唯一构建点是 `file_diff_from`(`crates/git-engine/src/git2_backend.rs:164`),commit / working / compare 三种 diff 都经它,在此注入即三处一次受益。
- `similar` crate 尚未引入。

## 决策

- **粒度:词级**(`similar::TextDiff::from_words`)。代码场景噪点最少,JetBrains/GitHub 默认接近此。
- **传输形态:传切好的段(segments),不传字节偏移。** Rust 这边把行切成若干段,前端只 map 渲染,不碰任何偏移——绕开「Rust UTF-8 字节 vs JS UTF-16 码元」的跨语言偏移错位坑(含中文/emoji 天然正确)。
- **计算位置:后端**(领域层做脏活、UI 薄)。

## 数据模型(git-core `model/diff.rs`)

```rust
/// 行内一段:text 是原文片段,changed 表示这段相对另一侧是否变化。
pub struct Seg {
    pub text: String,
    pub changed: bool,
}

// DiffLine 新增字段:
pub emphasis: Option<Vec<Seg>>,
//   None      = 无行内细节(上下文行、配不上对的行、整行重写),整行按 kind 着色。
//   Some(segs)= 逐段渲染,changed 段加重高亮。
```

**不变式**:`emphasis` 各段 `text` 顺序拼接 == `content`。`content` 字段保留不动(行级暂存仍依赖它,且作前端兜底)。`Seg` 加 `Serialize/Deserialize`,`DiffLine` 的 `emphasis` 默认 `None`(`FileDiff` 已 `Default`,但 `DiffLine` 是手工构造,新增字段处显式填 `None` 再由标注步骤覆盖)。

## 标注逻辑(git-engine,纯函数)

新增 `fn annotate_word_level(hunks: &mut [Hunk])`(同文件,自由函数,无 IO、不返回 Result)。

在 `file_diff_from` 末尾调用——放在 LFS 处理**之后**,这样 LFS 文件的 hunks 已被清空,标注自动 no-op;`too_large` / `is_binary` 同理(hunks 为空)。

算法:

1. 逐 hunk 处理。在 hunk 的 `lines` 里找「一段连续 Deletion 行」紧跟「一段连续 Addition 行」的配对块。
2. 该块内 `del[i]` 配 `add[i]`,`i` 取到 `min(删行数, 增行数)`;配不上对的多余行 `emphasis` 保持 `None`。
3. 每对调 `TextDiff::from_words(&del.content, &add.content)`:
   - 遍历 `iter_all_changes()`:`Equal` → 两侧都加一段 `changed:false`;`Delete` → 删行加 `changed:true` 段;`Insert` → 增行加 `changed:true` 段。
   - 相邻同 `changed` flag 的段合并(减少段数、利于渲染)。
   - 分别写入 `del.emphasis` / `add.emphasis`。
4. **噪声阈值**:对每对先看 `TextDiff::ratio()`(相似度 0..1),`< 0.25`(基本整行重写)时两行都置 `None`,只留整行红/绿,避免「满行高亮」噪声。

> 注:上下文行(Context)不参与配对,`emphasis` 恒为 `None`。

## 契约层(ipc-types)

- 新增 `SegDto { text: String, changed: bool }` + `From<Seg>`。
- `DiffLineDto` 加 `emphasis: Option<Vec<SegDto>>`,`From<DiffLine>` 透传。
- `app/src/ipc.ts`:`DiffLineDto` 加 `emphasis?: { text: string; changed: boolean }[] | null`,新增对应类型。

## 前端(DiffView)

行渲染处(当前 `app/src/components/DiffView.tsx:120` 的单个内容 `<span>`):

- 若 `l.emphasis` 有值:map 成多个 `<span>`——`changed` 段加重底色(add → `bg-success/30`,del → `bg-danger/30`,比整行底色 `/10` 深一档),非 `changed` 段正常;空文本段跳过。
- 若为 `null`/`undefined`:走现有整行渲染。

颜色只用既有 token,不硬编码 hex。行级暂存的选中/点击交互不变(仍作用于整行)。

## 错误处理

标注是纯字符串计算,无失败路径,原地改 `hunks`。`file_diff_from` 签名不变,仍返回 `Result<FileDiff, GitError>`。

## 测试(竖切各层)

- **git-engine 纯函数单测**(不碰真实 git,毫秒级):
  - `foo bar` → `foo baz`:删段 = [`foo ` 不变, `bar` 变],增段 = [`foo ` 不变, `baz` 变]。
  - 不变式:每行 `emphasis` 段 `text` 拼接 == `content`。
  - 低相似度(如 `aaaa` → `bbbb`,ratio < 0.25)→ 两行 `emphasis` 均为 `None`。
  - 数量不等(3 删 2 增):配对 2 对,多出的 1 删行 `emphasis` 为 `None`。
  - 纯新增 hunk(无删行)/纯上下文:全部 `None`。
- **ipc-types**:`DiffLineDto::from` 携带 `emphasis`(可在现有 DTO 测试里补一条,或纯靠类型 + 下面手验)。
- **前端**:本刀保证 `tsc --noEmit` + `build` 通过 + 手动验证段渲染;vitest 留到 M5 Trustworthy 阶段补。

## 依赖

`crates/git-engine/Cargo.toml` 加 `similar`(版本走 workspace 统一管理,若根 `Cargo.toml` 无则新增 `[workspace.dependencies]` 条目)。

## 竖切顺序(交给 writing-plans 细化)

git-core 模型(加 `Seg` + `emphasis`)→ git-engine(加 `similar` 依赖 + `annotate_word_level` + 接入 `file_diff_from` + 纯函数测试)→ ipc-types DTO → `ipc.ts` 类型 → DiffView 渲染 → `cargo test/clippy/fmt` + `tsc/build` + 手验。

## 明确不做(YAGNI)

- 并排 diff(side-by-side):下一刀 M5.2,本刀只做 unified 内的行内高亮(但 segments 数据天然为其复用)。
- 字符级 / 词边界收拢混合法:词级足够,复杂度不值当。
- 跨行的移动检测(moved block):超出本刀范围。
