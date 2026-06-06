//! ipc-types:前后端共享的数据契约(DTO)。
//! 生产项目里这里会接入 specta/ts-rs,从这些结构体自动生成 TypeScript 类型,
//! 让前后端类型在编译期对齐。阶段 0 先保持简单。

use git_core::model::{
    AheadBehind, BlameLine, BranchInfo, Commit, CommitRef, DiffLine, DiffLineKind, FetchOutcome,
    FileChange, FileDiff, FileEntry, FileState, Hunk, PullOutcome, PushOutcome, RefKind, StashEntry,
    WorkingTreeStatus,
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

/// 一条贮藏 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StashDto {
    pub index: usize,
    pub message: String,
}

impl From<StashEntry> for StashDto {
    fn from(s: StashEntry) -> Self {
        StashDto { index: s.index, message: s.message }
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
}

/// 行级 diff 的一行 DTO。kind:context | add | del。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLineDto {
    pub kind: String,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    pub content: String,
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
pub struct FileDiffDto {
    pub path: String,
    pub is_binary: bool,
    pub hunks: Vec<HunkDto>,
}

impl From<FileDiff> for FileDiffDto {
    fn from(d: FileDiff) -> Self {
        FileDiffDto {
            path: d.path,
            is_binary: d.is_binary,
            hunks: d.hunks.into_iter().map(HunkDto::from).collect(),
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
