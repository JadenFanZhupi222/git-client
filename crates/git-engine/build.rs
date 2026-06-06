fn main() {
    // Windows 链接修复:libgit2-sys 引用了 advapi32 里的符号
    // (进程令牌/SID 权限、注册表、旧版 CryptoAPI 哈希),
    // 但它的 build 脚本未声明该系统库依赖,导致依赖 git-engine 的
    // 测试二进制在链接阶段报一堆 LNK2019 未解析符号。这里显式补链。
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-lib=dylib=advapi32");
    }
}
