---
status: accepted
---

[English](0003-share-provider-connections-across-workflows.md) |
[简体中文](0003-share-provider-connections-across-workflows.zh-CN.md)

# 在 Workflows 之间共享提供方连接

提供方端点、协议和凭据作为可复用的 Service Connections 存储，而不是在每个 Workflow 中重复保存。一个 Workflow 引用一个 ASR 连接和一个 LLM 连接，同时保留自己的模型选择、ASR 指令和 Modules。这样可以集中管理秘密信息和端点的变更，同时不妨碍不同 Workflows 使用不同的模型或提供方。

## 后果

编辑一个 Service Connection 会影响所有引用它的 Workflow，并且在保存前必须显示受影响的 Workflows。需要隔离时，用户可以复制一个连接。被引用的连接在其引用被替换或移除之前不能删除。

只读的 Built-in Workflows 通过全局默认 ASR 和默认 LLM 服务槽进行绑定，使软件更新可以替换其结构，而不会覆盖用户特定的提供方或凭据。Custom Workflows 可以继承这些服务槽，也可以显式绑定。
