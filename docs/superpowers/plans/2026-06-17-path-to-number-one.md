# 通往「世界第一 git 客户端」的差距分析与路线图

> 2026-06-17 制定。基于对当前代码库的事实核实(Explore 审计)+ 对标 Sublime Merge / Fork / Tower / GitKraken / GitButler / lazygit。
> 衔接:M1-M6 已完成(见 `2026-06-08-world-class-roadmap.md` + `docs/HANDOFF.md`);本文件是 M7+ 的战略框架。

## 现状定位

- **架构是对标对象里最干净的**(六边形 + GitBackend trait + actor + spawn_blocking 纪律),几乎没有同类产品这么认真。
- **产品完整度约 60-65%**:个人本地流程扎实(status/stage/commit、图谱、词级/并排/图片 diff、文件/行历史、pickaxe、blame、reflog、stash、cherry-pick、revert、reset、交互式 rebase、三栏冲突编辑器、撤销/重做、命令面板、文件监听、虚拟化),但「成为第一」要赢的三个差异化战场一件未动。
- **差的不是架构,是:广度补全 + 三个差异化战场 + 产品化分发。** 不需要重写任何东西,沿现有竖切套路一刀一刀切。

## Tier 0 · 桌面 git 客户端的硬门槛(缺了就是「半成品」)

任何排得上号的客户端都默认有,缺一项即被扣到「不完整」:

1. **clone / init**(❌ clone 全无;init 仅测试用,未暴露命令)——最大的洞。用户须先在命令行 clone/init 再来「打开本地仓库」。世界级客户端第一屏就是「克隆 URL / 新建 / 打开」。这是 onboarding 的正门。
2. **独立 merge**(❌)——只能通过 pull 被动合并,无法主动把某分支合进当前分支。
3. **远程管理 add/remove/rename remote**(❌)——只有 `set_upstream` 和选已有远程;fork+upstream 多远程协作做不了。
4. **diff 语法高亮**(❌)——用了 CodeMirror Merge 但没加 `lang-*`,diff/冲突编辑器代码无色。与 VS Code/GitHub 的第一眼差距。注:三栏合并编辑器本身已有(`ConflictEditor.tsx`),缺的是其中的高亮。
5. **跨平台真实可用**(部分)——CI 仅 Linux;签名/公证无;有 Windows 专属处理(build.rs 链 advapi32)。macOS/Linux 未在 CI 编译过 = 未验证。

## Tier 1 · 决定能否争「第一」的三个差异化战场

补完 Tier 0 只是追平;真正拉开差距靠这三件,且都契合已有资产:

### ① 托管平台集成 / PR(= roadmap M7,❌ 完全没有)
GitKraken/Fork/Tower 的护城河:PR 列表、建 PR、code review、issue、CI 状态、@提及。是「个人工具」与「团队协作中枢」的分水岭。需要 GitHub/GitLab API 集成 + OAuth/token 安全存储(顺带建立真正的认证体系——当前 push/fetch 只靠系统 credential helper,无 SSH key/OAuth/token 管理 UI)。

### ② 有品味的 AI(❌,但 memory 早已定向)
零 AI,而这是 2026 正面战场。独有优势:**这是 Claude Code 项目,Anthropic SDK 触手可及。** 按 ROI 切入:
- 生成/改写提交信息(暂存 diff → message)——最高频、最易惊艳,首刀首选。
- diff/提交讲解。
- 冲突解决建议(三栏编辑器旁)。
- 自然语言搜历史(→ 转 pickaxe/log 查询)。

### ③ GitButler 式现代范式(部分:已起步)
「活的图谱 + 拖放 cherry-pick」已踩进这个方向。GitButler 论点(虚拟分支、跨分支拖 commit、图谱内重排=交互式 rebase)是 git GUI 十年最大范式创新。已有交互式 rebase 引擎 + 拖放图谱 + 活的图谱审美 → 把拖放语义扩展为「拖 commit 到分支标签=rebase onto / 图谱内拖动重排」。能定义品类,且是审美上最强的一手。

## Tier 2 · 从「好代码库」到「真产品」

世界第一要几十万人能无痛装上用:

- **自动更新**(❌ Tauri updater 未配)——桌面刚需。
- **代码签名 / 公证**(❌)——没签名,macOS 拦截、Windows 报毒,直接劝退。
- **i18n / 英文界面**(❌ 全中文硬编码)——「世界」第一的门票;越早接入成本越低。
- **CI 三平台矩阵 + e2e**(部分:Linux only,无 WebDriver e2e)。
- **设置界面 / 键位自定义**(部分:零散 localStorage,无 Settings 视图)。
- **崩溃上报**(❌)——用户报 bug 抓不到现场。

## 最关键的 5 件事(推荐顺序)

1. **Tier 0 补全竖切**:clone + init + 独立 merge + 远程管理(一个 sprint,价值密度最高、风险最低,补「正门」与分支策略)。
2. **diff 语法高亮**:CodeMirror 加 `lang-*` + 主题,投入小观感大。
3. **英文 i18n 骨架**:越早越省,文案已散落,晚了更痛。
4. **AI 提交信息生成**:Anthropic SDK 单点切入,最快做出差异化「哇」。
5. **M7 托管集成(GitHub PR)**:补完上面后的战略大件,进入协作中枢赛道。

## 一句话

地基(架构)已是世界级;AI + 虚拟分支是相比 Fork/Tower「多出一个维度」之处,i18n + 签名分发是「世界」二字的入场券。
