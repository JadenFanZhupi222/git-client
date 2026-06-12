# M5.3 文件历史(file history)· 设计

> 里程碑:M5 · 更深的 diff 与历史(第三刀)。
> 目标:看「某个文件的提交历史」——`git log --follow -- <path>`,跟随重命名,列出动过该文件的提交,点提交看该文件在那次提交的 diff。
> 日期:2026-06-12。

## 背景与现状

- 现有 `log` / `search_commits` 用 git2 revwalk(`crates/git-engine/src/git2_backend.rs`),返回 `Vec<Commit>`。
- 文件历史本质是带路径过滤的 log。关键差异是**重命名跟随(`--follow`)**:JetBrains 文件历史能穿过 rename 看到文件改名前的历史。**git2/libgit2 原生不支持 `--follow`**(要手写 rename 检测,复杂易错)。
- 项目铁律:复杂/需贴合 git 行为的走 `CliBackend`。`git log --follow` 正是此类。
- `Commit` 模型 / `CommitDto` 已存在,**本刀无需新增 DTO**。

## 决策

- **走 CLI 后端**:`git log --follow -n<limit> --format=<机器可读> -- <file>`,免费拿到重命名跟随 + 与 git 完全一致语义。
- **机器可读 format + 健壮解析**:字段间用 `%x1f`(0x1F 单元分隔)、提交间用 `%x1e`(0x1E 记录分隔)。body 含换行也不会错位(不靠行分隔)。
  - format:`%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%s%x1f%b%x1e`(id / 父 / 作者名 / 作者邮箱 / 作者时间戳 / summary / body)。
- **不做缓存 / 取消**:文件历史按需触发(打开面板时一次)、结果被 limit 截断、CLI 子进程返回快。YAGNI,先不引入 `cancelled` 回调和 LRU。
- **UI:双栏覆盖面板** `FileHistoryPanel`(仿 ReflogPanel overlay):左侧该文件的提交列表,选中提交 → 右侧用 `DiffView` 显示该文件在那次提交的 diff(复用 `commit_file_diff` / `useCommitDiff`,自动吃到 M5.2 并排视图)。
- **入口:文件右键**。CommitFileList 行加右键菜单「查看文件历史」,HistoryView 持 `historyFile` 状态并渲染面板。

## 竖切各层

1. **git-core**(`backend.rs`):trait 加默认方法
   ```rust
   /// 某文件的提交历史(git log --follow -- <file>),时间倒序,最多 limit 条。跟随重命名。
   /// 文件无历史/不存在 → 空。默认 Unsupported。
   fn file_history(&self, _repo: &Path, _file: &str, _limit: usize) -> Result<Vec<Commit>, GitError> {
       Err(GitError::Unsupported)
   }
   ```
2. **git-engine cli_backend**:
   - 纯函数 `parse_log_records(stdout: &[u8]) -> Vec<Commit>`(按 0x1e 切记录、0x1f 切字段,健壮容错)。
   - `CliBackend::file_history` 跑 git、非零退出转 `GitError::Backend`、成功交给解析函数。
   - **tempfile 真实仓库测试**:① 改两次的文件 → 2 条、新→旧;无关文件不混入。② 文件 rename 后 → `--follow` 仍返回改名前历史。③ 不存在的路径 → 空。④ 纯函数单测:含换行 body 不错位、父字段空格分割。
3. **composite**:`file_history` 路由到 `self.cli`。
4. **fake.rs**:trait 默认 Unsupported 即可,无需实现(app-service 不为它单测,验证靠 git-engine tempfile 测)。
5. **app-service**:`RepoService::file_history`(`lib.rs`,map 成 DTO)+ `RepoContext::file_history`(`repo_context.rs`,无缓存直透 service)。
6. **src-tauri**:`#[tauri::command] async fn file_history(...)`(`spawn_blocking` + `to_ipc`)+ `generate_handler!` 注册。
7. **ipc.ts**:`getFileHistory(repoPath, file, limit): Promise<CommitDto[]>`。
8. **queries.ts**:`qk.fileHistory` key + `useFileHistory(repo, file)` hook(`enabled: !!file`)。
9. **UI**:`FileHistoryPanel.tsx`(双栏 overlay)+ CommitFileList 右键菜单 + HistoryView 接线。

## 错误处理

- git 非零退出 → `GitError::Backend(stderr)`(穷尽 match 的 `to_ipc` 已有 `Backend` arm,无需改)。
- 解析永不失败(纯字符串,容错取 `unwrap_or`);id 为空的记录跳过。

## 测试

- 后端:cli_backend tempfile 测试(见上)+ 纯函数单测。`cargo test --workspace` 全绿、`clippy` 零警告、`fmt`。
- 前端:`tsc --noEmit` 干净、`pnpm build` 通过 + 手验(右键打开面板、列表、选提交看 diff、rename 文件能看到旧历史)。

## 明确不做(YAGNI)

- 行历史(`log -L`)= M5.4、pickaxe(`-S`/`-G`)= M5.5,本刀不碰。
- 文件历史里的 cherry-pick/revert 等 per-commit 操作:面板先只读(看历史 + diff),操作入口后续再说。
- 分页 / 取消 / 缓存:文件历史短、按需触发,先不做。
