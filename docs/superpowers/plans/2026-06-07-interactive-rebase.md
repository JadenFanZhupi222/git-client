# 计划:交互式 rebase(2026-06-07)

> 阶段 4 招牌特性。对标 JetBrains「Interactively Rebase from Here」。
> 决策:支持全集操作(pick/reword/squash/fixup/drop/重排序);排序用上下箭头(MVP)。

## 核心思路:全程非交互驱动 CLI

`git rebase -i` 默认弹编辑器(改 todo、改 squash/reword 信息)。本方案全程零弹窗:

1. **todo 列表自己生成**,通过 `GIT_SEQUENCE_EDITOR` 注入:
   写 todo 到临时文件 T,设 `GIT_SEQUENCE_EDITOR='cp "<T 正斜杠路径>"'`,
   git 实际执行 `cp <T> <git-rebase-todo>`(Git 自带 sh/coreutils,跨平台)。
   同时设 `GIT_EDITOR=true` 兜底(意外弹编辑器都 no-op 成功)。

2. **改信息用 exec 行**,不弹编辑器:
   - reword(m):`pick <sha>` + `exec git commit --amend -F <msgfile>`
   - squash(m):`fixup <sha>` + `exec git commit --amend -F <msgfile>`(设合并信息)
   - fixup:`fixup <sha>`(并入前一个、丢信息,无 exec)
   - pick:`pick <sha>`;drop:不输出该行

3. **冲突**:rebase 冲突 → repo_state=Rebasing → 复用现有 ConflictBanner(继续/中止)+ Changes 页解决。

## 竖切
- git-core:model RebaseAction/RebaseStep;trait interactive_rebase(repo, base:Option<&str>, steps) 默认 Unsupported。
- cli_backend:生成 todo+msg 临时文件、设 env 跑 rebase -i;冲突→MergeConflict。
- composite 委托 cli;fake 记录。
- app-service:校验(首个非 drop 不能 fixup/squash)+ 用例。
- src-tauri:命令 + RebaseStepInput 映射;无新 GitError 变体。
- 前端:ipc + RebaseEditor 弹层(↑↓ 调序、操作下拉、reword/squash 信息)+ 历史页「交互式变基」按钮。

## 边界
- 首个非 drop 步骤不能 fixup/squash;全 drop 视 noop(UI 留一个 pick);root 提交 base=None→--root;
  改写历史警告;已 push 的需 force push(仅提示)。cp/sh 依赖 Git 自带(本机 tempfile 测试验证)。
