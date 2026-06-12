# 路线图:M6 · Polish & Harden(打磨与硬化)（2026-06-12）

> 背景:M1–M5 全部完成(性能地基 / 永不丢工作 / 键盘流 / 真实世界 git / 更深的 diff 与历史)。
> 功能已基本对齐 JetBrains 内置 Git。但 M5 几刀为求快留了几处「能跑但没到世界级」的债,
> 且引入了几个新表面(并排 diff、图片 diff、CLI 读路径)未享受 M1 的基建(虚拟化/缓存/取消)。
>
> **M6 不堆新功能,专门补这几处债 + 打磨,把体验和工程硬度抬到「世界第一」那条线。**
> 原 roadmap 的「M6 协作/PR」顺延为 **M7**(见 `2026-06-08-world-class-roadmap.md`)。

---

## 诚实评估:M5 之后的真实差距(已核对代码,非凭印象)

| 债 | 现状 | 后果 | 落到 |
|---|---|---|---|
| 并排 diff 无同步滚动 / 不折叠未改区 | M5.2 明确跳过 | 体感不如 JetBrains(招牌:双栏联动 + 中间「河流」+ 折叠) | M6.1 |
| DiffView 未虚拟化 | M1.4 只虚拟化了 CommitGraph | 接近 2 万行上限的大 diff 渲染卡(尤其并排双栏) | M6.1 |
| 图片 diff base64 塞进 JSON | M5 图片刀的捷径 | 5MB 图膨胀 ~33% 还当 JSON 字符串解析,内存翻几倍 | M6.2 |
| 图片只做 2-up,无滑块/洋葱皮,SVG 当文本 | M5 图片刀 YAGNI | 对比模式比对手少;SVG 不能当图看 | M6.2 |
| 新 CLI 读路径绕过缓存 + 取消 | file_history/line_history/pickaxe 直透,无代次取消、无 LRU | 快速切文件留下没人要的 git 子进程;大仓库每次开面板重算 | M6.3 |
| DTO 手工同步(没接 specta) | `ipc.ts` 手抄 Rust DTO;ARCHITECTURE.md 自己点名要自动生成 | 漏字段 = 静默 bug(已多次手抄 FileDiffDto/CommitDto…) | M6.4 |
| M5 新增 TS 逻辑无测试 | `buildSbsRows`(并排配对)、图片字节估算等无 vitest(后端各刀有测) | 重构易回归 | M6.5(可融进各刀) |
| 新面板不可键盘导航 | file/line history、图片 diff 只能点外部关 | JetBrains 全键盘;Esc/方向键/焦点陷阱缺失 | M6.6 |

> 已经做对、**不要动**:六边形分层、`spawn_blocking` 铁律、thiserror→结构化 IpcError、
> 每后端刀的 tempfile 测试、token 化 UI、原子提交。M6 是在好底子上打磨,不是返工。

---

## 里程碑切片(按 价值/风险 排序,每刀仍走竖切 + feat 分支 + `--no-ff` 合 main + 全门绿)

### M6.1 · 并排 diff 体验补强(最高价值,纯前端)
**目标**:并排视图达到 JetBrains 手感的三件套。
- **同步滚动**:左右两列垂直滚动联动(滚一边另一边跟随)。当前列优先布局是两列各自 `overflow-x-auto`、
  共享外层纵向滚动,纵向其实已同步;**真正要做的是横向**——或改成「单一滚动容器 + 两列等宽行对齐」让
  纵横都联动。重新评估 `SplitDiff` 布局:行对(`SbsRow`)已配平,改为「每行一个 grid 两列」即可纵横天然同步。
- **折叠未改区**:连续 N(默认 ≥ 6)行 context 折叠成「… 展开 X 行 …」,点开展开。统一 + 并排都要。
  纯函数 `collapseContext(lines, ctx=3)` 切「改动块 ± 上下文」与「可折叠间隔」,DiffView 渲染折叠条。
- **DiffView 虚拟化**:用 `@tanstack/react-virtual`(项目已用,见 CommitGraph)。难点:hunk 头 + 行混排、
  并排两列、折叠态行数动态。方案:先把 diff 拍平成一维「渲染项」数组(hunk头 / 行 / 折叠条),再虚拟化该数组。
**文件**:`app/src/components/DiffView.tsx`(主)、新纯函数 `app/src/lib/diffRows.ts`(拍平 + 折叠,可测)。
**测试**:`collapseContext` / 拍平函数 vitest(各种 hunk 拓扑);并排配对 `buildSbsRows` 补测(欠的)。
**风险**:虚拟化 + 折叠 + 并排三者交互复杂;建议先折叠、再拍平、最后虚拟化,分 3 个 commit。
**YAGNI**:中间「河流」连接带(块对应可视化)留到后面;先把滚动/折叠/虚拟化做扎实。

### M6.2 · 图片 diff 去 base64 + 对比模式(去架构捷径)
**目标**:图片不再走 base64-in-JSON;补对比模式。
- **改用 Tauri 自定义协议**:注册一个 `gitimg://` 协议(或用 asset 协议),前端 `<img src>` 指向它,
  后端按 `(repo, rev|WORKDIR, file)` 流式返回字节。`FileDiff` 不再内联 base64,只标 `is_image` + 两侧的
  「取图句柄」(rev + path,或一个不透明 token)。**这是把 M5 图片刀的捷径还掉。**
  - Tauri 2 注册自定义协议见 `tauri::Builder::register_uri_scheme_protocol`;handler 在阻塞线程读 blob/工作区文件。
  - 安全:handler 校验 repo 在已打开集合内、path 不逃逸 workdir。
- **对比模式**:并排 2-up(已有)+ **滑块(swipe)** + **洋葱皮(opacity 混合)**。一个模式切换条,偏好存 localStorage。
- **SVG 当图**:SVG 既可文本 diff 又可渲染;加「文本 / 预览」切换(默认按文件类型,SVG 默认预览)。
**文件**:`app/src-tauri/src/lib.rs`(注册协议 + handler)、`crates/git-core` 模型调整(去 ImageData 内联,改句柄)、
`crates/git-engine`(按 rev+path 读字节的方法)、`ipc-types`、`DiffView.tsx` + 新 `ImageDiff` 模式。
**测试**:后端「按 rev+path 读图字节」tempfile 测;协议 handler 的路径校验测。
**风险**:改 FileDiff 形状(去 base64)= 破坏性,需同步改 M5 已落地的图片渲染。建议先加协议、双轨过渡、再删 base64。
**YAGNI**:像素级差异(diff xor)留后。

### M6.3 · 新 CLI 读路径接缓存 + 取消(一致性 + 性能,后端小刀)
**目标**:file_history / line_history / pickaxe 享受 M1 基建,与 log/blame 一致。
- **缓存**:在 `RepoContext` 给三者加 LRU(file_history 按 (file,limit);line_history 按 (file,start,end);
  pickaxe 按 (query,regex,limit))。失效语义:都属 **ref 域**(提交/分支变即失效),挂进 `invalidate(GitRef)`。
- **取消**:给 `RepoContext` 加对应代次计数(或复用一个「按需读」通用代次);更现实的做法是 **CLI 子进程可杀**——
  把 `Command` 换成可持 `Child` 句柄,被新请求取代时 `kill()`。或最低限度:前端 query 已有 keepPreviousData,
  后端至少别重复跑(缓存命中即跳过)。**先做缓存(便宜、收益大),取消按需。**
**文件**:`crates/app-service/src/repo_context.rs`(加缓存字段 + 失效)、必要时 `crates/git-engine`(可杀子进程)。
**测试**:app-service 缓存命中/失效测(FakeBackend 计数调用次数)。
**风险**:低。纯加缓存不改行为。

### M6.4 · specta/tauri-specta 自动生成 TS 类型(消除手工 DTO 同步)
**目标**:`ipc.ts` 的 DTO 类型从 Rust 自动生成,后端改字段→前端编译期报错。ARCHITECTURE.md 第 3.4 的安全网。
- 给 `ipc-types` 的 DTO 加 `#[derive(specta::Type)]`;`tauri-specta` 导出 `bindings.ts`;前端从 bindings import,
  逐步替换手写的 `ipc.ts` 类型(命令签名也可一并生成)。
- **大改、invasive**:碰所有 DTO + 命令注册。放在 M6 靠后、其它刀稳定后做,避免和它们打架。
**文件**:`crates/ipc-types`(全 DTO 加 derive)、`app/src-tauri`(导出 bindings)、`xtask` 或 build step、`app/src/ipc.ts`(改 import)。
**测试**:生成的 bindings 编译通过 + 一处手验字段对齐;CI 加「bindings 是否最新」检查(可选)。
**风险**:中高(版本/特性兼容、生成时机)。先在一个 DTO 上打通,再铺开。

### M6.5 · M5 测试补齐(derisk,可融进各刀)
**目标**:M5 引入的前端逻辑补上 vitest(后端各刀已有 tempfile 测)。
- `buildSbsRows`(并排配对:context/纯增/纯删/不等数量)、`collapseContext`(M6.1 产出)、图片字节估算、
  搜索模式切换的结果选择逻辑。
**说明**:**优先把这些测试直接写进 M6.1/M6.2 对应刀**,而不是单独一刀;此条作为「不要再欠前端测试」的提醒。

### M6.6 · 新面板键盘导航 + a11y(打磨)
**目标**:file/line history、图片 diff 面板可全键盘操作。
- Esc 关闭、↑↓ 在提交列表移动、Enter 选中、焦点陷阱(Tab 不逃出模态)、`role="dialog"` + `aria-modal`。
- 复用现有 listNav 纯逻辑(M3.2 `app/src/lib/listNav.ts`)。
**文件**:`FileHistoryPanel.tsx` / `LineHistoryPanel.tsx` / `DiffView`(图片)。
**风险**:低。

---

## 建议执行顺序与理由

1. **M6.1**(并排体验)—— 价值最高、纯前端、不破坏后端;先折叠→拍平→虚拟化分 3 commit。
2. **M6.3**(CLI 读缓存/取消)—— 便宜、后端小、立刻和 M1 基建一致。
3. **M6.2**(图片去 base64 + 模式)—— 还掉架构捷径,破坏性改 FileDiff,双轨过渡。
4. **M6.6**(键盘/a11y)—— 打磨,低风险。
5. **M6.4**(specta 自动类型)—— 最 invasive,放最后、其它都稳了再动。
- **M6.5** 不单独排期,测试写进 1/2/3 各刀。

## 成功标准
- 大 diff(近 2 万行)并排滚动 60fps、未改区折叠、纵横联动;图片不再走 base64-in-JSON 且有滑块/洋葱皮;
  file/line/pickaxe 与 log 一样命中缓存、切走即取消;DTO 改字段前端编译期报错;新面板全键盘可达。
- 全程:`cargo test/clippy/fmt` + `pnpm test`(vitest)+ `tsc`/`pnpm build` 全绿;每刀真机验收。

## 明确不做(避免散焦)
- 协作/PR(顺延 M7)。- diff 中间「河流」连接带、像素级图片差异:M6 之后再说。
- 不为做而做:每刀必须能跑、能验、体感有提升。
