# 隐私与权限

[English](privacy.md) | [简体中文](privacy.zh-CN.md)

> Status: Reference
>
> 已完成的 shared-speech（共享语音）里程碑保留了现有隐私行为，未引入新的凭据
> 存储、Local History（本地历史）模型或同步机制。后续每个里程碑仍须遵守下方的
> 审阅保证。

## 隐私原则

用户应当能够判断：

- 哪些数据会离开本机；
- 哪个提供商会收到这些数据；
- 哪些数据会保存在本地；
- 哪项权限启用了每种行为。

文档必须区分当前事实和 Deferred（已推迟）的目标策略。

## 当前实现事实

### 外部请求

- 已配置的远程 ASR 会接收录制的或用户提供的音频。
- 已配置的 AI Post-processing（AI 后处理）会接收 Raw Transcript（原始转写文本）
  和提示词上下文。
- Desktop（桌面应用）路径会在录音结束后、远程处理开始前检查
  `privacy.allow_external_requests`。
- CLI（命令行工具）的网络命令当前不执行该设置。

### Provider Credential（提供商凭据）

- 当前推荐的设置方式是在 TOML 中保存环境变量名称，并把秘密信息保留在进程
  环境中。
- 当前 Config（配置）也允许在 TOML 中保存明文 API key。
- 设置 UI 会遮蔽密钥字段，但持久化到 TOML 中的值没有加密。
- `config show` 会不经脱敏地输出直接保存的 API key，通过 `config set` 传入 key 还
  可能使其留在 shell history 中。复制或分享配置输出前必须进行脱敏。

已完成的 shared-speech（共享语音）重构没有悄然改变这些行为，也没有声称当前具有
更强的保护。后续任何凭据处理变更都必须单独进行行为与隐私审阅。

### JSONL 历史原型

- 当前默认值是 `privacy.history_enabled = true`；成功插入后，Desktop（桌面应用）
  路径会把 `history.jsonl` 追加到 Active（活动）Config（配置）文件旁边，或追加到
  配置的路径中。
- 当前每个条目同时保存原始文本、最终文本、毫秒时间戳和 `desktop` provider 标记。
- 该文件仅支持追加，当前没有查询、删除、保留数量或恢复界面。
- CLI（命令行工具）的语音命令不会写入这份 JSONL 历史。
- 设置 `privacy.history_enabled = false` 可禁用这项保留。

这是 JSONL 历史原型，不是 [CONTEXT](../../CONTEXT.zh-CN.md) 中定义的完整 Local
History（本地历史）概念。

### 音频

- 在 Active（活动）会话期间，录制的音频保存在内存中。
- 远程 ASR 可能通过拆分为最长 60 秒的请求来接收音频。
- 当前流程结束后，音频不会被有意保留。
- 渐进式临时 Audio Segment（音频片段）和 Audio Retention（音频保留）尚未实现。

### 剪贴板

- Windows 插入会暂时把 Final Text（最终文本）放到剪贴板上，并发送模拟的粘贴
  输入。
- 可选恢复功能会保留之前的文本剪贴板内容。
- 图像、文件、富格式以及目标是否确实接受了粘贴，目前还没有得到完整保留或验证。

## 持续重构保证

已完成的 shared-speech（共享语音）里程碑：

- 没有收集新数据；
- 没有引入额外的外部请求；
- 保持了 Provider Credential（提供商凭据）的优先级和存储方式；
- 保持了 JSONL 历史行为。

对于后续每个里程碑：

- 除非 Active（活跃）里程碑明确列出并获批，否则影响隐私的行为保持不变；
- 未经单独的行为审阅，不扩大任何隐私开关的作用范围；
- 在任何隐私行为变化之前更新文档。

## Deferred（已推迟）的目标策略

以下策略仍处于 Deferred（已推迟）状态：

- 在 Voice Input（语音输入）开始前执行 External-request Blocking（外部请求阻断）；
- 默认启用 Local History（本地历史），保留最近十条 Final Text（最终文本）结果、
  时间戳、Workflow（工作流）身份和处理状态；
- 允许用户选择更大的保留数量；
- 将 Raw Transcript（原始转写文本）保留设置为单独的 opt-in（主动选择加入）项；
- Audio Retention（音频保留）默认关闭，并与 Local History（本地历史）的生命周期
  绑定；
- Pending Voice Input（待处理语音输入）、Pending Refinement（待处理润色）和
  Insertion Recovery（插入恢复）；
- 完整的 Portable Installation（便携式安装）数据契约。

[ADR-0006](../adr/0006-store-workflows-in-sqlite-and-modules-as-toml.zh-CN.md)
规定：如果实现相应存储设计，则把明文 Provider Credential（提供商凭据）保存在
本地 SQLite 中。改变该存储决策需要一份 superseding ADR（取代它的新 ADR）。

显式风险确认仍是本文档中一项独立的 Deferred（已推迟）产品策略；ADR-0006 并未
决定该策略。由应用管理的跨平台加密仍是以后可能采用的升级方案。

任何未来的凭据存储都必须避免把 Provider Credential（提供商凭据）写入日志、
Local History（本地历史）和可共享的产品 Module（产品模块）文件，并且界面必须
继续遮蔽秘密值。

## 权限

| 权限或能力 | 当前需要 | 原因 |
| --- | --- | --- |
| 麦克风 | 录音命令和 Toggle Mode（切换模式）需要 | 捕获语音 |
| 网络 | 已配置的远程 ASR 和 AI 润色需要 | 调用用户选择的提供商 |
| 剪贴板 | 当前 Windows 插入适配器需要 | 交付 Final Text（最终文本） |
| 全局快捷键 | Windows Toggle Mode（切换模式）需要 | 开始和停止 Voice Input（语音输入） |
| 辅助功能或输入法集成 | 当前原型未使用 | 未来可能用于 Direct Insertion（直接插入） |
| 开机启动 | 不属于当前范围 | 当前没有需要 |

每项未来权限都必须在实现前明确其用户可见收益和降级行为。

## Deferred（已推迟）的同步方向

文件夹同步、WebDAV、加密数据包和 Local History（本地历史）同步都尚未排期。
它们不属于当前隐私承诺或重构完成标准。
