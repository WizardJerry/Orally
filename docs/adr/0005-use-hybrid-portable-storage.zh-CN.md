---
status: superseded by ADR-0006
---

[English](0005-use-hybrid-portable-storage.md) |
[简体中文](0005-use-hybrid-portable-storage.zh-CN.md)

# 私有记录使用 SQLite，可共享配置使用 TOML

Orally 使用混合式便携存储模型。Local History 和明文 Provider Credentials 位于便携 SQLite 数据库中，而全局设置、非 secret 的 Service Connection 数据、Workflows 和 Modules 使用 TOML，使用户可以检查、版本管理和共享配置。可共享的 TOML 绝不能包含 API keys。

## 后果

Service Connection 配置引用存储在 SQLite 中的凭据。Workflow 文件可以在不携带秘密信息的情况下独立分发，但导入时必须解析其引用的连接和 Modules。保留的音频仍位于便携音频目录中，并与 SQLite 中的历史记录关联。
