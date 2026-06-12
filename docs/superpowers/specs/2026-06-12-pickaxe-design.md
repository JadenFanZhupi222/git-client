# M5.5 pickaxe(按内容搜历史)· 设计

> 里程碑:M5 · 更深的 diff 与历史(第五刀,收尾)。
> 目标:按「代码内容」搜历史——哪次提交引入/删除了某段文本(`git log -S`),或哪次提交的 diff 改动行匹配某正则(`git log -G`)。
> 日期:2026-06-12。

## 背景与现状

- 现有 `search_commits`(git2 revwalk)搜的是**提交信息 / 作者 / SHA**,搜不到「代码内容」。pickaxe 搜的是 **diff 内容**,是另一种能力。
- pickaxe 结果就是一串提交(点开看 diff 即可),故返回 `Vec<Commit>` —— 与 search 完全同形,能复用 HistoryView 搜索结果列表 + 点选提交看 diff 的整套流程。
- 后端与 M5.3 file_history 几乎同构:跑 `git log` + 复用 `LOG_FORMAT` / `parse_log_records`。

## 决策

- **走 CLI 后端**:`git log -S<q>` / `-G<q>`(git2 无 pickaxe)。复用 M5.3 的 `LOG_FORMAT` + `parse_log_records`。
- **两种模式**(一个 `regex: bool` 参数):
  - `regex=false` → `-S<q>`:某字符串**出现次数变化**的提交(= 引入或删除了该串)。字面量,适合「这个函数名/常量哪来的」。
  - `regex=true` → `-G<q>`:diff 改动行**匹配正则**的提交。适合按模式找。
- **范围**:全仓库(不接 path 过滤)。YAGNI——「这段代码哪来的」通常不预先知道文件;按需再加 path。
- **入口:接进 HistoryView 现有搜索栏**。加一个三态模式切换「信息 / 内容 / 正则」:
  - 「信息」= 现有 `search_commits`(默认,行为不变)。
  - 「内容」= pickaxe `-S`、「正则」= pickaxe `-G`。
  - 三者结果都是 `CommitDto[]`,共用现有 `SearchList` 渲染 + 点选→ MidColumn 看 diff。零新结果 UI。
- **不做后端取消**:CLI 子进程、输入有 300ms debounce、limit 截断,够用(search_commits 的代次取消不强行套到 pickaxe)。

## 竖切各层

1. **git-core** `backend.rs`:trait 加
   ```rust
   /// pickaxe:按 diff 内容搜提交。regex=false→`-S<q>`(出现次数变化);regex=true→`-G<q>`(改动行匹配正则)。
   fn pickaxe(&self, _repo, _query: &str, _regex: bool, _limit: usize) -> Result<Vec<Commit>, GitError> { Err(Unsupported) }
   ```
2. **git-engine cli_backend**:`CliBackend::pickaxe` 跑 `git log <-S|-G><q> -n<limit> --format=LOG_FORMAT`;复用 `parse_log_records`。**tempfile 测试**:`-S` 命中引入某串的提交、删除该串的提交也算;`-G` 正则匹配;不命中为空。
3. **composite**:路由到 `self.cli`。
4. **app-service**:`RepoService::pickaxe`(map DTO,空 query → 空)+ `RepoContext::pickaxe`(不缓存)。
5. **src-tauri**:`pickaxe` 命令(`spawn_blocking` + `to_ipc`)+ 注册。
6. **ipc.ts**:`pickaxe(repo, query, regex, limit): Promise<CommitDto[]>`。
7. **queries.ts**:`usePickaxe(repo, query, regex, limit)`(`enabled` 仅当 query 非空)。
8. **UI**:HistoryView 加 `searchMode: "message"|"content"|"regex"` 状态;按模式选 `useCommitSearch` 或 `usePickaxe` 喂同一个 `searchResults`;搜索栏下加一行三态 chips + 按模式换 placeholder。

## 错误处理

- git 非零退出 → `GitError::Backend`(`to_ipc` 已有 arm)。`-G` 正则非法时 git 报错 → 走 Backend,前端红字提示。

## 测试

- 后端:cli_backend tempfile 测试;`cargo test/clippy/fmt` 全绿。
- 前端:`tsc` + `pnpm build` + 手验(三模式切换、-S 找引入/删除、-G 正则、点结果看 diff、非法正则报错)。

## 明确不做(YAGNI)

- path 限定的 pickaxe、`--pickaxe-regex`(-S 走正则)、`--all`(搜所有分支):先不做。
- 高亮命中的具体行:结果点开看整提交 diff 即可(M5 后续可加)。
- M5 至此(5.1–5.5)收口;图片 diff 另列。
