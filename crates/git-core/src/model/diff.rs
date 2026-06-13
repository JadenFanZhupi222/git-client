use crate::model::{Commit, FileState};
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

/// 行内一段:`text` 是原文片段,`changed` 表示这段相对另一侧是否变化。
/// 一行的所有 `Seg` 的 `text` 顺序拼接 == 该行 `content`(不变式)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Seg {
    pub text: String,
    pub changed: bool,
}

/// 行级 diff 的一行。old/new 行号在不适用时为 None
/// (新增行没有 old 行号,删除行没有 new 行号)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    pub content: String,
    /// 行内词级标注。`None` = 无行内细节(上下文行 / 配不上对的行 / 整行重写),
    /// 整行按 `kind` 着色;`Some(段)` = 逐段渲染,`changed` 段加重高亮。
    pub emphasis: Option<Vec<Seg>>,
}

/// 一个 hunk:`@@ -a,b +c,d @@` 段及其下面的若干行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

/// 一侧图片的「取图句柄」(M6.2:不再内联 base64,避免 5MB 图膨胀 33% 还当 JSON 字符串解析)。
/// 前端拿 `(mime, oid)` 经 `read_image` 命令取原始字节、转 Blob URL 渲染。
/// `oid` 为该侧 blob 的十六进制对象 id;空串表示读工作区文件(未暂存新一侧,内容尚未入库)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageRef {
    pub mime: String,
    pub oid: String,
}

/// 单个文件的行级 diff。
/// - 二进制文件 is_binary=true 且 hunks 为空。
/// - 超大文件 too_large=true 且 hunks 为空(为防卡死/爆内存,故意不计算逐行 diff)。
/// - Git LFS 指针文件 is_lfs_pointer=true 且 hunks 为空(指针文本不是真内容,
///   `lfs_size` 为指针记录的实际字节数;别把指针当内容 diff)。
/// - 图片文件 is_image=true(是二进制的一种);`old_image`/`new_image` 为新旧两版图片
///   (新增文件无 old、删除文件无 new、过大的图片两者都为 None)。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileDiff {
    pub path: String,
    pub is_binary: bool,
    pub too_large: bool,
    /// 是否 Git LFS 指针文件(内容是 LFS 指针而非真实文件)。
    pub is_lfs_pointer: bool,
    /// LFS 指针记录的实际文件字节数(原样字符串,非 LFS 时为空)。
    pub lfs_size: String,
    /// 是否图片文件(按扩展名识别;同时 is_binary=true)。
    pub is_image: bool,
    /// 旧版图片取图句柄(改动前);新增文件为 None。
    pub old_image: Option<ImageRef>,
    /// 新版图片取图句柄(改动后);删除文件为 None。
    pub new_image: Option<ImageRef>,
    pub hunks: Vec<Hunk>,
}

/// 行历史(`git log -L`)的一条:某提交 + 它对选中行范围的 diff(仅范围 hunk)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineHistoryEntry {
    pub commit: Commit,
    pub diff: FileDiff,
}
