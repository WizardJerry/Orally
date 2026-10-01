# Orally 产品

[English](CONTEXT.md) | [简体中文](CONTEXT.zh-CN.md)

Orally 将自然口述转换为适合用户当前写作上下文的润色文本。本文件只定义产品
语言；实现状态、存储选择、默认值和项目顺序由其他文档说明。

## 产品形态

**Windows Desktop App（Windows 桌面应用）**:
Windows 上用于触发 Voice Input（语音输入）、观察其状态和配置行为的主要最终
用户产品。
_避免使用（Avoid）_: Windows IME（Windows 输入法）、CLI（命令行工具）、
prototype runner（原型运行器）

**Portable Installation（便携式安装）**:
一种 Windows Desktop App（Windows 桌面应用）发行方式；复制应用目录时，
Orally 的持久数据会随该目录一起移动。
_避免使用（Avoid）_: config-only portable mode（仅配置便携模式）

**CLI（命令行工具）**:
一种面向开发者的工具，用于在最终用户产品之外运行、检查和排查 Orally。
_避免使用（Avoid）_: desktop app（桌面应用）、end-user interface（最终用户界面）

**Windows IME（Windows 输入法）**:
一种可能采用的原生 Windows 输入法集成。
_避免使用（Avoid）_: Windows Desktop App（Windows 桌面应用）

## Voice Input（语音输入）

**Voice Input（语音输入）**:
将提供的或录制的语音转换为已交付、可恢复或已丢弃文本的一次尝试。
_避免使用（Avoid）_: recording（录音）、transcription（转写）

**Toggle Mode（切换模式）**:
一种录音交互：第一次触发开始 Voice Input（语音输入），下一次触发结束录音。
_避免使用（Avoid）_: hold-to-talk（按住说话）、fixed-duration recording（固定时长录音）

**Global Trigger（全局触发器）**:
用于开始和停止 Toggle Mode（切换模式）的 Windows 组合键。
_避免使用（Avoid）_: hotkey preset（快捷键预设）

**Automatic Stop（自动停止）**:
一种可选录音行为，在持续静音后结束 Voice Input（语音输入）。
_避免使用（Avoid）_: default stop behavior（默认停止行为）

**Direct Insertion（直接插入）**:
把 Final Text（最终文本）交付到 Voice Input（语音输入）开始时处于活动状态的
写作目标中。
_避免使用（Avoid）_: preview-first output（预览优先输出）、transcript-only output
（仅转写文本输出）

**Insertion Recovery（插入恢复）**:
Direct Insertion（直接插入）失败后的回退状态；它保留 Final Text（最终文本），
以便再次尝试交付。
_避免使用（Avoid）_: successful insertion（成功插入）、transcript retry（转写重试）

**Status Overlay（状态浮层）**:
一种紧凑指示器，用于显示 Voice Input（语音输入）的状态和可用操作，但不显示
Raw Transcript（原始转写文本）或 Final Text（最终文本）。
_避免使用（Avoid）_: result preview（结果预览）、history view（历史视图）

**Pending Voice Input（待处理语音输入）**:
临时保留的已完成音频，让用户无需重新说话即可重试语音识别。
_避免使用（Avoid）_: Local History（本地历史）、Audio Retention（音频保留）

**Pending Refinement（待处理润色）**:
临时保留的 Raw Transcript（原始转写文本），用于重试文本润色。
_避免使用（Avoid）_: Final Text（最终文本）、Local History entry（本地历史条目）

## 语音与文本

**Raw Transcript（原始转写文本）**:
语音识别在润色之前生成的文本。
_避免使用（Avoid）_: Final Text（最终文本）

**Final Text（最终文本）**:
经过原始输出、AI Post-processing（AI 后处理）或 Local Basic Cleanup（本地基础
清理）后，被选中用于交付的文本。
_避免使用（Avoid）_: Raw Transcript（原始转写文本）

**AI Post-processing（AI 后处理）**:
首选的润色路径；它在保留说话者原意的同时，将 Raw Transcript（原始转写文本）
转换为可直接粘贴的 Final Text（最终文本）。
_避免使用（Avoid）_: speech recognition（语音识别）、Local Basic Cleanup
（本地基础清理）

**Local Basic Cleanup（本地基础清理）**:
当 AI Post-processing（AI 后处理）未被选择、不可用、不被允许或执行失败时使用
的本地润色路径。
_避免使用（Avoid）_: AI Post-processing（AI 后处理）

## 数据与隐私

**Config（配置）**:
一种控制应用行为的软件级设置。
_避免使用（Avoid）_: Workflow（工作流）、Module（产品模块）

**Provider Credential（提供商凭据）**:
用于访问外部语音识别或 AI 提供商的秘密信息。
_避免使用（Avoid）_: provider preset（提供商预设）、account（账户）

**External-request Blocking（外部请求阻断）**:
一种隐私控制，用于阻止通过外部提供商进行处理。
_避免使用（Avoid）_: offline transcription（离线转写）

**Local History（本地历史）**:
由用户控制、保存在本地的近期 Voice Input（语音输入）结果记录。
_避免使用（Avoid）_: audio archive（音频归档）、cloud history（云端历史）

**Audio Retention（音频保留）**:
把 Voice Input（语音输入）的音频作为其 Local History（本地历史）条目的附件
持久保存。
_避免使用（Avoid）_: temporary processing audio（临时处理音频）

**Audio Segment（音频片段）**:
用于处理较长 Voice Input（语音输入）的一段有界临时录音。
_避免使用（Avoid）_: retained recording（保留的录音）、transcript segment
（转写片段）

**First-run Setup（首次运行设置）**:
尚未配置的 Windows Desktop App（Windows 桌面应用）在开始 Voice Input（语音
输入）前必须完成的引导式配置。
_避免使用（Avoid）_: settings screen（设置页面）、normal startup（正常启动）

## Workflow（工作流）语言

**Workflow（工作流）**:
针对某一场景定义完整 Voice Input（语音输入）流程的、具名且可由用户调整的定义。
_避免使用（Avoid）_: Config（配置）、document（文档）、preset（预设）、
voice profile（语音配置档）

**Built-in Workflow（内置工作流）**:
由 Orally 提供并更新的只读 Workflow（工作流）。
_避免使用（Avoid）_: built-in Module（内置产品模块）、Custom Workflow（自定义
工作流）

**Custom Workflow（自定义工作流）**:
由用户创建、可以调整和删除的 Workflow（工作流）。
_避免使用（Avoid）_: Built-in Workflow（内置工作流）、Module（产品模块）

**Speech-recognition Stage（语音识别阶段）**:
Workflow（工作流）中必需的第一个阶段，负责把音频转换为 Raw Transcript
（原始转写文本）。
_避免使用（Avoid）_: text refinement（文本润色）、AI Post-processing（AI 后处理）

**Module（产品模块）**:
一种可复用的静态 Prompt（提示词）指令，由 Workflow（工作流）引用并包含在其
Composed Prompt（组合提示词）中。这里的 Module 指产品概念，不是 Rust source
module（Rust 源模块）。
_避免使用（Avoid）_: node（节点）、Prompt Module（提示词模块）、LLM stage
（LLM 阶段）、model（模型）、request（请求）

**Module Identifier（产品模块标识符）**:
Module（产品模块）不可变的规范身份，与其可编辑的显示名称不同。
_避免使用（Avoid）_: display name（显示名称）、Workflow name（工作流名称）

**Composed Prompt（组合提示词）**:
由 Orally 规则、按顺序排列的 Module（产品模块）和一个 Raw Transcript（原始
转写文本）组装而成的单个润色提示词。
_避免使用（Avoid）_: Module（产品模块）、Raw Transcript（原始转写文本）

**Output-language Module（输出语言产品模块）**:
一种 Module（产品模块），用于选择目标自然语言，同时保留不可翻译的标识符。
_避免使用（Avoid）_: language detection（语言检测）、ASR language hint（ASR
语言提示）

**Service Connection（服务连接）**:
一种可复用的外部提供商连接，包含端点、协议和 Provider Credential（提供商凭据）
引用。
_避免使用（Avoid）_: Workflow（工作流）、model（模型）、Module（产品模块）

**Default Service Slot（默认服务槽）**:
一种全局绑定，把默认语音识别或润色角色绑定到一个 Service Connection（服务
连接）和模型。
_避免使用（Avoid）_: Service Connection（服务连接）、Workflow（工作流）

**Active Workflow（活动工作流）**:
为下一次 Voice Input（语音输入）选中的 Workflow（工作流）。
_避免使用（Avoid）_: default provider（默认提供商）、global Config（全局配置）
