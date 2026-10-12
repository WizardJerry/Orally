---
status: accepted
---

[English](0006-store-workflows-in-sqlite-and-modules-as-toml.md) |
[简体中文](0006-store-workflows-in-sqlite-and-modules-as-toml.zh-CN.md)

# 完整 Workflows 存入 SQLite，Modules 使用 TOML

Orally 将完整的 Workflow 结构、Service Connections、明文 Provider Credentials 和 Local History 存储在便携 SQLite 数据库中。Module 内容保持可以独立共享：每个 Module 是一个 TOML 文件，内置 Modules 位于只读的 `module/builtin/` 下，自定义 Modules 位于可编辑的 `module/custom/` 下。Workflows 保留对这些 Modules 的实时引用。全局进程、窗口和热键 Config 继续位于 `config.toml` 中，保留的录音位于 `audio/` 下。

## 后果

共享一个 Module 不会共享整个 Workflow、Service Connection 或 Provider Credential；接收者自行组装自己的 Workflow。编辑一个自定义 Module 会更新所有引用它的 Workflow。内置 Modules 为只读，可以复制到一个新的、不可变的小写 ASCII kebab-case 标识符下，并且不能被自定义 Module 遮蔽。缺失或无效的 Module 文件可能使 Workflows 不完整，因此 Orally 必须在录音前验证引用，并且绝不能在 Module 文件中包含 API keys。
