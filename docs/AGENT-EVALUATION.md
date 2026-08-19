# Agent 稳定性与 Token 评测基线

## 目的

这套基线回答两个问题：Agent 是否在发起 Provider 请求前正确执行预算控制，以及上下文、
工具循环、compactor、verifier 和重启恢复是否保持可预测的 usage 语义。它不评价模型答案的
主观质量，也不替代真实桌面交互验收。

评测位于 `crates/agent-session/tests/evaluation_baseline.rs`，只使用固定输入、临时工作区和
fake provider。默认 CI 不访问网络、不读取 API key，也不消耗真实模型额度。

## 运行

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-agent-evals.ps1
```

该命令会输出一行 `AGENT_EVAL_REPORT={...}`。测试同时包含硬性断言，因此
`cargo test --workspace` 已自动覆盖这套基线。

## 固定场景

| 场景 | 关键断言 | 当前固定报告 |
| --- | --- | --- |
| 长上下文规划 | 估算输入不得超过 6976 Token，且必须形成有界 memory working set | 16 条历史压缩为 11 个 transcript item，估算 6071 Token |
| 预算前置拦截 | 零剩余预算必须在 Provider I/O 前 checkpoint | 0 请求、0 round、0 usage |
| 重复工具批次 | 三次相同证据后停止继续调用工具，进入 tool-free synthesis | 4 请求、3 tool calls、最后请求无工具 |
| working-set compaction | 仅压缩较旧批次，保留最近两个完整工具批次 | 1 请求；90 input / 30 cached / 20 output |
| verifier 修复 | 无效 JSON 只修复一次，并累计两次 Provider usage | 2 请求；21 input / 3 cached / 6 output |
| 重启恢复 | 状态转换不得触发 Provider 请求，usage 必须保留 | 0 请求；保留 123 input；revision +1 |

报告中的 token 数来自 fake provider 或保守 estimator，不代表任一家真实 Provider 的 tokenizer。
固定数字用于发现代码路径和记账语义变化；真实成本仍以 Provider 返回 usage 和模型价格快照为准。

## 回归规则

- 预算不足、重启恢复以及纯状态转换的 Provider 请求数必须为零。
- Provider 返回的 input、cached input 和 output usage 必须分别 checked 累计，禁止饱和或遗漏。
- verifier 最多执行初次请求和一次 contract repair；修复 usage 必须计费。
- compactor 预算不足时必须跳过 Provider I/O；执行时必须计入独立 usage。
- 重复工具批次达到阈值后，后续请求必须禁用工具，防止无进展继续消耗预算。
- context planner 的估算输入必须处于 provider window 扣除 output reserve 和 safety margin 后的预算内。

评测数值如因 prompt、schema、estimator 或策略的有意调整而变化，应在同一提交中说明原因并更新
本文件。不得为了通过基线简单放宽预算或删除安全余量。

## 真实 Provider 人工样本

真实 Provider 评测不进入 CI。需要判断 Token 是否仍偏高时，对同一组脱敏任务分别运行至少三次，
记录 model ID、input/cached/output token、Provider 请求数、compaction 次数、verifier 次数、完成状态、
耗时和费用。比较中位数，不比较单次随机波动。若没有质量提升但输入或费用中位数持续上升 15% 以上，
或 verifier repair 经常出现，再检查 prompt、context planner 和 compaction；不要先更换框架。

## 桌面端闭环检查表

- 创建 Goal 后导航离开再返回，仍显示同一 authoritative snapshot。
- steering 在当前 Goal 的安全边界注入，不创建第二个并发 Goal。
- 读工具直接运行；写工具必须显示脱敏审批，允许和拒绝都只生效一次。
- Pause 在安全 checkpoint 停止；Resume 从同一 checkpoint 继续。
- 预算耗尽时不再请求 Provider；扩展只能增加 limit，扩展后可继续。
- 应用重启后 Goal 显示 `paused/app_restarted`，未点击 Resume 前无 Provider 或 effect 工作。
- workspace digest 冲突和 ambiguous mutation 显示稳定 block reason，不自动重放写操作。
- 只有 verifier 接受后的 canonical result 才进入会话；stream/candidate 不提前成为 assistant message。
- 缺失 DeepSeek、OpenAI 或 Anthropic 凭据时，错误正确指向对应设置入口。

完成真机检查时记录应用版本、操作系统、Provider/model、失败步骤和 diagnostic ID；不要在记录中包含
API key、完整 prompt、源码、完整模型响应或未脱敏工具结果。
