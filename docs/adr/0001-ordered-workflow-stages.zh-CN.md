---
status: superseded by ADR-0002
---

[English](0001-ordered-workflow-stages.md) |
[简体中文](0001-ordered-workflow-stages.zh-CN.md)

# Workflow 使用有序处理阶段

每个 Workflow 恰好包含一个语音识别阶段，之后是零个或多个由用户排序的文本细化阶段。Orally 使用线性链，而不采用分支、并行或合并执行，使配置、进度、历史和重试行为保持可预测，同时仍支持针对具体场景的多步处理。

## 后果

用户可以添加、移除、启用、禁用文本细化阶段并调整其顺序，而且每个阶段选择自己的模型和指令。必需阶段失败时会暂停插入；可跳过阶段则会传递上一个成功阶段的文本并发出警告。分支图和并行图不在 Windows MVP 范围内。
