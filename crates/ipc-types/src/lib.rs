//! ipc-types:前后端共享的数据契约(DTO)。
//! 生产项目里这里会接入 specta/ts-rs,从这些结构体自动生成 TypeScript 类型,
//! 让前后端类型在编译期对齐。阶段 0 先保持简单。

use git_core::model::{
    AheadBehind, BlameLine, BranchDeleteImpact, BranchInfo, Commit, CommitRef, ConflictSides,
    DiffLine, DiffLineKind, FetchOutcome, FileChange, FileDiff, FileEntry, FileState, Hunk,
    ImageData, LineHistoryEntry, PullOutcome, PushOutcome, RefKind, ReflogEntry, Seg, SignatureInfo,
    SignatureStatus, StashEntry, SubmoduleInfo, SubmoduleStatus, WorkingTreeStatus, WorktreeInfo,
};
use serde::{Deserialize, Serialize};

/// 传给前端的提交 DTO。这里特意和领域模型 Commit 分开:
/// 领域模型可以很丰富,DTO 只暴露前端真正需要的字段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitDto {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    /// 提交信息正文(首行 summary 之后的部分);无正文时为空串。
    pub body: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64,
    pub parents: Vec<String>,
}

impl From<Commit> for CommitDto {
    fn from(c: Commit) -> Self {
        CommitDto {
            id: c.id,
            short_id: c.short_id,
            summary: c.summary,
            body: c.body,
            author_name: c.author.name,
            author_email: c.author.email,
            timestamp: c.timestamp,
            parents: c.parents,
        }
    }
}

/// 一条 reflog 记录 DTO。`new_oid` 是"重置回这一步"时 reset 的目标提交。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflogEntryDto {
    pub index: usize,
    pub selector: String,
    pub new_oid: String,
    pub new_short: String,
    pub message: String,
    pub committer_name: String,
    pub timestamp: i64,
}

impl From<ReflogEntry> for ReflogEntryDto {
    fn from(e: ReflogEntry) -> Self {
        ReflogEntryDto {
            index: e.index,
            selector: e.selector,
            new_oid: e.new_oid,
            new_short: e.new_short,
            message: e.message,
            committer_name: e.committer.name,
            timestamp: e.timestamp,
        }
    }
}

/// 一步 Undo / Redo 的描述:`label`=操作中文名,`target_short`=移动后 HEAD 的短 SHA。
/// 既用于 `undo`/`redo` 的返回(已执行),也用于 [`UndoStateDto`] 里描述「下一步能做什么」。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoStepDto {
    /// 操作的中文名,如 "提交"、"重置(reset)"。
    pub label: String,
    /// 这一步移动后 HEAD 指向的提交短 SHA(供 toast/tooltip 显示"回到 abc1234")。
    pub target_short: String,
    /// 这一步用的还原语义:`true` = 忠实还原了工作区(reset --hard,撤销 reset/cherry-pick 等),
    /// `false` = 只动 HEAD、内容回暂存区(reset --soft,撤销提交)。仅用于 toast 文案精确化。
    pub worktree_restored: bool,
}

/// 撤销/重做的当前可用性。驱动顶栏「撤销」「重做」两个按钮的显隐与文案。
/// `None` = 该方向无可用项(按钮不显);来自 `RepoContext` 内的操作时间线 + 光标。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoStateDto {
    /// 后退一步(撤销最近一次操作)。
    pub can_undo: Option<UndoStepDto>,
    /// 前进一步(重做刚撤销的操作)。
    pub can_redo: Option<UndoStepDto>,
}

/// 操作日志的一项:本工具做过的一次写操作的落点。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpLogEntryDto {
    /// 操作中文名,如 "提交"、"cherry-pick";时间线基点为 "起点"。
    pub label: String,
    /// 该操作后 HEAD 的短 SHA。
    pub target_short: String,
    /// Unix 时间戳(秒),供「几分钟前」显示。
    pub timestamp: i64,
}

/// 操作日志面板数据:本会话写操作时间线(oldest→newest)+ 当前光标位置。
/// 点击第 i 项 = goto(i),沿时间线 reset --soft 跳过去。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpLogDto {
    pub entries: Vec<OpLogEntryDto>,
    /// 当前 HEAD 所在的项下标(高亮「现在在哪」)。
    pub current: usize,
}

/// 跨 IPC 边界的错误:带错误码(前端做逻辑分支)+ 友好信息 + 是否可重试。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcError {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
}

/// 工作区单个文件状态的 DTO。state 用字符串,前端直接渲染徽章。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntryDto {
    pub path: String,
    pub state: String, // modified | added | deleted | renamed | untracked | conflicted
    pub staged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusDto {
    pub entries: Vec<FileEntryDto>,
}

impl From<FileEntry> for FileEntryDto {
    fn from(e: FileEntry) -> Self {
        let state = match e.state {
            FileState::Added => "added",
            FileState::Modified => "modified",
            FileState::Deleted => "deleted",
            FileState::Renamed => "renamed",
            FileState::Untracked => "untracked",
            FileState::Conflicted => "conflicted",
        };
        FileEntryDto {
            path: e.path,
            state: state.to_string(),
            staged: e.staged,
        }
    }
}

impl From<WorkingTreeStatus> for StatusDto {
    fn from(s: WorkingTreeStatus) -> Self {
        StatusDto {
            entries: s.entries.into_iter().map(FileEntryDto::from).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChangeDto {
    pub path: String,
    pub status: String, // added | modified | deleted | renamed | untracked | conflicted
    pub additions: usize,
    pub deletions: usize,
}

impl From<FileChange> for FileChangeDto {
    fn from(c: FileChange) -> Self {
        let status = match c.status {
            FileState::Added => "added",
            FileState::Modified => "modified",
            FileState::Deleted => "deleted",
            FileState::Renamed => "renamed",
            FileState::Untracked => "untracked",
            FileState::Conflicted => "conflicted",
        };
        FileChangeDto {
            path: c.path,
            status: status.to_string(),
            additions: c.additions,
            deletions: c.deletions,
        }
    }
}

/// 当前分支相对上游的领先/落后 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AheadBehindDto {
    pub ahead: usize,
    pub behind: usize,
}

impl From<AheadBehind> for AheadBehindDto {
    fn from(a: AheadBehind) -> Self {
        AheadBehindDto {
            ahead: a.ahead,
            behind: a.behind,
        }
    }
}

/// blame 一行 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlameLineDto {
    pub line_no: u32,
    pub commit_id: String,
    pub short_id: String,
    pub author_name: String,
    pub timestamp: i64,
    pub content: String,
}

impl From<BlameLine> for BlameLineDto {
    fn from(l: BlameLine) -> Self {
        BlameLineDto {
            line_no: l.line_no,
            commit_id: l.commit_id,
            short_id: l.short_id,
            author_name: l.author_name,
            timestamp: l.timestamp,
            content: l.content,
        }
    }
}

/// 提交签名 DTO。status: "none" | "good" | "unverified" | "bad"。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureInfoDto {
    pub status: String,
    pub signer: String,
}

impl From<SignatureInfo> for SignatureInfoDto {
    fn from(s: SignatureInfo) -> Self {
        let status = match s.status {
            SignatureStatus::None => "none",
            SignatureStatus::Good => "good",
            SignatureStatus::Unverified => "unverified",
            SignatureStatus::Bad => "bad",
        };
        SignatureInfoDto {
            status: status.into(),
            signer: s.signer,
        }
    }
}

/// 子模块 DTO。status: "uninitialized" | "up-to-date" | "modified" | "conflict"。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmoduleInfoDto {
    pub path: String,
    pub url: String,
    pub head_sha: String,
    pub short_sha: String,
    pub status: String,
    pub describe: String,
}

impl From<SubmoduleInfo> for SubmoduleInfoDto {
    fn from(s: SubmoduleInfo) -> Self {
        let status = match s.status {
            SubmoduleStatus::Uninitialized => "uninitialized",
            SubmoduleStatus::UpToDate => "up-to-date",
            SubmoduleStatus::Modified => "modified",
            SubmoduleStatus::Conflict => "conflict",
        };
        SubmoduleInfoDto {
            short_sha: s.head_sha.chars().take(7).collect(),
            path: s.path,
            url: s.url,
            head_sha: s.head_sha,
            status: status.into(),
            describe: s.describe,
        }
    }
}

/// 工作树 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeInfoDto {
    pub path: String,
    pub head_sha: String,
    pub short_sha: String,
    pub branch: String,
    pub is_main: bool,
    pub is_current: bool,
    pub detached: bool,
    pub locked: bool,
    pub bare: bool,
}

impl From<WorktreeInfo> for WorktreeInfoDto {
    fn from(w: WorktreeInfo) -> Self {
        WorktreeInfoDto {
            short_sha: w.head_sha.chars().take(7).collect(),
            path: w.path,
            head_sha: w.head_sha,
            branch: w.branch,
            is_main: w.is_main,
            is_current: w.is_current,
            detached: w.detached,
            locked: w.locked,
            bare: w.bare,
        }
    }
}

/// 冲突文件三方内容 DTO(base/ours/theirs,某方缺失为 null)。供三栏合并编辑器用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictSidesDto {
    pub base: Option<String>,
    pub ours: Option<String>,
    pub theirs: Option<String>,
}

impl From<ConflictSides> for ConflictSidesDto {
    fn from(s: ConflictSides) -> Self {
        ConflictSidesDto {
            base: s.base,
            ours: s.ours,
            theirs: s.theirs,
        }
    }
}

/// 一条贮藏 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StashDto {
    pub index: usize,
    pub message: String,
}

impl From<StashEntry> for StashDto {
    fn from(s: StashEntry) -> Self {
        StashDto {
            index: s.index,
            message: s.message,
        }
    }
}

/// 本地分支 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchDto {
    pub name: String,
    pub is_head: bool,
}

impl From<BranchInfo> for BranchDto {
    fn from(b: BranchInfo) -> Self {
        BranchDto {
            name: b.name,
            is_head: b.is_head,
        }
    }
}

/// 删分支影响预览 DTO:`unmerged_commits`>0 时前端做强危险二次确认。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchDeleteImpactDto {
    pub unmerged_commits: usize,
    pub sample_summaries: Vec<String>,
}

impl From<BranchDeleteImpact> for BranchDeleteImpactDto {
    fn from(i: BranchDeleteImpact) -> Self {
        BranchDeleteImpactDto {
            unmerged_commits: i.unmerged_commits,
            sample_summaries: i.sample_summaries,
        }
    }
}

/// 一次 fetch 的结果 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchResultDto {
    pub remote: String,
    pub summary: String,
}

impl From<FetchOutcome> for FetchResultDto {
    fn from(o: FetchOutcome) -> Self {
        FetchResultDto {
            remote: o.remote,
            summary: o.summary,
        }
    }
}

/// 一次 pull 的成功结果 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullResultDto {
    pub summary: String,
}

impl From<PullOutcome> for PullResultDto {
    fn from(o: PullOutcome) -> Self {
        PullResultDto { summary: o.summary }
    }
}

/// 一次 push 的成功结果 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushResultDto {
    pub summary: String,
    /// 本次是否顺带建立了上游(首次 push 自动 -u 时为 true,前端可特别提示)。
    pub set_upstream: bool,
}

impl From<PushOutcome> for PushResultDto {
    fn from(o: PushOutcome) -> Self {
        PushResultDto {
            summary: o.summary,
            set_upstream: o.set_upstream,
        }
    }
}

/// 图谱中一段连线:在某行单元格内,从 `from` 列连到 `to` 列。
/// 上半段语义为 顶边→节点(中点),下半段为 节点(中点)→底边。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphSegDto {
    pub from: u32,
    pub to: u32,
    pub color: u32,
}

/// 指向某 commit 的引用 DTO。kind:head | local | remote。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefDto {
    pub name: String,
    pub kind: String,
}

impl From<CommitRef> for RefDto {
    fn from(r: CommitRef) -> Self {
        let kind = match r.kind {
            RefKind::Head => "head",
            RefKind::LocalBranch => "local",
            RefKind::RemoteBranch => "remote",
            RefKind::Tag => "tag",
        };
        RefDto {
            name: r.name,
            kind: kind.to_string(),
        }
    }
}

/// 图谱一行:嵌入提交信息 + 节点列/颜色 + 上下半段连线 + 指向该提交的引用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphRowDto {
    pub commit: CommitDto,
    pub column: u32,
    pub color: u32,
    pub top: Vec<GraphSegDto>,
    pub bottom: Vec<GraphSegDto>,
    /// 指向本行提交的引用(分支/远程/HEAD);多数行为空。
    pub refs: Vec<RefDto>,
    /// 同步标记:"" 普通 | "outgoing" 已 commit 未 push | "incoming" 已 fetch 未 pull。
    pub sync: String,
}

/// 行内一段 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegDto {
    pub text: String,
    pub changed: bool,
}

impl From<Seg> for SegDto {
    fn from(s: Seg) -> Self {
        SegDto {
            text: s.text,
            changed: s.changed,
        }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HunkDto {
    pub header: String,
    pub lines: Vec<DiffLineDto>,
}

impl From<Hunk> for HunkDto {
    fn from(h: Hunk) -> Self {
        HunkDto {
            header: h.header,
            lines: h.lines.into_iter().map(DiffLineDto::from).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageDataDto {
    pub mime: String,
    pub base64: String,
}

impl From<ImageData> for ImageDataDto {
    fn from(i: ImageData) -> Self {
        ImageDataDto {
            mime: i.mime,
            base64: i.base64,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiffDto {
    pub path: String,
    pub is_binary: bool,
    pub too_large: bool,
    pub is_lfs_pointer: bool,
    pub lfs_size: String,
    pub is_image: bool,
    pub old_image: Option<ImageDataDto>,
    pub new_image: Option<ImageDataDto>,
    pub hunks: Vec<HunkDto>,
}

impl From<FileDiff> for FileDiffDto {
    fn from(d: FileDiff) -> Self {
        FileDiffDto {
            path: d.path,
            is_binary: d.is_binary,
            too_large: d.too_large,
            is_lfs_pointer: d.is_lfs_pointer,
            lfs_size: d.lfs_size,
            is_image: d.is_image,
            old_image: d.old_image.map(ImageDataDto::from),
            new_image: d.new_image.map(ImageDataDto::from),
            hunks: d.hunks.into_iter().map(HunkDto::from).collect(),
        }
    }
}

/// 行历史的一条:某提交 + 它对选中行范围的 diff。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineHistoryEntryDto {
    pub commit: CommitDto,
    pub diff: FileDiffDto,
}

impl From<LineHistoryEntry> for LineHistoryEntryDto {
    fn from(e: LineHistoryEntry) -> Self {
        LineHistoryEntryDto {
            commit: CommitDto::from(e.commit),
            diff: FileDiffDto::from(e.diff),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git_core::model::{FileEntry, FileState, WorkingTreeStatus};

    #[test]
    fn maps_file_change_to_dto() {
        use git_core::model::{FileChange, FileState};
        let dto = FileChangeDto::from(FileChange {
            path: "a.rs".into(),
            status: FileState::Deleted,
            additions: 0,
            deletions: 5,
        });
        assert_eq!(dto.path, "a.rs");
        assert_eq!(dto.status, "deleted");
        assert_eq!(dto.additions, 0);
        assert_eq!(dto.deletions, 5);
    }

    #[test]
    fn maps_reflog_entry_to_dto() {
        use git_core::model::{ReflogEntry, Signature};
        let dto = ReflogEntryDto::from(ReflogEntry {
            index: 0,
            selector: "HEAD@{0}".into(),
            new_oid: "abcdef1234567890".into(),
            new_short: "abcdef1".into(),
            message: "commit: hello".into(),
            committer: Signature {
                name: "Tester".into(),
                email: "t@e".into(),
            },
            timestamp: 42,
        });
        assert_eq!(dto.selector, "HEAD@{0}");
        assert_eq!(dto.new_short, "abcdef1");
        assert_eq!(dto.committer_name, "Tester");
        assert_eq!(dto.timestamp, 42);
    }

    #[test]
    fn maps_status_to_dto_with_string_state() {
        let st = WorkingTreeStatus {
            entries: vec![
                FileEntry {
                    path: "a.txt".into(),
                    state: FileState::Modified,
                    staged: false,
                },
                FileEntry {
                    path: "b.txt".into(),
                    state: FileState::Added,
                    staged: true,
                },
            ],
        };
        let dto = StatusDto::from(st);
        assert_eq!(dto.entries.len(), 2);
        assert_eq!(dto.entries[0].state, "modified");
        assert!(!dto.entries[0].staged);
        assert_eq!(dto.entries[1].state, "added");
    }
}
