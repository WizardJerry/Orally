# Orally 产品架构

[English](architecture.md) | [简体中文](architecture.zh-CN.md)

> Status: Reference
>
> 本文档描述稳定的产品边界和 Deferred（已推迟）的产品决策。仓库事实和拟定的
> 迁移路线位于[重构架构基线](../engineering/refactor-baseline.zh-CN.md)。

## 核心价值流

产品流程被刻意保持得很小：

    触发器或命令
      -> 音频输入
      -> 语音识别
      -> 文本润色
      -> 文本交付

Local History（本地历史）、Audio Retention（音频保留）、Automatic Stop（自动
停止）、Workflow（工作流）选择、待处理状态和恢复功能可以为该流程提供支持，
但它们不是必需的处理阶段。

## 产品边界

- **driving shell（驱动 shell）** 启动 Voice Input（语音输入）、呈现状态，并通过
  其运行环境交付 Final Text（最终文本）。Windows Desktop App（Windows 桌面
  应用）和 CLI（命令行工具）是面向不同用户、具有不同交互约束的 shell。
- **audio adapter（音频适配器）** 提供有界音频，但不决定如何润色或交付文本。
- **speech-recognition adapter（语音识别适配器）** 把音频转换为 Raw Transcript
  （原始转写文本），并负责该请求所需的外部协议细节。
- **refinement policy（润色策略）** 选择原始输出、AI Post-processing（AI 后处理）
  或 Local Basic Cleanup（本地基础清理），并生成 Final Text（最终文本）。
- **delivery adapter（交付适配器）** 写入或插入 Final Text（最终文本），但不负责
  语音或润色策略。
- Config（配置）、Local History（本地历史）、待处理状态和恢复功能为价值流提供
  支持，但不会成为每个适配器的必需依赖。

与运行环境相关的生命周期属于 driving shell（驱动 shell）。只有当共享代码为
多个调用方隐藏了真实产品决策时，它才有价值；单纯的透传包装器本身并不是产品
架构边界。

## Deferred（已推迟）的设计承诺

当相应能力进入 Active（活动）状态时，accepted（已接受）的 ADR 生效：

| 决策 | 能力 |
| --- | --- |
| [ADR-0002](../adr/0002-compose-modules-into-one-request.zh-CN.md) | 产品 Module（产品模块）组合为一次 AI 请求 |
| [ADR-0003](../adr/0003-share-provider-connections-across-workflows.zh-CN.md) | Workflow（工作流）共享 Service Connection（服务连接） |
| [ADR-0004](../adr/0004-keep-portable-data-with-the-application.zh-CN.md) | 完整 Portable Installation（便携式安装）把持久数据保存在应用目录中 |
| [ADR-0006](../adr/0006-store-workflows-in-sqlite-and-modules-as-toml.zh-CN.md) | SQLite 保存 Workflow（工作流）、连接、凭据和 Local History（本地历史）记录；产品 Module（产品模块）继续使用 TOML |
| [ADR-0007](../adr/0007-stream-long-recordings-through-audio-segments.zh-CN.md) | 长录音使用渐进式 Audio Segment（音频片段） |

[ADR-0001](../adr/0001-ordered-workflow-stages.zh-CN.md) 和
[ADR-0005](../adr/0005-use-hybrid-portable-storage.zh-CN.md) 因已被取代而属于
Historical（历史）文档。Accepted（已接受）表示当相应能力被实现时，该决策继续
有效；它并不会为该能力自动排期。

## Deferred（已推迟）的行为参考

以下先前达成一致的产品行为会保留到下一次相关审阅，但本文档不会激活它们：

- 已保存的 Global Trigger（全局触发器）通过直接录入获得、会检查冲突，并且无需
  重启即可生效；
- Automatic Stop（自动停止）是可选功能，默认关闭；
- First-run Setup（首次运行设置）先验证语音识别和麦克风访问，再提供 AI
  Post-processing（AI 后处理）设置，最后启用 Global Trigger（全局触发器）；
- Built-in Workflow（内置工作流）为只读；用户必须先复制一份，之后才能自定义；
- Active Workflow（活动工作流）可以从托盘或设置中选择，并且在选择结果有影响时
  显示其名称；
- Local History（本地历史）由用户控制，并保留数量有界的 Voice Input（语音输入）
  结果；
- Audio Retention（音频保留）默认关闭，并遵循相应 Local History（本地历史）
  条目的生命周期；
- Pending Voice Input（待处理语音输入）允许用户无需重新说话即可重试语音识别，
  并在处理成功、明确丢弃、开始新录音或退出后被清除；
- Pending Refinement（待处理润色）允许在当前运行期间根据 Raw Transcript（原始
  转写文本）重试润色；跨运行恢复需要单独启用 Raw Transcript（原始转写文本）
  保留选项；
- 当 AI Post-processing（AI 后处理）失败而 Local Basic Cleanup（本地基础清理）
  成功时，Final Text（最终文本）仍可交付，同时 shell 会提示当前使用了降级路径；
- Insertion Recovery（插入恢复）会让 Final Text（最终文本）继续保留在剪贴板和
  Local History（本地历史）中，并提供重试插入或打开 Local History（本地历史）
  的操作；
- External-request Blocking（外部请求阻断）会阻止外部提供商，但不会阻止已经
  配置的本地语音识别路径；
- 完整 Portable Installation（便携式安装）不会在 Windows MVP 中创建开机启动项。

每一项在实现前都要根据当前用户问题、隐私模型和 accepted（已接受）ADR 重新
检查。

## 平台方向

平台 shell 最终可能包括 Windows TSF、Android `InputMethodService`、macOS 菜单栏
集成或 Linux IBus/Fcitx5。平台广度是一种 Deferred（已推迟）的可能性，不是产品
成功标准，也不是 Active（活动）里程碑。
