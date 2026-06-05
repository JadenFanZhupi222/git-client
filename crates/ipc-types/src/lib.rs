//! ipc-types:前后端共享的数据契约(DTO)。
//! 生产项目里这里会接入 specta/ts-rs,从这些结构体自动生成 TypeScript 类型,
//! 让前后端类型在编译期对齐。阶段 0 先保持简单。

use git_core::model::{Commit, FileEntry, FileState, WorkingTreeStatus};
use serde::{Deserialize, Serialize};

/// 传给前端的提交 DTO。这里特意和领域模型 Commit 分开:
/// 领域模型可以很丰富,DTO 只暴露前端真正需要的字段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitDto {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub author_name: String,
    pub timestamp: i64,
}

impl From<Commit> for CommitDto {
    fn from(c: Commit) -> Self {
        CommitDto {
            id: c.id,
            short_id: c.short_id,
            summary: c.summary,
            author_name: c.author.name,
            timestamp: c.timestamp,
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

#[cfg(test)]
mod tests {
    use super::*;
    use git_core::model::{FileEntry, FileState, WorkingTreeStatus};

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
