use crate::model::FileState;
use serde::{Deserialize, Serialize};

/// 一个提交里改动的单个文件(文件级)。带本文件的增删行数(diff --stat)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub status: FileState,
    /// 新增行数;二进制文件为 0。
    pub additions: usize,
    /// 删除行数;二进制文件为 0。
    pub deletions: usize,
}

/// 一行 diff 的种类:上下文 / 新增 / 删除。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffLineKind {
    Context,
    Addition,
    Deletion,
}

/// 行级 diff 的一行。old/new 行号在不适用时为 None
/// (新增行没有 old 行号,删除行没有 new 行号)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    pub content: String,
}

/// 一个 hunk:`@@ -a,b +c,d @@` 段及其下面的若干行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

/// 单个文件的行级 diff。
/// - 二进制文件 is_binary=true 且 hunks 为空。
/// - 超大文件 too_large=true 且 hunks 为空(为防卡死/爆内存,故意不计算逐行 diff)。
/// - Git LFS 指针文件 is_lfs_pointer=true 且 hunks 为空(指针文本不是真内容,
///   `lfs_size` 为指针记录的实际字节数;别把指针当内容 diff)。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileDiff {
    pub path: String,
    pub is_binary: bool,
    pub too_large: bool,
    /// 是否 Git LFS 指针文件(内容是 LFS 指针而非真实文件)。
    pub is_lfs_pointer: bool,
    /// LFS 指针记录的实际文件字节数(原样字符串,非 LFS 时为空)。
    pub lfs_size: String,
    pub hunks: Vec<Hunk>,
}
