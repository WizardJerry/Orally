# Orally 产品定义

[English](README.md) | [简体中文](README.zh-CN.md)

> Status: Reference
>
> 当前工程范围：
> [重构架构基线](../engineering/refactor-baseline.zh-CN.md)

## 产品问题

Orally 帮助用户自然地说话，并在其正在写作的位置获得润色后的文本。

有用的产品闭环被刻意保持得很小：

1. 开始一次 Voice Input（语音输入）；
2. 捕获或提供音频；
3. 转写语音；
4. 润色 Raw Transcript（原始转写文本）；
5. 交付 Final Text（最终文本）。

## 当前有用范围

现有原型已经满足当前的基本需求：

- 兼容 OpenAI 的语音识别；
- Local Basic Cleanup（本地基础清理）或兼容 OpenAI 的 AI Post-processing
  （AI 后处理）；
- CLI（命令行工具）的语音命令和 Windows Desktop App（Windows 桌面应用）
  共用音频到 Final Text（最终文本）的编排；
- 用于录音、转写、处理和听写的 CLI（命令行工具）命令；
- 一个支持 Toggle Mode（切换模式）、托盘控制、浮层和剪贴板插入的 Windows
  Desktop App（Windows 桌面应用）；
- 本地 TOML 配置；
- Desktop（桌面应用）路径中的 JSONL 历史原型。

当前目标是在保留并理解这一功能切片的同时，只通过经过审阅的最小里程碑扩展
它，而不是一次性实现每一种 Deferred（已推迟）可能性。

## 产品角色

- **Windows Desktop App（Windows 桌面应用）** 是最终用户产品。
- **CLI（命令行工具）** 是面向开发者的工具，也是已完成 shared-speech（共享语音）
  迁移的第一个 driving adapter（驱动适配器）。
- **Windows IME（Windows 输入法）** 是 Deferred（已推迟）的研究方向，不是
  当前产品。

shared-speech（共享语音）迁移从 CLI（命令行工具）开始，改变的是实现顺序，而不是
产品定位。

## 产品原则

- **先保证有用**：保留现有的语音到润色文本闭环。
- **小步推进**：与其搭建完整框架，不如优先完成一个容易理解的改进。
- **由用户控制**：提供商端点、模型、提示词和密钥继续采用显式 Config（配置）。
- **隐私可见**：清楚记录哪些数据会离开本机，哪些数据会被保留。
- **在需要处原生化**：平台特有的触发和插入仍作为共享行为外围的适配器。
- **学习优先于广度**：只有当前行为确实需要时，才引入相应架构。

## 当前开发意图

Active（活动）基线定义了保持行为不变的 shared-speech（共享语音）重构，记录了
其已实现的停止点，并负责后续 backend-first（后端优先）顺序。各阶段、受影响
文件和下一次审阅只属于该基线。本产品定义本身不独立授权进一步实现。

## Deferred（已推迟）的产品广度

[路线图](roadmap.zh-CN.md)索引了未来可能的能力。它们不是缺失的验收标准；
只有已获批准的 Active（活动）基线才能排期其中某项能力。

## 扩展产品之前

在实现任何新能力之前，请审阅：

1. 本产品定义；
2. [隐私与权限](privacy.zh-CN.md)；
3. [CONTEXT](../../CONTEXT.zh-CN.md) 中的相关术语；
4. 相关的 accepted（已接受）和 superseded（已取代）ADR；
5. Active（活动）重构基线。

只有当这些文档就最小有用范围和明确非目标达成一致后，才能开始实现。
