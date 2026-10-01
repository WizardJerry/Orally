# 重构架构基线

[English](refactor-baseline.md) |
[简体中文](refactor-baseline.zh-CN.md)

> Status: Active
>
> 基线分支：`refactor`
>
> 基线提交：`c452065`
>
> 基线日期：2026-08-30
>
> 目的：以共享语音处理为基础，按准入门槛逐步推进后端优先里程碑

## 1. 目的

Orally 已经具备当前使用中最重要的能力：可以将语音发送给 OpenAI 兼容的转写提供方，
通过本地处理或 AI Post-processing 进行完善，并交付为可用文本。Windows Desktop App
还支持 Toggle Mode、托盘进程、浮层和剪贴板插入。

这次重构并非试图把原型变成一个面面俱到的商业产品。它是一条学习路线：在保留现有
实用行为的同时，改进职责归属、代码深度、依赖方向与可维护性。

第一条实现路线从共享的音频到 Final Text 接缝开始，随后先迁移 CLI，再迁移 Desktop
App。这改变的是实现顺序，而不是产品定位：Windows Desktop App 仍是面向最终用户的
产品，CLI 仍是面向开发者的工具。

## 2. 范围与权威性

本文档是当前重构唯一的执行计划。它负责：

- 结构性工作必须保留的实现快照；
- 活跃阶段及其顺序；
- 每个阶段可以影响的文件；
- 明确的非目标、验证、回滚与停止点；
- 在行为或功能工作开始前必须进行的文档审阅。

2026-08-30，用户明确要求为现有 ASR 与后处理路径建立一个共享 crate。该方向激活了
下方的共享语音里程碑，并取代了此前只针对 CLI 的提案。

在审阅该结果后，用户确立了持续的后端优先目标：在 `refactor` 上以小型、有测试支持的
里程碑实现已记录的产品能力；在出现后续需求前保持 UI 简单；并以英文和简体中文维护
面向用户的文档与决策文档。这一项目级方向授权对完整路线进行规划，但每项 Reference
或 Deferred 能力在开始实现前，仍必须通过功能准入门槛。

使用[文档导航](../README.zh-CN.md)回答其他类型的问题。特别是：

- 代码和现有测试是当前行为的权威来源；
- [CONTEXT](../../CONTEXT.zh-CN.md)提供产品词汇；
- 已接受的 [ADR](../adr/) 保留难以作出的决策；
- 产品文档描述稳定意图与延期的可能性；
- GitHub Issues 跟踪候选缺陷和工作，但其本身既不授权也不安排这些工作。

当某项能力被延期时，已接受的 ADR 仍然有效。延期不等于否决。后续决策若与某个 ADR
冲突，必须明确取代它。

## 3. 当前实用切片

仓库包含两条相关但行为不同的流程。

### 3.1 Windows Desktop App

Desktop App 是当前日常使用的参考实现：

    Global Trigger 或托盘操作
      -> 切换录音
      -> 完整 PCM16 音频保存在内存中
      -> External-request Blocking 检查
      -> orally-speech：远程 ASR，在内部拆分为最长 60 秒的请求
      -> orally-speech：原始输出、配置的 Local Basic Cleanup 或 AI Post-processing
      -> 恢复原先的前台窗口
      -> 剪贴板粘贴
      -> 成功插入后可选地追加 JSONL history

重要细节：

- `output.raw` 绕过两条完善路径。
- 仅在 Desktop 流程中，`postprocess.mode` 选择 AI Post-processing。
- `postprocess.fallback_to_builtin` 仅在
  `OpenAiChatPostprocessor::process` 返回错误时适用。缺少凭据或后处理器构造错误发生在
  该 fallback 之前，并会使 Voice Input 失败。
- JSONL history 追加是可选的。history 写入失败会在插入之后报告，且不会撤销插入。
- Tauri 入口点负责录音、隐私、插入和 history 策略；`orally-speech` 负责共享的音频到
  Final Text 部分。此流程不使用 `orally_core::OrallyPipeline`。

### 3.2 CLI 命令

当前 CLI 行为是初始阶段的兼容性基线：

| 命令 | 当前行为 |
| --- | --- |
| `demo` | 演示文本 ASR、`BuiltInTextProcessor`、终端输出 |
| `process` | 默认使用 `BuiltInTextProcessor`；仅在显式传入 `--ai` 时使用 LLM |
| `record` | CLI 专用的固定时长麦克风录音，写入 WAV |
| `transcribe` | WAV、远程 ASR、原始文本或 `BuiltInTextProcessor`、输出或可选插入 |
| `dictate` | 固定时长录音、远程 ASR、原始文本或 `BuiltInTextProcessor`、输出或可选插入 |
| `listen` | Windows Toggle Mode、远程 ASR、原始文本或 `BuiltInTextProcessor`、强制插入 |
| `config` | TOML 路径、显示、初始化和设置操作 |

共享语音里程碑不得标准化或修复以下兼容性细节：

- `transcribe`、`dictate` 和 `listen` 不从 `postprocess.mode` 选择 AI
  Post-processing，不强制执行 `privacy.allow_external_requests`，也不写入 JSONL
  history。
- `listen` 复用 `DictateOptions`，随后强制设定 `insert = true`。它接受并解析
  `seconds`，但 Toggle 录音不使用该值。
- `listen` 始终注册 `Ctrl+Alt+Space`；它不读取 `hotkey.preset`。
- `record` 不加载 `AppConfig`、`audio.record_output` 或
  `audio.dictate_seconds`。它自身的默认值是三秒和 `orally-recording.wav`。
- `process --ai` 不使用 `postprocess.fallback_to_builtin`。除非显式提供 prompt
  标志，否则它使用由 cleanup、outline 或 translate 任务选择的 prompt，而不是
  `postprocess.system_prompt` 和 `postprocess.user_template`。
- `process` 有自己的 locale 和 show-changes 默认值，而不是使用 `OutputConfig`。
- 与语音命令一样，`process --ai` 不强制执行
  `privacy.allow_external_requests`。
- CLI 输出选项没有环境变量层。
- `demo` 忽略尾随参数，因此 `demo --help` 仍会运行演示并以 `0` 退出。`config path`
  同样忽略尾随参数。
- 硬编码的 cleanup 字典、process 任务别名、请求 endpoint、请求形状、prompt 和
  timeout 都是这条结构性路线的兼容性事实。
- `process --ai` 任务是开发者操作，不是 Workflow 或产品 Module 实现。

这些特性可能值得后续修改。当前路线会保留它们，直到单独的行为审阅作出其他决定。

### 3.3 当前退出契约

CLI 迁移必须保留以下进程契约：

| 情形 | 当前结果 |
| --- | --- |
| 没有命令或未知顶层命令 | 将全局帮助打印到 stdout；以 `0` 退出 |
| 命令成功 | 以 `0` 退出 |
| 参数被命令解析器拒绝，包括大多数命令级 `--help` 标志 | 打印错误或用法；以 `2` 退出 |
| Config、文件 I/O、音频、网络、处理或插入失败 | 打印到 stderr；以 `1` 退出 |

即便当前接口并不常规，确切文本及其 stdout/stderr 目标也是兼容性快照的一部分。

## 4. 当前仓库架构

共享语音迁移之后的 Rust 依赖方向是：

    orally-cli ---------\
                         -> orally-speech -> orally-asr -> orally-audio + orally-core
    orally-desktop -----/                \-> orally-llm -> orally-core
                                        \-> orally-core

    orally-cli 还依赖 -> orally-audio + orally-config + orally-core
                     -> orally-llm（仅文本的 process --ai）
                     -> orally-windows
    orally-desktop 还依赖 -> orally-audio + orally-config + orally-core
                         -> orally-storage + orally-windows

| 代码区域 | 当前职责 | 基线评估 |
| --- | --- | --- |
| `orally-core` | 共享类型和接口、Local Basic Cleanup、演示适配器、旧演示 pipeline | 稳定基础；旧 pipeline 仍只用于演示 |
| `orally-audio` | CPAL 捕获、PCM16、指标、WAV、内存内拆分 | 实用的适配器实现 |
| `orally-asr` | Multipart 和 chat-audio 远程 ASR 实现 | 相对深的 crate |
| `orally-llm` | OpenAI 兼容的文本完善 | 实用的外部适配器 |
| `orally-speech` | 类型化运行时 plan、ASR 构造，以及 Raw/Local/AI 完善编排 | CLI 与 Desktop 共用的深层接缝 |
| `orally-config` | TOML schema、preset、路径、序列化、字符串更新 | 在共享语音里程碑期间保留原状 |
| `orally-storage` | 仅追加的 JSONL history 原型 | Desktop 专用原型，不是目标 Local History |
| `orally-windows` | Win32 trigger 和剪贴板插入 | 平台适配器 |
| `orally-cli` | 解析、配置解析、录音、终端输出和可选插入 | 位于共享语音接缝之上的开发者适配器 |
| `orally-desktop` | Tauri shell、录音/隐私策略、插入、history 和 UI | 位于共享语音接缝之上的最终用户适配器 |

`OrallyPipeline` 是一个浅层、仅用于演示的源代码 module：它的接口暴露多个输入，而
实现只负责在 ASR、处理和插入之间转发。`orally-speech` 是共享的生产接缝，但删除旧的
演示 pipeline 仍是一个单独的清理决策。

Desktop WebView 还会将演示用快捷键选择保存在 `localStorage` 中。这些选择不是运行时的
`hotkey.preset`，也不属于完整的 Portable Installation 契约。

## 5. 能力状态

| 能力 | 状态 |
| --- | --- |
| CPAL 麦克风录音与 WAV 支持 | Implemented |
| Multipart 和 chat-audio 远程 ASR | Implemented |
| 通过 `BuiltInTextProcessor` 实现的 Local Basic Cleanup | Implemented |
| OpenAI 兼容的 AI Post-processing | Implemented |
| CLI 与 Desktop 共用的 ASR 和完善编排 | Implemented |
| Windows Toggle Mode、托盘、浮层、剪贴板插入 | Implemented prototype |
| 可执行文件旁的 Config | Implemented config-local prototype |
| JSONL history 追加 | Implemented Desktop prototype |
| VAD 驱动的停止 | Not implemented; metrics only |
| 渐进式 Audio Segment 录音 | Not implemented |
| Workflow、产品 Module、Service Connection | Not implemented |
| SQLite 产品存储 | Not implemented |
| 完整 Portable Installation 契约 | Not implemented |
| Pending states 和 Insertion Recovery | Not implemented |
| Windows TSF 输入法 | Scaffold only |
| Android、macOS、Linux shell | Not implemented |

开放的[长录音 issue #1](https://github.com/WizardJerry/Orally/issues/1)与
[ADR-0007](../adr/0007-stream-long-recordings-through-audio-segments.zh-CN.md)相关。
当前 ASR 请求拆分并不是渐进式 Audio Segment 录音：完整录音会先保留在内存中，然后
才创建请求。

### 5.1 当前事实与延期产品术语

| 区域 | 当前事实 | 产品方向或延期方向 |
| --- | --- | --- |
| 便携数据 | 仅支持可执行文件本地的 `config.toml` | Portable Installation 携带所有持久化产品数据 |
| History | Desktop 可以把原始文本和最终文本追加到 JSONL | Local History 拥有经过审阅的保留策略、需主动选择启用的 Raw Transcript、状态与恢复 |
| 长录音 | 完整音频保存在内存中，随后才拆分 ASR 请求 | Audio Segment 被渐进写入，并经过有意的清理或保留 |
| 完善 | CLI 语音命令使用原始文本或 `BuiltInTextProcessor`；仅 `process --ai` 显式使用 AI | 产品语言将 AI Post-processing 认定为首选；开发者向 CLI 是否及何时采用该策略，由单独审阅决定 |
| Trigger | Desktop 启动时读取固定 preset；`listen` 硬编码一个快捷键 | Global Trigger 捕获、冲突验证和即时注册被延期 |
| 提供方配置 | 扁平 TOML section 和可选环境变量 | Service Connection 和 Default Service Slot 被延期 |

[CONTEXT](../../CONTEXT.zh-CN.md)定义词汇，而不代表实现已经完成。已接受的 ADR 定义
决策，而不代表当前里程碑。

### 5.2 基线提交时的验证

在 `c452065`：

- Rust workspace 的全部 40 项测试通过；
- `cargo fmt --all -- --check` 通过；
- strict Clippy 在 `orally-config` 报告 `derivable_impls`；
- strict Clippy 在 `orally-cli` 报告 `items_after_test_module`；
- strict Clippy 在 `orally-desktop` 报告 `enum_variant_names`；
- Desktop Rust target 没有行为测试；
- frontend 没有 test、lint、typecheck 或端到端 script。

这些 lint 发现是共享语音范围之外已记录的债务。更广泛的测试骨架被有意延期；此里程碑
使用聚焦的 crate 测试、现有测试和手动冒烟检查。

### 5.3 共享语音停止点的验证

在 2026-08-30 的 S4 停止点、D0 添加双语文档配对之前：

- Rust workspace 的全部 53 项测试通过，其中包括 13 项聚焦的 `orally-speech` 测试；
- `cargo fmt --all -- --check` 通过；
- `orally-speech`、`orally-core`、`orally-asr` 和 `orally-llm` 的 strict
  Clippy 通过；
- 基线中的三项 strict-Clippy 发现仍保留在 Config、CLI 和 Desktop 中，没有把无关清理
  合并进此里程碑；
- Desktop Rust target 构建成功；
- 全部八项非网络 CLI 冒烟用例均保留预期内容与原生退出状态；
- 仓库文档中的全部 40 个本地 Markdown 链接均可解析。

## 6. 目标架构

已实现的目标是为现有音频到 Final Text 路径提供一个共享 Rust crate：

    orally-cli ---------\
                         -> orally-speech -> orally-asr -> orally-audio + orally-core
    orally-desktop -----/                \-> orally-llm -> orally-core
                                        \-> orally-core

`orally-speech` 负责类型化运行时 plan、ASR protocol 解析与自动推断、具体 ASR 适配器
构造、Raw/Local/AI 完善选择、AI 到本地的 fallback、默认字典组装，以及结构化 outcome。

它的运行时接口被有意保持狭窄：

    SpeechProcessor::new(SpeechPlan) -> Result<SpeechProcessor, SpeechBuildError>
    SpeechProcessor::process(AudioInput) -> Result<SpeechOutcome, OrallyError>

`SpeechOutcome` 保留 Raw Transcript、Final Text 和 cleanup changes。`SpeechPlan` 是运行时
输入，而不是 Workflow、产品 Module、Service Connection 或持久化 schema。

该接缝由两个真实调用方证明其合理性。删除它会使 protocol 推断、提供方构造、完善策略、
fallback 顺序和结果组装重新回到 CLI 与 Desktop 两处。

该 crate 不负责录音、文件读取、Toggle Mode、Global Trigger、External-request
Blocking、终端输出、进程退出、Direct Insertion、前台窗口恢复、Local History、托盘、
浮层或 UI。CLI 与 Desktop 将各自不同的配置来源和当前行为映射到 `SpeechPlan`；共享
crate 不会悄然使这些行为变得相同。

## 7. 重构规则

每个实现阶段都必须遵守以下规则：

1. 除非先审阅了单独的行为决策，否则保留可观察行为。
2. 每次只进行一项主要结构变更。
3. 不要在同一阶段组合文件移动、行为变更、依赖变更和功能工作。
4. 没有当前需求时，不要新增 crate、trait、依赖、framework 或配置键。
5. 保持 Desktop 可用，并且不要在 CLI 阶段重写它。
6. 不要只为让目录树看起来完整而重命名 crate。
7. 在每个阶段结束后停止、验证，并在开始下一阶段之前对其进行审阅。
8. 让每个实现阶段都保持为可提交、可独立审阅的状态。仅在用户请求或批准时创建
   commit；创建 commit 时，每个 commit 只包含一个主要阶段。如果完成检查失败，应修改
   该阶段，而不是把修复隐藏在下一阶段中。
9. 只将产品术语 `Module` 用于产品概念；代码结构使用 Rust crate 或源代码 module。
10. 除非后续文档审阅明确引入该步骤，否则不要在这条路线中添加行为测试骨架。

## 8. 明确的非目标

初始路线不会：

- 更改 CLI 命令名称、flag、alias、help、output、退出行为或默认值；
- 让 CLI 语音命令采用 Desktop 的 AI、privacy、fallback、insertion 或 JSONL history
  行为；
- 标准化现有 flag、environment、TOML 和默认值的 precedence；
- 更改 ASR 或 LLM endpoint、请求形状、protocol、timeout 或 prompt；
- 更改硬编码字典、process 任务 alias、`listen` 快捷键、强制插入或未使用的
  `listen --seconds` 行为；
- 重新设计 TOML 配置或凭据；
- 升级第三方依赖或重写无关的 `Cargo.lock` 条目；
- 在已批准阶段中将 ASR 选择移入公共 factory；
- 迁移仅文本的 `process --ai` 诊断命令；
- 在此里程碑中删除或重命名仅用于演示的 `OrallyPipeline` 集群；
- 添加 Workflow、产品 Module、Service Connection 或 Active Workflow；
- 添加 SQLite 或迁移 JSONL history 原型；
- 完成 Portable Installation、Local History、Audio Retention、pending states 或
  Insertion Recovery；
- 实现渐进式 Audio Segment、First-run Setup、任意 Global Trigger 捕获或替代 Desktop
  UI；
- 实现 TSF、Android、macOS、Linux、本地 ASR、sync、plugin、encryption、新 CLI
  framework 或新 async runtime。

## 9. 阶段协议

每个阶段开始前：

1. 重新阅读该阶段以及上方的兼容性事实；
2. 确认上一阶段具有可审阅的 diff 和已记录的验证；
3. 在该阶段可能影响 stdout、stderr 和退出码时，捕获具有代表性的这些内容；
4. 仅实现列出的变更；
5. 运行列出的验证；
6. 在讨论下一阶段前停止并报告结果。

先运行一次 `cargo build -q -p orally-cli`，再执行下表中的原生可执行文件命令。对于两个
解析错误用例，必须直接运行二进制文件，因为 `cargo run` wrapper 会把子进程失败报告为
自身的退出码 `1`，而不会保留 CLI 的原生退出码 `2`。

标准非网络冒烟矩阵是：

| 命令 | 预期退出结果 |
| --- | --- |
| `.\target\debug\orally-cli.exe` | `0`，全局帮助输出到 stdout |
| `.\target\debug\orally-cli.exe unknown-command` | `0`，相同的全局帮助输出到 stdout |
| `.\target\debug\orally-cli.exe process "嗯 今天 写 一封 邮件"` | `0`，内置处理后的文本输出到 stdout |
| `.\target\debug\orally-cli.exe demo --help` | `0`，运行演示并忽略尾随 flag |
| `.\target\debug\orally-cli.exe config path unexpected` | `0`，打印配置路径并忽略尾随值 |
| `.\target\debug\orally-cli.exe record --seconds nope` | `2`，解析错误和全局帮助 |
| `.\target\debug\orally-cli.exe process --help` | `2`，当前命令帮助错误路径 |
| `.\target\debug\orally-cli.exe transcribe --file Z:\\orally-missing.wav --api-key dummy --model dummy` | `1`，文件错误且不发出网络请求 |

将输出与该阶段开始前立即进行的捕获比较。这些检查描述了现有行为，但不引入测试
framework。

## 10. 已完成的共享语音里程碑

2026-08-30 的用户请求激活了此里程碑，并取代了此前仅针对 CLI 的序列。每次源代码迁移
仍是一个可独立审阅的步骤；任何阶段都不会悄然授权下一阶段。

### S0 — 共享接缝决策

- **Status:** Completed.
- **目标：** 记录 CLI 与 Desktop 共用的最小接口。
- **决策：** 添加 `crates/orally-speech`；使用类型化的 `SpeechPlan`、唯一的
  `SpeechProcessor::process` 运行时入口和结构化的 `SpeechOutcome`。
- **行为规则：** 调用方将现有策略映射到 plan。CLI 语音命令保持 Raw 或 Local Basic
  Cleanup；Desktop 保留 Raw、Local、AI 和 AI 到本地的 fallback。
- **归属规则：** External-request Blocking、录音、插入、Local History 和 UI 留在 crate
  之外。
- **完成条件：** 此基线为 Active，且接口已通过其 process 接缝获得聚焦测试。

### S1 — 添加 `orally-speech`

- **Status:** Completed.
- **目标：** 在迁移调用方之前实现共享 crate。
- **允许的变更：** workspace membership、新 crate、必要的本地 workspace 依赖条目，
  以及新 crate 中的聚焦测试。
- **接口：** 生产调用方从类型化运行时 plan 构造 `SpeechProcessor`，并调用
  `process(AudioInput)`。
- **由接缝隐藏的实现：**
  - ASR protocol alias 与 `auto` 推断；
  - 具体 multipart 或 chat-audio 适配器的构造；
  - direct-or-named-environment Provider Credential 解析；
  - Raw Transcript 保留；
  - Raw、Local Basic Cleanup 或 AI Post-processing 选择；
  - AI process 失败时 fallback 到 Local Basic Cleanup；
  - 当前默认字典和 `ProcessInput` 组装；
  - 结构化 Raw Transcript、Final Text 和 cleanup changes。
- **兼容性约束：**
  - 只有 ASR 成功后才解析 AI 凭据和 AI 适配器；
  - 缺少 AI 凭据或 AI 适配器构造错误不会触发 fallback；
  - fallback 仅覆盖 AI `process` 错误；
  - ASR 失败时绝不运行完善处理；
  - 处理保持同步且阻塞。
- **不得改动：** 现有调用方、持久化配置 schema、外部请求形状、第三方依赖版本或旧演示
  pipeline。
- **验证：** 聚焦接口测试覆盖 Raw、Local、AI 成功、AI 失败且有/无 fallback、ASR
  失败、protocol 解析/推断、credential redaction 和调用顺序；新 crate 的 strict Clippy
  通过。
- **回滚：** 如果该接口比其隐藏的重复行为更宽，则移除 workspace member 和新 crate。

### S2 — 迁移 CLI 语音路径

- **Status:** Completed.
- **目标：** 让 `transcribe`、`dictate` 和 `listen` 在取得音频后调用
  `orally-speech`。
- **允许的变更：** CLI 源代码和 manifest，以及必要的本地 lockfile 条目。
- **必须保留的行为：**
  - CLI flag/environment/TOML precedence 仍由 CLI 解析负责；
  - `--raw` 选择 Raw；其他所有语音路径都选择 Local Basic Cleanup；
  - CLI 不采用 `postprocess.mode`、AI fallback、
    `privacy.allow_external_requests` 或 JSONL history；
  - 文件读取、录音、Toggle Mode、终端输出、退出码、changes、强制 `listen` 插入和剪贴板
    插入仍留在 CLI 中。
- **不得改动：** `record`、仅文本的 `process --ai`、命令语法、prompt、默认值、Desktop
  或持久化配置。
- **验证：** 现有 CLI 测试和完整冒烟矩阵通过，且不存在可观察差异。CLI 的 strict
  Clippy 仍停在基线 `items_after_test_module` 债务上；此阶段不混入该项无关清理。
- **回滚：** 如果 stdout、stderr、退出状态、选项 precedence 或插入行为发生变化，则仅
  回滚 CLI 迁移。

### S3 — 迁移 Desktop 语音处理

- **Status:** Completed.
- **目标：** 用 `orally-speech` 替换 Desktop 中重复的 ASR 构造和完善函数。
- **允许的变更：** Desktop Rust 源代码和 manifest，以及必要的本地 lockfile 条目。
- **必须保留的行为：**
  - External-request Blocking 仍发生在录音之后、processor 构造之前；
  - `output.raw` 绕过完善处理；
  - `postprocess.mode = "llm" | "ai"` 选择 AI Post-processing；
  - 其他 mode 使用 Local Basic Cleanup；
  - fallback 范围和消息保持不变；
  - 前台窗口恢复、Direct Insertion、JSONL history 顺序、浮层和错误呈现仍由 Desktop
    负责。
- **不得改动：** Tauri command、托盘、hotkey、录音生命周期、UI、配置 schema、History
  storage 或剪贴板适配器。
- **验证：** Desktop 编译通过，workspace 测试通过，而且重复的 `AsrProtocol`、
  `AsrOptions`、provider factory、refinement function 和 dictionary 已从 Desktop 中
  移除。
- **回滚：** 如果处理顺序、插入或 history 行为发生变化，则仅回滚 Desktop 迁移。

### S4 — 集成停止点

- **Status:** Completed；已完成审阅，并作为持续重构的基础。
- **目标：** 验证共享接缝，并在无关清理开始前停止。
- **允许的变更：** 仅在验证后更新事实性文档。
- **验证：** workspace 测试、格式检查和 CLI 冒烟矩阵通过；`orally-speech`、
  `orally-core`、`orally-asr` 和 `orally-llm` 的 strict Clippy 通过；Desktop 构建
  成功。基线中记录的三项 strict-Clippy 发现仍原样保留在 Config、CLI 和 Desktop，
  而未被并入此里程碑。
- **明确停止：** 不要在此里程碑中删除 `OrallyPipeline`、拆分 CLI 文件、标准化
  CLI/Desktop 行为、添加 async/streaming 或开始新功能。
- **下一次审阅：** 单独决定是否应移除演示 pipeline，以及 CLI 语音命令是否应采用
  Desktop 的 AI 或 privacy 策略。

## 11. 延期的设计承诺

已接受的 ADR 为这些能力进入活跃范围的那一天保留以下决策：

| ADR | 延期能力 |
| --- | --- |
| [ADR-0002](../adr/0002-compose-modules-into-one-request.zh-CN.md) | 有序的产品 Module 组合成一次 AI 请求 |
| [ADR-0003](../adr/0003-share-provider-connections-across-workflows.zh-CN.md) | Service Connection 在 Workflow 之间共享 |
| [ADR-0004](../adr/0004-keep-portable-data-with-the-application.zh-CN.md) | 完整 Portable Installation 数据契约 |
| [ADR-0006](../adr/0006-store-workflows-in-sqlite-and-modules-as-toml.zh-CN.md) | SQLite 存储 Workflow、connection、credential 和 Local History 记录；产品 Module 仍为 TOML |
| [ADR-0007](../adr/0007-stream-long-recordings-through-audio-segments.zh-CN.md) | 长录音使用渐进式 Audio Segment |

[ADR-0001](../adr/0001-ordered-workflow-stages.zh-CN.md) 和
[ADR-0005](../adr/0005-use-hybrid-portable-storage.zh-CN.md) 是历史记录，因为它们已被
取代。

其他延期的可能性包括 TSF、Android、macOS、Linux、本地 ASR、sync、provider plugin、
personal dictionary、encrypted backup 和完整的 Local History 接口。

## 12. 功能准入门槛

任何延期能力进入活跃状态前：

1. 说明用户问题，以及当前实用切片为何不足；
2. 重新阅读产品、隐私、产品语言和相关 ADR；
3. 选择能够解决问题的最小行为；
4. 列出明确的非目标；
5. 更新此基线；
6. 停止并交由用户审阅；
7. 仅在基线获得批准且为 Active 后实现。

该门槛避免学习与功能增长坍缩为一次不可审阅的变更。

## 13. 已完成的双语文档里程碑

持续目标从文档基线开始，使后续每项功能都能以英文或简体中文审阅，同时不改变其范围
或停止条件的含义。

### D0 — 建立配对契约

- **Status:** Completed；在进入下一项后端里程碑前等待用户审阅。
- **目标：** 让仓库拥有的每份用户、产品、工程和决策文档都有一个英文 canonical 文件
  和一个简体中文配对文件。
- **命名：** 保留现有英文路径；为中文配对文件在扩展名前添加 `.zh-CN.md`。
- **必需配对：** 根 README、产品语言、文档导航、产品定义、架构、隐私、路线图、
  Grilling 目录指南、重构基线、Windows IME 研究笔记，以及 ADR-0001 至 ADR-0007。
- **例外：** 由 Grilling skill 生成的单篇探索性文档可以继续只使用英文。如果某项结论
  成为需求或决策，则将其提升到双语产品文档、基线或 ADR 中。
- **翻译不变量：** 保持状态值、命令、路径、配置键、环境变量、错误文本、代码标识符、
  ADR 状态和取代关系不变。
- **仅供 agent 使用的文件：** `.agents/` 保持仅英文，以免机器指令产生第二份逐渐偏离的
  副本。
- **验证：** 每组必需配对都存在、每组都有双向语言导航、本地 Markdown 链接可解析、
  ADR status 一致，并且英文/中文 heading structure 已针对覆盖范围完成审阅。
- **完成证据：** `scripts/check-docs.ps1` 对 17 组必需语言配对和 151 个本地链接检查
  通过；heading structure 与 ADR status 一致；翻译已接受独立的覆盖范围和术语审阅。
- **非目标：** D0 不包含任何面向用户的行为、依赖、持久化 schema 或 UI 变更。
- **停止：** 在激活下一项后端能力前，完成并审阅 D0。

## 14. 候选后端序列

这是一条考虑依赖关系的规划序列，并非笼统的实现授权。每一项都通过功能准入门槛获得
自己的 Active 里程碑：

1. 通过彼此分离且行为中立的里程碑，强化当前后端的证据：依次验证 JSONL append、
   Config I/O 与 path precedence、ASR/LLM loopback HTTP contract，最后建立可重复的手动
   Windows adapter 冒烟检查；
2. 将孤立的结构债务与 strict-Clippy 债务同这些 contract-test 里程碑分开处理；
3. 通过渐进式 Audio Segment 改进当前核心路径；
4. 分别实现 Windows 交互能力：依次为 Global Trigger、Automatic Stop、First-run Setup；
5. 按顺序引入领域行为：产品 Module、Service Connection、Workflow，随后是 Active
   Workflow；
6. 仅在这些领域接口稳定后添加 ADR-0006 持久化模型；
7. 在该持久化基础上构建 Local History、pending states、Insertion Recovery 和 Audio
   Retention；
8. 完成 Portable Installation 契约；
9. 仅在 Windows 后端稳定后，再重新考虑 TSF、本地 ASR、其他平台 shell、sync、plugin
   和 encryption。

## 15. 后端证据里程碑草案

本节是下一次审阅的提案。在用户批准前，它不是 Active。

### E1 — 固化 JSONL append 契约

- **Status:** Draft.
- **问题：** Desktop 当前通过 `HistoryStore` 追加成功的 Voice Input 结果，但唯一的
  storage 测试只覆盖路径推导。父目录创建、JSON 编码、追加顺序和失败传播都在被依赖，
  却没有直接证据。
- **目标：** 通过现有接口刻画 `HistoryStore::append` 的当前行为，不改变产品行为。
- **允许的变更：** 聚焦的 `orally-storage` 测试，以及必要时的私有测试支持。当标准库
  临时路径足够时，不增加新依赖。
- **必需用例：** 创建缺失的父目录；写入一个有效 JSONL 对象；追加多个条目且不覆盖并
  保持顺序；保留中文文本并转义内嵌换行；对无效或不可写目标返回 `OrallyError`。
- **接口：** 保持 `HistoryStore::new`、`HistoryStore::append`、
  `HistoryStore::path` 和 `default_history_path` 不变。
- **非目标：** 不增加 history 读取、查询、删除、保留数量、SQLite、Workflow identity、
  Raw Transcript opt-in、Audio Retention、Desktop UI 或持久化 schema 变更。
- **验证：** 聚焦 storage 测试、workspace 测试、格式检查、严格 `orally-storage`
  Clippy，以及未变化的 CLI 冒烟行为全部通过。
- **停止：** 独立报告 E1，再起草 Config I/O 测试或其他后端能力。
