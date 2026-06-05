# 项目:生产级 Rust Git 客户端

## 背景
- 目标:JetBrains 内置 Git 插件水平的桌面客户端
- 技术栈:Tauri 2.x + React 前端 + 多 crate Rust 工作区
- 开发者:前端出身(React/Next),Rust 初学者,请多解释 Rust 概念

## 必读文档
- 完整架构设计见 @ARCHITECTURE.md
- 启动步骤见 @README.md
- 当前进度:阶段 0 骨架已编译验证通过

## 关键铁律
- 所有 git 操作必须放进 spawn_blocking,绝不在 async 命令里直接调
- 上层只依赖 GitBackend trait,不依赖具体实现