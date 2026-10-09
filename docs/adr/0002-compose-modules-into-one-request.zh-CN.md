---
status: accepted
---

[English](0002-compose-modules-into-one-request.md) |
[简体中文](0002-compose-modules-into-one-request.zh-CN.md)

# 将 Modules 组合到一次后处理请求中

当前配置编辑器采用 [ADR-0008](0008-edit-self-contained-model-profiles.zh-CN.md)
规定的多模型例外。以下共享产品 Workflow 与 Module 设计仍为 Deferred 参考。

每个 Workflow 执行一个 Speech-recognition Stage（语音识别阶段），并且最多执行一次 AI 后处理请求。根据 [ADR-0007](0007-stream-long-recordings-through-audio-segments.zh-CN.md)，识别阶段可以对多个 Audio Segment（音频片段）依次发出请求，但会为 Workflow 生成一份按顺序组合的 Raw Transcript（原始转写文本）。Orally 不为每个细化步骤调用一个单独的模型，而是将一组经过排序的、可复用的内置或用户创建 Modules 组合成一个 prompt，并发送给该 Workflow 选择的 LLM。此方案取代 ADR-0001 中按顺序发出多个后处理请求的设计，以降低延迟和成本，同时让指令能够跨场景复用。

## 后果

Modules 可以被创建、保存、选择、移除和重新排序，而无需在各个 Workflows 中复制其文本。单个 Module 不会独立执行或失败；验证和失败处理作用于组合后的整个后处理请求。分支执行、并行执行和按 Module 选择模型不在此模型范围内。

Windows MVP 存储静态 Module 文本。在基本组合流程得到验证后，可以再增加声明式参数和每个 Workflow 的参数值。

Workflows 保留对 Modules 的实时引用，而不是保存快照。因此，编辑一个用户创建的 Module 会更新所有使用它的 Workflow，并且在保存前显示受影响的 Workflows；内置 Modules 为只读，但可以复制。

组合时首先放置不可覆盖的 Orally 规则，然后按可见顺序放置 modules，最后只放置一次 transcript。后面的 modules 优先于前面的用户 modules，设置界面会公开最终组合的 prompt 以供检查。
