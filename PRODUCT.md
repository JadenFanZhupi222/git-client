# Product

## Register

product

## Users

Rust/前端开发者与日常使用 git 的工程师。使用场景:在桌面上管理本地 git 仓库——查看历史图谱、暂存/提交、分支与远程操作、解决冲突、追溯与变基。多为「在任务流中」的专业用户,期待工具跟手、不打断思路,对标 JetBrains 内置 Git、GitKraken、Fork、Tower 等专业客户端。本项目作者本人(React/Next 出身、Rust 初学)既是开发者也是首要用户。

## Product Purpose

一款达到 JetBrains 内置 Git 插件水平的生产级桌面 git 客户端。技术栈 Tauri 2.x + React 前端 + 多 crate Rust 工作区(六边形架构:git-core 领域层 / git-engine 适配器 / app-service 应用层 / ipc-types 契约 / src-tauri 外壳)。核心价值:**本地优先、纯 Rust 内核带来的速度与隐私**,以及专业级的图谱可视化与历史/diff 深挖能力。成功 = 能完全替代日常 git 工作流,且在大仓库下依然流畅跟手。

## Brand Personality

精密、跟手、克制(precise / responsive / restrained)。气质是「开发者工具」而非消费级 App:信息密集但有秩序,玻璃材质只用于外壳浮层点到为止,主色朱红(vermilion)作为身份色而非装饰。动效服务于状态反馈与「活的图谱」的生命感,不为炫技。情绪目标:打开即觉「这是个认真的专业工具」,用起来「快且可信」。

## Anti-references

- **深紫色 AI 模板风**(硬禁忌):紫色渐变、generic SaaS landing 审美一律拒绝。
- 消费级花哨:大圆角(>16px 卡片)、彩色渐变文字、装饰性 glassmorphism 滥用、hero-metric 模板、每个区块的小号大写宽字距眉签。
- 把密集数据(diff / 图谱 / 文件列表)塞进玻璃容器——玻璃只用于外壳。
- 为「好看」牺牲密度与跟手感的设计。

## Design Principles

1. **工具消失于任务**:earned familiarity 优先于新奇;标准操作用标准 affordance,加速器(拖放/快捷键/命令面板)叠加在标准路径之上而非取代它。
2. **活的图谱**:图谱对操作有生命感的反馈(选中滑动、新提交入场、hover 泳道、拖放),但每一处动效都服务于状态可读性,且尊重 prefers-reduced-motion。
3. **token 是唯一真相**:颜色/字体只走 `index.css` 的 @theme token;语义 token 与图谱泳道可视化色分两套;白底加深 accent 保 AA。
4. **破坏性操作要么有护栏要么可撤销**:危险操作走确认或智能拦截;能进 undo 时间线的操作允许低门槛即时执行。
5. **本地优先、密集而有序**:面向专业用户的信息密度是特性;用层次、分组、留白驯服密度,而非削减信息。

## Accessibility & Inclusion

- 目标 WCAG AA:正文 ≥4.5:1,大字 ≥3:1;accent 在白底特意加深达标。
- 焦点可见(`:focus-visible` 朱红描边);浮层支持 Esc 关闭与焦点陷阱(`useModalListNav`)。
- 颜色不作为唯一信息载体(同步状态除色条外有 tooltip;节点空心/实心区分 push/pull 状态)。
- 所有动效在 `prefers-reduced-motion: reduce` 下降级为淡入或瞬时。
- 加速器(拖放等)始终有键盘/菜单等价路径,不锁死键盘与读屏用户。
- 提供「降低透明度」开关(玻璃转实底),照顾对模糊/透明敏感的用户。
