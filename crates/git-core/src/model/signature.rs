use serde::{Deserialize, Serialize};

/// 提交的 GPG/SSH 签名验证状态(对应 git 的 `%G?`)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignatureStatus {
    /// 无签名(`N`)。
    #[default]
    None,
    /// 签名有效且密钥可信(`G`)。
    Good,
    /// 有签名但验不通过可信(`U` 未知有效性 / `X` 过期 / `Y` 密钥过期 / `R` 密钥吊销 / `E` 无法校验)。
    Unverified,
    /// 签名无效(`B`)。
    Bad,
}

/// 一个提交的签名信息:状态 + 签名者(`%GS`,无则空)。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SignatureInfo {
    pub status: SignatureStatus,
    pub signer: String,
}
