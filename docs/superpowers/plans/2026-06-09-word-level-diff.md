# 词级 diff(M5.1)Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 unified DiffView 的增删行里,用 `similar` 算出行内词级改动段,前端高亮真正改了的那几个词,而不是整行红/整行绿。

**Architecture:** 后端在唯一 diff 构建点 `file_diff_from` 末尾跑纯函数 `annotate_word_level`,把每对配上的删行/增行切成 `Seg { text, changed }` 段(传切好的段、不传字节偏移,绕开 UTF-8/UTF-16 跨语言坑)。DTO 透传,DiffView 逐段渲染。

**Tech Stack:** Rust(git-core / git-engine / ipc-types)+ `similar` crate;React + TypeScript(DiffView)。

设计依据:`docs/superpowers/specs/2026-06-09-word-level-diff-design.md`。当前分支:`feat/word-level-diff`。

**Rust 概念提示(给初学者)**
- `Option<Vec<Seg>>`:`None` 表示「这行没有行内细节,整行着色」;`Some(段列表)` 表示「逐段渲染」。代替了 null。
- `&mut [Hunk]`:可变切片借用,函数原地改 hunks、不拿走所有权、不返回。
- `ChangeTag`:`similar` 用它标每段是 `Equal`(两边相同)/`Delete`(只在旧)/`Insert`(只在新)。
- 给 `DiffLine` 加字段会让**所有构造它的地方**编译报错(Rust 要求结构体字段全部填)。本计划 Task 1 一次补齐所有构造点,保证每步都能编译。

---

## File Structure

| 文件 | 责任 | 改动 |
|---|---|---|
| `crates/git-core/src/model/diff.rs` | 领域模型 | 加 `Seg` 结构体 + `DiffLine.emphasis` 字段 |
| `crates/git-core/src/model/mod.rs` | 模型导出 | 导出 `Seg` |
| `crates/git-engine/src/git2_backend.rs` | diff 构建 | 补 `emphasis: None` 构造;加 `annotate_word_level` + `word_segments` + `push_seg`;接入 `file_diff_from`;加单测 |
| `crates/git-engine/Cargo.toml` | 依赖 | 加 `similar`(optional,绑 git2-backend 特性) |
| `Cargo.toml`(根) | 统一版本 | `[workspace.dependencies]` 加 `similar = "2"` |
| `crates/app-service/src/lib.rs` | 用例 + 测试 | 测试里构造 `DiffLine` 补 `emphasis: None` |
| `crates/ipc-types/src/lib.rs` | 契约 DTO | 加 `SegDto` + `DiffLineDto.emphasis` + `From` 透传 |
| `app/src/ipc.ts` | 前端类型 | `DiffLineDto` 加 `emphasis` + `SegDto` 类型 |
| `app/src/components/DiffView.tsx` | 渲染 | 行内容改为逐段渲染 |

---

## Task 1: git-core 模型加 `Seg` + `emphasis` 字段(全部构造点补齐)

**Files:**
- Modify: `crates/git-core/src/model/diff.rs:23-31`(`DiffLine`)
- Modify: `crates/git-core/src/model/mod.rs:22`(导出)
- Modify: `crates/git-engine/src/git2_backend.rs:215-220`(构造点)
- Modify: `crates/app-service/src/lib.rs:800-805`(测试构造点)

纯结构改动、零行为变化。本任务不写新测试,靠 `cargo test --workspace` 验证仍全绿。

- [ ] **Step 1: 在 `diff.rs` 加 `Seg` 结构体**

在 `DiffLine` 定义(第 25 行 `pub struct DiffLine` 上方)插入:

```rust
/// 行内一段:`text` 是原文片段,`changed` 表示这段相对另一侧是否变化。
/// 一行的所有 `Seg` 的 `text` 顺序拼接 == 该行 `content`(不变式)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Seg {
    pub text: String,
    pub changed: bool,
}
```

- [ ] **Step 2: 给 `DiffLine` 加 `emphasis` 字段**

把 `DiffLine` 结构体改成(在 `content` 后加一行):

```rust
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    pub content: String,
    /// 行内词级标注。`None` = 无行内细节(上下文行 / 配不上对的行 / 整行重写),
    /// 整行按 `kind` 着色;`Some(段)` = 逐段渲染,`changed` 段加重高亮。
    pub emphasis: Option<Vec<Seg>>,
}
```

- [ ] **Step 3: 导出 `Seg`**

`crates/git-core/src/model/mod.rs:22` 改为:

```rust
pub use diff::{DiffLine, DiffLineKind, FileChange, FileDiff, Hunk, Seg};
```

- [ ] **Step 4: 补 git2_backend 构造点**

`crates/git-engine/src/git2_backend.rs` 第 215 行的 `lines.push(DiffLine {` 块,在 `content,` 后加 `emphasis: None,`:

```rust
                        lines.push(DiffLine {
                            kind,
                            old_lineno: dl.old_lineno(),
                            new_lineno: dl.new_lineno(),
                            content,
                            emphasis: None,
                        });
```

- [ ] **Step 5: 补 app-service 测试构造点**

`crates/app-service/src/lib.rs` 第 800 行 `vec![DiffLine {` 块,在 `content: "hi".into(),` 后加 `emphasis: None,`:

```rust
                lines: vec![DiffLine {
                    kind: DiffLineKind::Addition,
                    old_lineno: None,
                    new_lineno: Some(1),
                    content: "hi".into(),
                    emphasis: None,
                }],
```

- [ ] **Step 6: 编译 + 全测试验证仍绿**

Run: `cargo test --workspace`
Expected: 全部 PASS,无编译错误(若有别处构造 `DiffLine` 报错,同样补 `emphasis: None`)。

- [ ] **Step 7: Commit**

```bash
git add crates/git-core/src/model/diff.rs crates/git-core/src/model/mod.rs \
        crates/git-engine/src/git2_backend.rs crates/app-service/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(core): DiffLine 加词级标注字段 Seg/emphasis

纯结构,默认 None,零行为变化。后续 annotate_word_level 填充。

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: git-engine 词级标注纯函数(TDD)+ 接入

**Files:**
- Modify: `Cargo.toml`(根,`[workspace.dependencies]`)
- Modify: `crates/git-engine/Cargo.toml`
- Modify: `crates/git-engine/src/git2_backend.rs`(加函数 + 接入 `file_diff_from` + `#[cfg(test)]` 模块)

`annotate_word_level` 是纯函数(无 IO、不返回 `Result`),先写失败测试再实现。

- [ ] **Step 1: 加 `similar` 依赖**

根 `Cargo.toml` 的 `[workspace.dependencies]` 末尾加:

```toml
similar = "2"
```

`crates/git-engine/Cargo.toml`:把 `[dependencies]` 改为加一行 optional similar,并把它绑进 `git2-backend` 特性(`annotate_word_level` 只在 git2 路径用):

```toml
[features]
default = ["git2-backend"]
git2-backend = ["dep:git2", "dep:similar"]

[dependencies]
git-core = { path = "../git-core" }
git2 = { version = "0.18", optional = true }
similar = { workspace = true, optional = true }
```

- [ ] **Step 2: 写失败测试**

在 `crates/git-engine/src/git2_backend.rs` 文件末尾加测试模块(若文件已有 `#[cfg(test)] mod tests`,把这些函数并进去):

```rust
#[cfg(test)]
mod word_level_tests {
    use super::*;
    use git_core::model::{DiffLine, DiffLineKind, Hunk};

    fn line(kind: DiffLineKind, content: &str) -> DiffLine {
        DiffLine { kind, old_lineno: None, new_lineno: None, content: content.into(), emphasis: None }
    }

    // 取一行 emphasis 段的 (text, changed) 便于断言。
    fn segs(line: &DiffLine) -> Vec<(String, bool)> {
        line.emphasis.as_ref().unwrap().iter().map(|s| (s.text.clone(), s.changed)).collect()
    }

    #[test]
    fn pairs_word_level_changes() {
        let mut hunks = vec![Hunk {
            header: "@@".into(),
            lines: vec![
                line(DiffLineKind::Deletion, "foo bar"),
                line(DiffLineKind::Addition, "foo baz"),
            ],
        }];
        annotate_word_level(&mut hunks);
        assert_eq!(segs(&hunks[0].lines[0]), vec![("foo ".into(), false), ("bar".into(), true)]);
        assert_eq!(segs(&hunks[0].lines[1]), vec![("foo ".into(), false), ("baz".into(), true)]);
    }

    #[test]
    fn segments_concat_equals_content() {
        let mut hunks = vec![Hunk {
            header: "@@".into(),
            lines: vec![
                line(DiffLineKind::Deletion, "let x = compute(a, b);"),
                line(DiffLineKind::Addition, "let x = compute(a, c);"),
            ],
        }];
        annotate_word_level(&mut hunks);
        for l in &hunks[0].lines {
            let joined: String = l.emphasis.as_ref().unwrap().iter().map(|s| s.text.as_str()).collect();
            assert_eq!(joined, l.content);
        }
    }

    #[test]
    fn whole_line_rewrite_yields_none() {
        let mut hunks = vec![Hunk {
            header: "@@".into(),
            lines: vec![
                line(DiffLineKind::Deletion, "aaaa"),
                line(DiffLineKind::Addition, "bbbb"),
            ],
        }];
        annotate_word_level(&mut hunks);
        assert!(hunks[0].lines[0].emphasis.is_none());
        assert!(hunks[0].lines[1].emphasis.is_none());
    }

    #[test]
    fn unequal_counts_leave_extra_line_none() {
        let mut hunks = vec![Hunk {
            header: "@@".into(),
            lines: vec![
                line(DiffLineKind::Deletion, "foo one"),
                line(DiffLineKind::Deletion, "foo two"),
                line(DiffLineKind::Deletion, "leftover line"),
                line(DiffLineKind::Addition, "foo ONE"),
                line(DiffLineKind::Addition, "foo TWO"),
            ],
        }];
        annotate_word_level(&mut hunks);
        assert!(hunks[0].lines[0].emphasis.is_some()); // 配对 0
        assert!(hunks[0].lines[1].emphasis.is_some()); // 配对 1
        assert!(hunks[0].lines[2].emphasis.is_none()); // 多出的删行
    }

    #[test]
    fn context_lines_stay_none() {
        let mut hunks = vec![Hunk {
            header: "@@".into(),
            lines: vec![
                line(DiffLineKind::Context, "unchanged"),
                line(DiffLineKind::Addition, "brand new line"),
            ],
        }];
        annotate_word_level(&mut hunks);
        assert!(hunks[0].lines[0].emphasis.is_none());
        assert!(hunks[0].lines[1].emphasis.is_none()); // 无配对删行
    }
}
```

- [ ] **Step 3: 跑测试确认失败**

Run: `cargo test -p git-engine word_level`
Expected: FAIL —— `cannot find function `annotate_word_level` in this scope`。

- [ ] **Step 4: 实现 `annotate_word_level` + 辅助函数**

在 `git2_backend.rs` 的 `file_diff_from` 函数**上方**(约第 163 行,`use` 之后、`fn file_diff_from` 之前)加:

```rust
use git_core::model::Seg;
use similar::{ChangeTag, TextDiff};

/// 一对行相似度低于此值视为「整行重写」,不出行内段(避免满行高亮噪声)。
const WORD_DIFF_MIN_RATIO: f32 = 0.25;

/// 把一段文本按 `changed` flag 合并追加进段列表(相邻同 flag 合并;空串跳过)。
fn push_seg(segs: &mut Vec<Seg>, text: &str, changed: bool) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = segs.last_mut()
        && last.changed == changed
    {
        last.text.push_str(text);
        return;
    }
    segs.push(Seg { text: text.to_string(), changed });
}

/// 对一对(旧行, 新行)算词级 diff,返回 (删行段, 增行段)。
/// 相似度过低返回 None(整行重写,留整行着色)。
fn word_segments(old: &str, new: &str) -> Option<(Vec<Seg>, Vec<Seg>)> {
    let diff = TextDiff::from_words(old, new);
    if diff.ratio() < WORD_DIFF_MIN_RATIO {
        return None;
    }
    let mut del = Vec::new();
    let mut add = Vec::new();
    for change in diff.iter_all_changes() {
        let text = change.value();
        match change.tag() {
            ChangeTag::Equal => {
                push_seg(&mut del, text, false);
                push_seg(&mut add, text, false);
            }
            ChangeTag::Delete => push_seg(&mut del, text, true),
            ChangeTag::Insert => push_seg(&mut add, text, true),
        }
    }
    Some((del, add))
}

/// 给每个 hunk 内「连续删除行 + 紧接的连续新增行」配对,逐对标注行内词级段。
/// 原地修改;上下文行、配不上对的多余行的 emphasis 保持 None。
fn annotate_word_level(hunks: &mut [Hunk]) {
    for hunk in hunks.iter_mut() {
        let lines = &mut hunk.lines;
        let mut i = 0;
        while i < lines.len() {
            if lines[i].kind != DiffLineKind::Deletion {
                i += 1;
                continue;
            }
            let del_start = i;
            while i < lines.len() && lines[i].kind == DiffLineKind::Deletion {
                i += 1;
            }
            let del_end = i;
            let add_start = i;
            while i < lines.len() && lines[i].kind == DiffLineKind::Addition {
                i += 1;
            }
            let add_end = i;
            let pairs = (del_end - del_start).min(add_end - add_start);
            for k in 0..pairs {
                let old = lines[del_start + k].content.clone();
                let new = lines[add_start + k].content.clone();
                if let Some((del_segs, add_segs)) = word_segments(&old, &new) {
                    lines[del_start + k].emphasis = Some(del_segs);
                    lines[add_start + k].emphasis = Some(add_segs);
                }
            }
        }
    }
}
```

> 说明:`if let ... && ...`(let-chains)是 edition 2024 特性,本项目已用(见 `file_diff_from` 里的 LFS 块)。`Hunk`/`DiffLineKind` 在本文件已 `use`(构造 DiffLine 时用到),无需重复导入;若编译器报未导入,补到文件顶部的 `use git_core::model::{...}`。

- [ ] **Step 5: 接入 `file_diff_from`**

在 `file_diff_from` 的 LFS 处理块**之后**、`Ok(result)` **之前**(约第 236 行)加:

```rust
    // 词级标注:对剩余 hunks 标行内改动段。LFS/二进制/过大文件 hunks 已空 → 自动 no-op。
    annotate_word_level(&mut result.hunks);
    Ok(result)
```

- [ ] **Step 6: 跑测试确认通过**

Run: `cargo test -p git-engine word_level`
Expected: 5 个测试全部 PASS。

- [ ] **Step 7: 全量验证**

Run: `cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check`
Expected: 全绿、零警告、格式干净。

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml crates/git-engine/Cargo.toml crates/git-engine/src/git2_backend.rs
git commit -m "$(cat <<'EOF'
feat(engine): 词级 diff 标注 annotate_word_level

similar::from_words 对配对的删/增行算行内段,相似度 <0.25 视为整行重写不标注。接入 file_diff_from,三种 diff 一次受益。含 5 个纯函数单测。

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: ipc-types DTO 透传 emphasis

**Files:**
- Modify: `crates/ipc-types/src/lib.rs:5-8`(import)、`:464-487`(`DiffLineDto` + `From`)

- [ ] **Step 1: import 加 `Seg`**

`crates/ipc-types/src/lib.rs` 第 5-8 行的 `use git_core::model::{ ... }`,在 `Hunk,` 附近的列表里加 `Seg`(保持字母序不强求,加上即可):

```rust
use git_core::model::{
    // ...原有...
    DiffLine, DiffLineKind, FetchOutcome, FileChange, FileDiff, FileEntry, FileState, Hunk, Seg,
    // ...原有...
};
```

> 注:上面这行是示意——实际只需在现有 import 列表里加一个 `Seg`,别删别的。

- [ ] **Step 2: 加 `SegDto` + `From<Seg>`,给 `DiffLineDto` 加字段**

把第 464-487 行的 `DiffLineDto` 定义与 `From` 实现替换为:

```rust
/// 行内一段 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegDto {
    pub text: String,
    pub changed: bool,
}

impl From<Seg> for SegDto {
    fn from(s: Seg) -> Self {
        SegDto { text: s.text, changed: s.changed }
    }
}

/// 行级 diff 的一行 DTO。kind:context | add | del。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLineDto {
    pub kind: String,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    pub content: String,
    /// 行内词级段;None = 整行着色。
    pub emphasis: Option<Vec<SegDto>>,
}

impl From<DiffLine> for DiffLineDto {
    fn from(l: DiffLine) -> Self {
        let kind = match l.kind {
            DiffLineKind::Context => "context",
            DiffLineKind::Addition => "add",
            DiffLineKind::Deletion => "del",
        };
        DiffLineDto {
            kind: kind.to_string(),
            old_lineno: l.old_lineno,
            new_lineno: l.new_lineno,
            content: l.content,
            emphasis: l
                .emphasis
                .map(|segs| segs.into_iter().map(SegDto::from).collect()),
        }
    }
}
```

- [ ] **Step 3: 编译验证**

Run: `cargo test --workspace && cargo clippy --workspace -- -D warnings`
Expected: 全绿、零警告(app-service 的 `commit_file_diff_maps_dto` 测试仍通过,emphasis 默认 None 透传)。

- [ ] **Step 4: Commit**

```bash
git add crates/ipc-types/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(ipc): DiffLineDto 透传词级 emphasis(SegDto)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: 前端类型同步(ipc.ts)

**Files:**
- Modify: `app/src/ipc.ts:375-385`(`DiffLineDto`)

- [ ] **Step 1: 加 `SegDto` 类型 + `emphasis` 字段**

`app/src/ipc.ts` 第 375 行 `export interface DiffLineDto {` 上方加 `SegDto`,并给 `DiffLineDto` 加 `emphasis`:

```typescript
export interface SegDto {
  text: string;
  changed: boolean;
}

export interface DiffLineDto {
  kind: string;
  old_lineno: number | null;
  new_lineno: number | null;
  content: string;
  emphasis?: SegDto[] | null;
}
```

> 注:保持其余字段与现有定义一致,只加 `emphasis` 一行;`SegDto` 新增。

- [ ] **Step 2: 类型检查**

Run: `pnpm --dir app exec tsc -p tsconfig.json --noEmit`
Expected: 无错误(DiffView 还没用 emphasis,纯加字段不破坏)。

- [ ] **Step 3: Commit**

```bash
git add app/src/ipc.ts
git commit -m "$(cat <<'EOF'
feat(ui): ipc.ts DiffLineDto 加 emphasis 类型

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: DiffView 逐段渲染

**Files:**
- Modify: `app/src/components/DiffView.tsx:96-122`(行渲染)

- [ ] **Step 1: 抽一个段渲染辅助 + 替换行内容 span**

`app/src/components/DiffView.tsx` 第 120 行当前是:

```tsx
                <span className="flex-1 whitespace-pre pr-3 text-fg">{l.content || " "}</span>
```

替换为(用 emphasis 逐段渲染,无 emphasis 时整行):

```tsx
                <span className="flex-1 whitespace-pre pr-3 text-fg">
                  {l.emphasis && l.emphasis.length > 0
                    ? l.emphasis.map((s, si) =>
                        s.changed ? (
                          <span
                            key={si}
                            className={add ? "bg-success/30" : "bg-danger/30"}
                          >
                            {s.text}
                          </span>
                        ) : (
                          <span key={si}>{s.text}</span>
                        ),
                      )
                    : l.content || " "}
                </span>
```

> 说明:`add`/`del` 是该行已算好的布尔(第 97-98 行)。`changed` 段加重底色比整行底色 `/10` 深一档(`/30`),非 `changed` 段正常。颜色用既有 token,无硬编码 hex。空文本不会出现(后端 `push_seg` 跳过空串),无需额外保护。

- [ ] **Step 2: 类型检查 + 构建**

Run: `pnpm --dir app exec tsc -p tsconfig.json --noEmit && pnpm --dir app run build`
Expected: 均通过。

- [ ] **Step 3: 手动验证(真机)**

```bash
cd app && pnpm tauri dev
```
打开一个有「一行里改了个词」的提交 diff(如改函数名/常量值),确认:
- 改动的词被加重高亮(增行绿底深一档、删行红底深一档),未改部分正常。
- 整行重写的行(如完全不同内容)仍是整行红/绿、无碎高亮。
- 行级暂存(Changes 视图点 +/- 行选中)交互不受影响。

- [ ] **Step 4: Commit**

```bash
git add app/src/components/DiffView.tsx
git commit -m "$(cat <<'EOF'
feat(ui): DiffView 行内词级高亮渲染

emphasis 有值时逐段渲染,changed 段底色深一档;无值走整行着色。

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: 收尾合并

- [ ] **Step 1: 全量验证**

Run:
```bash
cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check
pnpm --dir app exec tsc -p tsconfig.json --noEmit && pnpm --dir app run build
```
Expected: 全绿。

- [ ] **Step 2: 更新 HANDOFF**

`docs/HANDOFF.md`「已完成功能」近期段落加一条 M5.1 词级 diff;「下一步候选」的 M5 条目标注 5.1 已完成、下一刀 M5.2 并排 diff。提交。

- [ ] **Step 3: 合并回 main**

```bash
git checkout main
git merge --no-ff feat/word-level-diff -m "Merge branch feat/word-level-diff: M5.1 词级 diff"
git branch -d feat/word-level-diff
```

> push 由用户手动做(铁律)。

---

## Self-Review

- **Spec 覆盖**:数据模型(Task 1)、标注逻辑含配对/噪声阈值/接入点(Task 2)、契约层(Task 3)、前端类型(Task 4)、DiffView 渲染(Task 5)、依赖(Task 2 Step 1)、测试点(Task 2 五个单测覆盖 spec 列的全部用例)——逐项有任务承接。
- **占位扫描**:无 TBD/TODO,每步有完整代码或确切命令。
- **类型一致**:`Seg`/`emphasis`/`SegDto`/`annotate_word_level`/`word_segments`/`push_seg`/`WORD_DIFF_MIN_RATIO` 跨任务命名一致;`emphasis` 三层(core→dto→ts)字段名统一。
- **构造点完整**:`DiffLine` 两处构造(git2_backend、app-service 测试)Task 1 均补 `emphasis: None`,Task 1 Step 6 兜底「别处报错也补」。
