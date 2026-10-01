# Orally 路线图

[English](roadmap.md) | [简体中文](roadmap.zh-CN.md)

> Status: Reference
>
> 本文件索引当前工作基线和尚未排期的产品可能性。
> [重构架构基线](../engineering/refactor-baseline.zh-CN.md)是唯一 Active（活动）
> 执行计划。

## 工作原型

仓库已经通过远程 ASR、Local Basic Cleanup（本地基础清理）或 AI Post-processing
（AI 后处理）、CLI（命令行工具）和一个 Windows Desktop（Windows 桌面应用）
原型，提供了可用的语音到润色文本闭环。TOML Config（配置）、JSONL 历史和可执行
文件本地配置都以有限的原型形式存在。CLI（命令行工具）的语音命令和 Desktop App
（桌面应用）现在通过 `orally-speech` 共享音频到 Final Text（最终文本）的处理。

当前目标是在保留和理解这一工作切片的同时，通过经过审阅的 backend-first（后端
优先）里程碑增加后续能力。确切的当前行为、已实现的 shared-speech（共享语音）
路线和下一个 Active（活动）步骤请参阅基线；本文不重复这些内容。

## 尚未排期的产品可能性

- Workflow（工作流）、产品 Module（产品模块）和 Active Workflow（活动工作流）；
- Service Connection（服务连接）和 Default Service Slot（默认服务槽）；
- SQLite 产品存储和 Provider Credential（提供商凭据）迁移；
- 完整的 Portable Installation（便携式安装）；
- 完整的 Local History（本地历史）、Audio Retention（音频保留）和恢复功能；
- 用于渐进式处理长录音的 Audio Segment（音频片段）；
- First-run Setup（首次运行设置）和任意 Global Trigger（全局触发器）录入；
- Windows TSF；
- Android、macOS 和 Linux shell；
- 本地 ASR；
- 个人词典；
- 文件夹或 WebDAV 同步；
- 提供商插件；
- 加密备份。

本列表记录的是可能性，而不是优先级或承诺。GitHub Issue 可以描述具体 bug 或提案，
但只有已获批准的 Active（活动）基线将其纳入范围后，该 Issue 才算已排期。

## 准入规则

任何条目进入 Active（活动）状态前，都要记录用户问题、解释当前工作切片为何不足、
选择最小有用行为、列出非目标、让产品与隐私文档和相关 ADR 达成一致，并停下来
等待用户审阅。详细 gate（准入关卡）由 Active（活动）基线负责。
