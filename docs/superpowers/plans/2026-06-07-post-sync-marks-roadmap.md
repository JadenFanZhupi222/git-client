# 计划:未push标记之后的功能路线(2026-06-07)

> 背景:未push/未pull 图谱标记已完成并合入 main(commit e4451b9)。
> 目标:继续向「世界第一的 git 工具(对标 JetBrains)」补齐高价值缺口。
> 执行原则:每个功能走既有竖切(trait→git2/cli→composite→fake→ipc-types→service→src-tauri→ipc.ts→UI),
> 每刀 feat 分支 → `--no-ff` 合 main → 删分支;push 到 origin 等用户发话。

## 缺口评估(按价值/风险排序)

| 功能 | 价值 | 风险 | 备注 |
|---|---|---|---|
| Revert(回滚提交) | 高 | 低 | 几乎照搬 cherry-pick;RepoState::Reverting 已就绪 |
| 日志搜索/过滤 | 很高 | 中 | 图谱与过滤不兼容 → 搜索时切「扁平列表模式」 |
| 创建/删除 tag | 中 | 低 | 图谱已渲染 tag 徽章,只差写操作 |
| 任意两提交/分支 diff 比较 | 高 | 中 | 新 UI 流程 |
| reset(soft/mixed/hard) | 中 | 中 | hard 破坏性,要二次确认 |
| 交互式 rebase | 很高 | 高 | 阶段4,工程量大,留后 |

## 本轮执行(自动,不停)

### 切片 1:Revert(回滚提交)—— 镜像 cherry-pick
- `git-core/backend.rs`:trait 加 `revert(repo, commit_id)` 默认 Unsupported。
- `cli_backend.rs`:`revert` 方法 `run_op(&["revert", commit_id])`(continue/abort 的 Reverting 分支已存在)。
- `composite.rs`:委托 cli。`fake.rs`:记录 `revert:{id}`。
- `app-service`:`revert` 用例(空 id 拦截)+ FakeBackend 测试。
- `src-tauri`:`revert` 命令(spawn_blocking + to_ipc)+ 注册。
- `ipc.ts`:`revert(repoPath, commitId)`。
- `HistoryView`:提交详情头加「Revert」按钮(挨着 Cherry-pick),冲突 → 进入 reverting,「更改」页解决(复用现有冲突闭环 + ConflictBanner 的 reverting 文案)。
- 测试:cli tempfile(正常回滚 / 冲突报 MergeConflict)+ app-service Fake。

### 切片 2:日志搜索/过滤 —— 扁平列表模式
- `git-core/backend.rs`:trait 加 `search_commits(repo, query, limit)` 默认走 log 客户端过滤;git2 实现:revwalk 从 HEAD,大小写不敏感匹配 summary/body/author.name/author.email/SHA 前缀,收集到 limit。
- `composite`/`fake` 透传。`ipc-types`:复用 CommitDto。
- `app-service`:`search_commits` 用例(空 query → 返回空)+ Fake 测试。
- `src-tauri`:`search_commits` 命令 + 注册。`ipc.ts`:`searchCommits`。
- `lib/queries.ts`:`useCommitSearch(repo, query)`(enabled = query 非空,debounce 由组件处理)。
- `HistoryView`:图谱列顶部加搜索框;query 非空时左栏切「扁平匹配列表」(单列圆点 + 提交信息,复用行渲染,无 lane),清空回到图谱。选中项照常驱动中/右栏。

## 之后候选(本轮不做)
创建/删除 tag、两分支 diff 比较、reset、交互式 rebase。
