---
target: feat/ui-redesign 活的图谱 + 玻璃外壳
total_score: 29
p0_count: 0
p1_count: 2
timestamp: 2026-06-16T12-50-48Z
slug: app-src-components-commitgraph-tsx
---
## 设计健康分:29/40(Good)

| # | 启发式 | 分 | 关键问题 |
|---|--------|----|---------|
| 1 | 系统状态可见性 | 3 | 拖放 cherry-pick 进行中图谱无 in-flight 信号 |
| 2 | 贴合真实世界 | 3 | 拖到 HEAD = cherry-pick 的隐喻需自悟 |
| 3 | 用户控制与自由 | 3 | undo/redo 强;拖放即触发无即时撤销提示 |
| 4 | 一致性与标准 | 3 | 拖放是为 cherry-pick 新造的非标准 affordance |
| 5 | 错误预防 | 2 | 拖放对已合并提交无拦截(已修)|
| 6 | 识别优于回忆 | 3 | 拖放零可发现性(已修:grab 光标 + 一次性提示)|
| 7 | 灵活与效率 | 4 | ⌘K/键盘/右键/拖放,power-user 面厚实 |
| 8 | 美学与极简 | 3 | 玻璃克制;顶栏控件渐密(最多 8 组)|
| 9 | 错误恢复 | 3 | 错误走 toast 明语,冲突路由到「更改」页 |
| 10 | 帮助与文档 | 2 | title 提示好,无 onboarding |

## 反模式裁决
不像 AI 生成。token 纪律(语义/泳道分两套、白底加深 accent)、玻璃仅外壳、活的图谱滑动选中条都是 craft 信号。
检测器:Toast border-l-4(alert 侧色条,边缘案例 P2);index.css stroke-width 假阳性(SVG 描边非布局)。

## 优先问题
- [P1·已修] 拖放零可发现性 → grab 光标 + 一次性 localStorage 提示。
- [P1·已修] 拖放对已合并提交无拦截 → HEAD 可达性 BFS,无效投放 no-drop 光标 + 忽略。
- [P2] 投放进行中无状态 + HEAD 行可能在视口外 → 顶部进度信号 + 常驻投放区(未做)。
- [P2] 顶栏控件密度逼近工作记忆上限(8 组)→ 撤销/重做非激活时收成图标(未做)。

## 角色红旗
- Alex:效率满分,唯发现拖放靠运气(已缓解)。
- Sam:cherry-pick 有键盘路径,拖放仅加速器,未锁死;拖放本身无 ARIA。
- Riley:已合并提交拖到 HEAD 报空提交(已修);快速滚动误触发拖拽(部分缓解)。

## 待办(下一轮)
- harden:投放进行中状态 + 常驻投放区(P2)。
- layout:顶栏密度收纳(P2)。
- 真机视觉验收:玻璃 backdrop-filter、拖放手感、暗底投放气泡对比。
