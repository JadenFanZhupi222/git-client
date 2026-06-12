# M5.4 行历史(line history)· 设计

> 里程碑:M5 · 更深的 diff 与历史(第四刀)。
> 目标:看「某文件某几行」的演变史——`git log -L<start>,<end>:<file>`,列出动过这几行的提交,每条直接给出那几行在该提交的 diff hunk。
> 日期:2026-06-12。

## 背景与现状

- M5.3 文件历史给了「动过该文件的提交列表 + 整文件 diff」。行历史更聚焦:**只看选中的几行**怎么变的(「这个函数最后是谁、为什么改的」)。
- `git log -L<a>,<b>:<file>` 输出 = 每个提交的元数据 + **仅该行范围的 diff hunk**,二者交织。比普通 log 多带 diff,故不能只返回 `Vec<Commit>`。
- 入口需要一个「行范围」。BlameView 按当前文件行号逐行展示,是选行的最自然处。

## 决策

- **忠实呈现(非仅过滤列表)**:返回 `Vec<LineHistoryEntry { commit, diff }>`,右侧只显示那几行的演变 hunk,而不是整文件 diff。这才是行历史的精髓。
- **走 CLI 后端**:`git log -L<start>,<end>:<file> --format=<marker fmt>`。`-L` git2 无对应能力。
- **健壮解析**:format 以 `%x1e`(0x1E 记录分隔)起头、字段间 `%x1f`(0x1F)。0x1E 在源码/diff 内容里几乎不可能出现 → 按它切提交块,绕开「marker 撞到 diff 行内容」。
  - format:`%x1e%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%s`(id/父/作者名/邮箱/时间戳/summary)。**不含 body**(多行会破坏「首行=元数据、其后=diff」的切分)。
  - 每块:首行按 0x1f 切元数据,其后是该提交的 unified diff 文本。
- **复用 DiffView**:把 `-L` 的 diff 文本解析成现有 `FileDiff` 模型(`Hunk`/`DiffLine`),前端用 `DiffView` 渲染(自动吃到 M5.2 并排)。需要一个 **unified diff 文本 → FileDiff** 纯函数解析器(约 40 行,可复用)。

## 数据模型(git-core `model/diff.rs`)

```rust
/// 行历史的一条:某提交 + 它对选中行范围的 diff(仅范围 hunk)。
pub struct LineHistoryEntry {
    pub commit: Commit,
    pub diff: FileDiff,
}
```

## 解析器(git-engine cli_backend,纯函数)

- `parse_unified_diff(text: &str) -> FileDiff`:逐行扫。`@@ -a,b +c,d @@` 起新 hunk 并从 a/c 初始化 old/new 行号;` `→Context(old/new 都给、各自++)、`+`→Addition(new 给、new++)、`-`→Deletion(old 给、old++);`diff --git`/`---`/`+++`/`\ No newline` 行在首个 `@@` 前或识别后跳过。`emphasis` 全 `None`(行历史不做词级)。
- `parse_line_log(stdout: &[u8]) -> Vec<LineHistoryEntry>`:按 0x1E 切块;每块首行(到首个 `\n`)按 0x1F 切元数据建 `Commit`,其后文本交给 `parse_unified_diff`。

## 竖切各层

1. **git-core**:`model/diff.rs` 加 `LineHistoryEntry`(+ mod 导出);`backend.rs` trait 加 `line_history(repo, file, start: u32, end: u32) -> Result<Vec<LineHistoryEntry>, GitError>` 默认 Unsupported。
2. **git-engine cli_backend**:两个纯函数 + `CliBackend::line_history`(跑 `git log -L`,非零退出转 `Backend`)。**tempfile 测试**:行被改两次 → 2 条、新→旧、各带正确 hunk;纯函数测 unified diff 解析(含创建提交的 `/dev/null`、单行 `@@ -2 +2 @@` 省略计数)。
3. **composite**:路由到 `self.cli`。
4. **ipc-types**:`LineHistoryEntryDto { commit: CommitDto, diff: FileDiffDto }` + `From`。
5. **app-service**:`RepoService::line_history`(map DTO)+ `RepoContext::line_history`(不缓存)。
6. **src-tauri**:`line_history` 命令(`spawn_blocking` + `to_ipc`)+ 注册。
7. **ipc.ts**:`LineHistoryEntryDto` 类型 + `getLineHistory(repo, file, start, end)`。
8. **queries.ts**:`useLineHistory(repo, file, start, end)`(`enabled` 仅当 file 且 start>0)。
9. **UI**:
   - `LineHistoryPanel`:双栏 overlay,左提交列表(每条 commit 元数据)、右选中条的 `entry.diff`(直接渲染,不再单独查 diff)。
   - **BlameView 选行**:点行选中(anchor=focus),shift-点扩成范围;选中行高亮;出现 sticky 操作条「查看第 a–b 行历史」→ 打开面板。

## 错误处理

- git 非零退出 → `GitError::Backend(stderr)`(`to_ipc` 已有 arm)。解析永不失败(容错取默认)。

## 测试

- 后端:cli_backend tempfile + 纯函数单测;`cargo test/clippy/fmt` 全绿。
- 前端:`tsc` + `pnpm build` + 手验(blame 选行/扩范围、面板列表、右侧范围 hunk、并排切换)。

## 明确不做(YAGNI)

- 词级 emphasis 进行历史 hunk:先不做(`None` 整行着色)。
- 多段范围 / 跨重命名的 `-L`(git `-L` 对 rename 支持有限):本刀单段范围、不显式跟 rename。
- 从 DiffView 直接选行进入:本刀入口只在 blame(行号语义最干净)。
- pickaxe(`-S`/`-G`)= M5.5。
