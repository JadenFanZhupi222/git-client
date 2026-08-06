# Agent 下一阶段计划

> 基线：`main` 已具备可用的 GitHub PR AI 评审，包括 DeepSeek 模型选择、语言选择、
> token 预估、文件全选、受预算约束的只读工具调用、结果缓存、人工编辑与确认发布。
> 当前实现说明见 `HANDOFF-pr-review-agent.md`。

## 下一阶段目标

把现有 PR 评审从“单一 Agent 功能”演进成可复用但不过度抽象的 Agent 能力，并以
**GitHub Issue 分诊 Agent**作为第二个真实工作流验证架构。阶段结束时应做到：

1. 同一套运行时可以接入不止一家模型服务，业务代码不出现 provider 分支。
2. PR 评审行为保持兼容，模型切换不改变安全边界、预算和发布规则。
3. Issue 分诊可以生成摘要、分类、优先级、疑似重复项和建议操作。
4. 任何 GitHub 写操作都必须由用户预览并明确确认；模型不能直接改标签、评论、关闭 Issue。

## 不变的安全边界

- 模型输出永远是**提案**，不是权限；外部写操作由应用代码校验并在用户确认后执行。
- PR 评审继续固定到 head SHA；Issue 分诊固定到读取时的 `updated_at` 与评论快照，发布前重新检查。
- 默认工具只读、显式白名单、强预算；任务内容不能新增工具、扩大预算或改变系统指令。
- 凭据只留在 Rust 后端，不返回 WebView；日志不记录密钥、完整提示词、源码、完整模型响应或推理内容。
- 本阶段不授予 shell、本地文件写入、提交、推送、合并、关闭 Issue 等自主权限。
- 所有阻塞型 Git/文件操作继续放入 `spawn_blocking`；上层仍只依赖稳定 trait。

## 架构方向

现有 `ModelProvider`、规范化 turn、工具循环、预算、取消和脱敏 trace 已证明了 provider
边界。下一阶段先在 `review-agent` 内固化契约；只有当 Issue Agent 成为第二个消费者时，
再把已经被两个工作流共同使用的部分提取为 `agent-runtime` crate，避免提前设计一个空泛框架。

目标依赖关系：

```text
PR Review workflow ─┐
                    ├─> agent-runtime ─> ModelProvider ─> provider adapter
Issue Triage workflow┘

provider adapter: DeepSeek / 第二 provider / 后续本地模型
```

`agent-runtime` 只负责：

- 规范化消息、tool call 与最终文本；
- 工具注册、调用循环、去重、预算和取消；
- provider 能力描述、usage 与费用估算数据；
- 稳定错误、有限重试和脱敏 trace。

工作流自行负责领域规则：

- PR Review：GitHub diff、文件读取、行号校验、review 发布；
- Issue Triage：Issue/评论快照、重复项搜索、建议校验、标签/评论发布。

## 实施切片

### A1 · Provider 契约与模型目录

- 为 provider 增加稳定标识和能力描述：tool calling、结构化输出、上下文上限、usage 能力。
- 引入后端拥有的模型目录；前端从 IPC 读取可选 provider/model，不再硬编码模型列表。
- 价格元数据带来源版本和更新时间；UI 明确区分“发送前估算”与“API 返回的实际 usage”。
- 建立 provider 合约测试：同一组规范化工具调用、最终输出、错误与 usage fixture 必须得到相同领域结果。
- 保持 DeepSeek 当前行为不变，先以测试锁住兼容性。

验收：PR 评审现有测试全部通过；UI 仍可选择当前两个 DeepSeek 模型；无 provider 判断进入 orchestrator。

### A2 · 第二 Provider 适配

- 默认候选为 OpenAI，实施前按当时官方 API 文档重新确认接口；选择不得写死到通用运行时。
- adapter 只处理鉴权、请求/响应格式、工具调用编码和 usage 映射。
- 使用 HTTP fixture 覆盖：普通最终输出、连续工具调用、拒绝/限流、截断、无效结构和取消。
- Settings 增加对应凭据状态；凭据仍由 Rust/keyring 管理。
- 模型不可用或能力不匹配时在发送前阻止，不在运行中静默降级或偷换模型。

验收：至少两个 provider 能通过同一套 orchestrator 合约测试；切换 provider 不改变 PR 发布结果结构。

### A3 · GitHub Issue 分诊（只读）

- 新增 Issue 工作区最小入口：Issue 列表、详情和“AI 分诊”。
- 读取工具仅包含当前 Issue、评论、仓库标签以及受限的相似 Issue 搜索。
- 输出固定结构：摘要、类型、优先级、置信度、疑似重复项、建议标签、建议回复、依据。
- 对标签、Issue 编号、引用和建议动作做确定性校验；纯文本 fallback 只能成为摘要，不能产生可执行动作。
- 结果按仓库、Issue 编号和快照版本本地缓存；Issue 更新后标记过期并要求重新分析。

验收：只读分析不产生任何 GitHub 写请求；陈旧快照不会被当成当前结果；无建议时使用紧凑完成态。

### A4 · Issue 分诊人工发布

- 发布前展示将要执行的动作差异：新增/移除标签、评论草稿；默认不选择高风险动作。
- 首版只允许“添加已有标签”和“发表评论”；不自动创建标签、不指派、不关闭、不锁定 Issue。
- 用户逐项选择并确认后，由后端再次校验 Issue 快照，再批量执行允许的动作。
- 部分失败必须返回逐项结果，可安全重试且不得重复评论。

验收：没有明确确认时 GitHub 写调用为零；快照变化返回稳定错误；重试具备幂等保护。

### A5 · 生产化收尾

- 统一运行队列与并发策略：同一资源只允许一个活跃任务，新任务不得覆盖旧结果。
- 只对网络、限流等瞬时错误做有上限且带抖动的重试；无效模型输出不盲目重复消费 token。
- 在 UI 展示阶段、实际 token、估算费用、耗时、取消状态和可分享的脱敏诊断 ID。
- 增加 provider × workflow 契约矩阵、竞态测试、缓存迁移测试和 Tauri 命令集成测试。
- 更新隐私披露：不同工作流分别说明会发送哪些数据，不复用模糊的一次性同意。

## 推荐执行顺序

1. 先做 **A1**，这是下一刀；它只重构契约和模型目录，不增加外部权限。
2. 完成 **A2**，用第二 provider 证明抽象真实可用。
3. 竖切 **A3**，到只读结果可用为止。
4. 单独评审安全与交互后实施 **A4**。
5. 最后用 **A5** 收紧可靠性与可观测性。

每刀都使用独立 `codex/agent-*` 分支，测试通过后 `--no-ff` 合回 `main`；不要让 A3/A4
与 provider 重构同时改同一批核心文件。

## 本阶段明确不做

- 本地开发 Agent、shell/终端执行、文件修改、自动提交或推送；
- 自动 approve/request-changes、自动合并 PR；
- 自动关闭、锁定、指派 Issue，或创建仓库标签；
- 多 Agent 自主协作、长期后台自治和无人工确认的定时任务。

这些能力需要单独的权限模型、工作区隔离、命令沙箱、变更预览与恢复方案，不能扩展现有
PR Review 的只读运行时来“顺便实现”。

## 每个切片的验证门槛

- Rust：`cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、
  `cargo test --workspace`。
- 边界：`powershell -NoProfile -File scripts/check-dependency-boundaries.ps1`。
- 前端：`pnpm -C app test`、`pnpm -C app build`。
- DTO 变更：`cargo test -p ipc-types` 并提交生成后的 TypeScript bindings。
- 涉及 GitHub API 的测试全部使用 fixture；CI 不消耗真实模型 token，也不依赖真实凭据。
- 涉及新写权限的切片必须补“未确认零写入”“快照过期拒绝”“重复提交幂等”测试。
