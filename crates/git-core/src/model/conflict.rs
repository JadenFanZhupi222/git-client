use serde::{Deserialize, Serialize};

/// 冲突文件的三方内容,来自 git index 的 stage 1/2/3 blob:
/// - `base`:共同祖先(stage 1)
/// - `ours`:我方/当前分支(stage 2)
/// - `theirs`:对方/被合入分支(stage 3)
///
/// 某一方缺失(如「新增 vs 新增」无共同祖先、或「一方删除」)时该字段为 `None`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictSides {
    pub base: Option<String>,
    pub ours: Option<String>,
    pub theirs: Option<String>,
}
