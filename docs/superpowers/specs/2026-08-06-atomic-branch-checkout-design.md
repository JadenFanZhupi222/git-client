# 原子化分支切换设计

## 问题

`Git2Backend::checkout_branch` 当前先调用 `checkout_tree` 更新工作区和 index，再调用
`set_head` 移动 HEAD。当目标分支已被另一个 worktree 占用时，前一步成功、后一步失败，
导致当前分支名称不变，但工作区和 index 已变成目标分支内容。

## 目标

- 分支切换成功时，HEAD、index 和工作区共同切换到目标分支。
- 任意预检失败时，HEAD、index 和工作区保持调用前状态。
- 保留现有安全 checkout 行为：可能覆盖本地修改时返回 `CheckoutConflict`。
- 不改变 `GitBackend` trait、app-service、IPC 或前端接口。

## 方案

在写入工作区前完成两类预检：

1. 检查目标本地分支是否已被同一仓库的其他 worktree 占用；若占用，返回后端错误，
   不执行 checkout。
2. 继续使用现有 libgit2 safe checkout 验证工作区和 index 是否可以切换；冲突继续映射为
   `CheckoutConflict`。

已复现的半切换只发生在目标分支被其他 worktree 占用时：`checkout_tree` 成功后，`set_head`
才报告占用错误。占用预检将该失败提前到任何写入之前，因此无需增加会覆盖用户本地修改风险更高的
补偿式回滚。

## Worktree 占用检测

通过仓库的 worktree 元数据枚举 linked worktrees，读取每个 worktree 的 HEAD，比较其符号引用
是否等于目标 `refs/heads/<name>`。当前 worktree 不视为冲突。该检查必须发生在任何工作区写入前。

## 测试

新增真实临时仓库回归测试：

1. 创建 `main` 和 `dev`，两者文件内容不同。
2. 为 `dev` 创建 linked worktree。
3. 在主 worktree 调用 `checkout_branch("dev")` 并断言失败。
4. 断言主 worktree 的当前分支、文件内容和 index 均与调用前一致。

继续运行现有 checkout 测试，确保正常切换、缺失分支和脏工作区冲突行为不变。

## 非目标

- 不增加“强制切换”或自动 stash。
- 不改变 worktree 管理 UI。
- 不修改 PR #2 或 PR 工作区设计。
