# Orally

[English](README.md) | [简体中文](README.zh-CN.md)

Orally 是一个可实际工作的 AI 辅助语音输入早期原型，也是一个面向学习的 Rust 重构项目。

当前可用的功能链路可以录制或接收音频，将其发送至兼容 OpenAI 的语音识别提供方，在本地或通过 AI Post-processing（AI 后处理）优化转写文本，并生成可直接粘贴的文本。Windows Desktop App（Windows 桌面应用）还提供 Toggle Mode、托盘控制、浮层提示和剪贴板插入功能。

## 开发方式

当前进行中的重构通过 `orally-speech` 共享现有的 ASR 与后处理链路。CLI 语音命令和 Windows Desktop App（Windows 桌面应用）都使用这一接缝，同时保留各自不同的外围策略。其目的是以小步、保持行为不变的方式改进职责归属，而不是实现产品说明中的所有能力。

开始实现工作前，请阅读[重构架构基线](docs/engineering/refactor-baseline.zh-CN.md)；有关各文档的职责，请参阅[文档导航](docs/README.zh-CN.md)。

## 当前原型

此仓库目前包含：

- `crates/orally-core`：共享的 Rust 领域类型、接口、Local Basic Cleanup（本地基础清理）、演示适配器和旧版演示流水线。
- `crates/orally-audio`：跨平台麦克风采集、PCM16 指标和 WAV 编码。
- `crates/orally-speech`：从音频到 Final Text（最终文本）的共享编排，包括 ASR 适配器选择以及 Raw、本地或 AI 优化。
- `apps/orally-cli`：面向开发者的工具，用于演示、录音、转写、文本处理、听写、热键监听和配置。
- `apps/orally-desktop`：Tauri 桌面外壳，包含 Windows 托盘图标，以及由本地 TOML 配置驱动的 Material 风格设置界面。
- `docs/product`：产品定义、架构说明、隐私模型、路线图以及未来的产品 Grilling 说明。
- `docs/engineering`：供人阅读的实现与平台说明。
- `.agents`：仅供 agent 使用的设置、skill 和工具上下文。

## 试用

```powershell
cargo run -p orally-cli -- demo
cargo run -p orally-cli -- process "嗯 今天 我想 写 一封 邮件 给 visual studio code 团队"
cargo run -p orally-cli -- record --seconds 3 --output orally-recording.wav
cargo run -p orally-cli -- transcribe --file orally-recording.wav --model <model>
cargo run -p orally-cli -- dictate --seconds 3 --model <model>
cargo run -p orally-cli -- transcribe --file orally-recording.wav --raw
cargo run -p orally-cli -- dictate --seconds 3 --show-changes
cargo run -p orally-cli -- dictate --seconds 3 --insert --paste-delay-ms 1200
cargo run -p orally-cli -- listen --paste-delay-ms 300
cargo run -p orally-cli -- config init --provider aliyun-openai
cargo run -p orally-cli -- config set output.paste_delay_ms 300
cargo run -p orally-cli -- config show
cargo run -p orally-desktop
```

`record` 命令从默认麦克风采集音频，并写入 PCM16 WAV 文件。`transcribe` 将已有 WAV 文件发送至已配置的 ASR 提供方。`dictate` 则在一个步骤中完成麦克风录音并将采集到的音频发送至 ASR。

默认情况下，远程 ASR 输出会经过 Orally 内置的后处理流水线。使用 `--raw` 可直接输出提供方返回的转写文本而不做后处理，使用 `--show-changes` 可显示本地清理操作。

在 Windows 上，使用 `--insert` 可将最终文本粘贴到当前获得焦点的输入框。CLI 会先等待 `--paste-delay-ms` 指定的时长再粘贴，以便你在录音或转写完成后将焦点切换到目标应用。

在 Windows 上使用 `listen` 可让 Orally 持续运行，并通过 `Ctrl+Alt+Space` 触发听写。按一次开始录音，再按一次相同热键即可停止录音、转写、后处理并粘贴。在终端中按 `Ctrl+C` 可停止监听器。

对于 ASR，请配置兼容 OpenAI 的转写端点：

```powershell
$env:ORALLY_ASR_API_KEY="..."
$env:ORALLY_ASR_BASE_URL="https://openrouter.ai/api/v1"
$env:ORALLY_ASR_MODEL="<model>"
cargo run -p orally-cli -- transcribe --file orally-recording.wav
```

OpenRouter 音频模型使用 chat-completions 音频输入，因此当 base URL 为 `https://openrouter.ai/api/v1` 时，Orally 会自动选择 `chat-audio` 协议。对于 Whisper 风格的 multipart 转写 API，请使用：

```powershell
$env:ORALLY_ASR_PROTOCOL="openai-transcriptions"
$env:ORALLY_ASR_BASE_URL="https://api.openai.com/v1"
```

若要通过兼容 OpenAI 的端点使用阿里云百炼（Model Studio）：

```powershell
$env:ORALLY_ASR_API_KEY=$env:ORALLY_OPENAI_COMPAT_API_KEY
$env:ORALLY_ASR_BASE_URL="https://ws-xzr3kkbjij82s72f.cn-beijing.maas.aliyuncs.com/compatible-mode/v1"
$env:ORALLY_ASR_MODEL="qwen3-asr-flash"
$env:ORALLY_ASR_PROTOCOL="chat-audio"
cargo run -p orally-cli -- transcribe --file orally-recording.wav
```

包括 `qwen3-asr-flash` 在内的一些兼容 OpenAI 的 chat 音频模型要求 `input_audio.data` 使用 WAV data URL。Orally 会根据模型名称和兼容端点选择这种请求结构。

## 配置

在 Windows 上，Orally 可以从 `%APPDATA%\Orally\config.toml` 加载本地配置。设置 `ORALLY_CONFIG` 可使用自定义路径。

初始化提供方预设：

```powershell
cargo run -p orally-cli -- config init --provider aliyun-openai
```

支持的预设：

- `aliyun-openai`
- `openrouter`
- `openai`

显示当前生效的配置：

```powershell
cargo run -p orally-cli -- config show
```

更新一个配置值：

```powershell
cargo run -p orally-cli -- config set asr.model qwen3-asr-flash
cargo run -p orally-cli -- config set asr.api_key_env ORALLY_OPENAI_COMPAT_API_KEY
cargo run -p orally-cli -- config set postprocess.mode llm
cargo run -p orally-cli -- config set postprocess.model deepseek-v4-flash-0731
cargo run -p orally-cli -- config set postprocess.fallback_to_builtin true
cargo run -p orally-cli -- config set output.paste_delay_ms 300
cargo run -p orally-cli -- config set output.restore_clipboard true
cargo run -p orally-cli -- config set output.show_changes true
```

当前支持直接设置 `asr.api_key` 和 `postprocess.api_key`，但这些值会以明文保存。通过
`config set` 传入直接 key 还可能使其留在 shell history 中，`config show` 也会不经
脱敏地输出直接 key。请优先使用环境变量引用，并在复制或分享配置输出前进行脱敏。

常用配置键：

- `asr.base_url`
- `asr.model`
- `asr.protocol`
- `asr.api_key`
- `asr.api_key_env`
- `asr.language`
- `asr.prompt`
- `postprocess.mode`
- `postprocess.base_url`
- `postprocess.model`
- `postprocess.api_key`
- `postprocess.api_key_env`
- `postprocess.system_prompt`
- `postprocess.user_template`
- `postprocess.fallback_to_builtin`
- `output.locale`
- `output.raw`
- `output.show_changes`
- `output.insert`
- `output.paste_delay_ms`
- `output.restore_clipboard`
- `output.restore_clipboard_delay_ms`
- `audio.dictate_seconds`
- `audio.record_output`
- `audio.input_mode`
- `audio.auto_stop_enabled`
- `audio.min_record_ms`
- `audio.max_record_ms`
- `audio.silence_timeout_ms`
- `audio.silence_threshold`
- `hotkey.preset`
- `privacy.allow_external_requests`
- `privacy.history_enabled`
- `privacy.history_path`

出于隐私考虑，建议只保存 API key 对应的环境变量名称，并将 key 保留在 shell 或操作系统环境中。对于 OpenAI-compatible 预设，请设置：

```powershell
$env:ORALLY_OPENAI_COMPAT_API_KEY="..."
```

如果希望桌面应用在未预先设置环境变量的情况下运行，本地配置也可以保存 `asr.api_key`。该值会写入 `config.toml`，因此只应在你信任的机器上使用。

## 桌面设置应用

运行当前桌面设置外壳：

```powershell
cargo run -p orally-desktop
```

该应用使用 Tauri v2，并在设置窗口打开或隐藏时保持托盘图标运行。左键单击托盘图标，或选择 `Open Settings`，即可重新显示窗口。

桌面应用会在启动时注册 `hotkey.preset` 选择的 Windows 听写热键。新配置默认为
`Ctrl+Alt+Space`；无法读取配置或 preset 无效时也会回退到这个快捷键。按一次开始录音。
屏幕底部附近会出现一个置顶的小型提示框。再次按下相同热键，或单击 `停止`，即可停止
录音、转写、后处理，并将最终文本粘贴回录音开始时处于活动状态的窗口。

桌面应用目前仅使用 Toggle Mode。静音、松开按键、固定时长和最长时长都不会停止录音；只有再次按下热键或执行显式停止操作才会结束录音。旧有的自动停止字段仍保留在配置 schema 中以兼容现有配置，但桌面运行时会忽略这些字段。

托盘菜单也可以启动或停止听写、暂停或恢复热键、打开设置以及退出 Orally。

当前原型仍然提供固定的热键预设列表，并且只在启动时读取变更。直接捕获快捷键、即时
冲突验证以及无需重启即可完成注册，都是在已完成 shared-speech（共享语音）里程碑之后
仍处于 Deferred（已推迟）状态的产品改进。

设置界面可以编辑：

- API 提供方预设、base URL、模型、协议、API key、API key 环境变量、语言提示和 ASR prompt。
- AI 后处理模式、兼容 OpenAI 的 LLM 端点/模型/API key、system prompt 和 user template。
- 全局快捷键、粘贴延迟、输出 locale、Raw Transcript（原始转写文本）模式和剪贴板恢复。
- CLI 语音命令使用的共享 `output.insert` 与 `output.show_changes` 设置。Desktop 当前始终
  插入 Final Text（最终文本），且不显示处理变更列表。
- 外部请求隐私开关和本地历史记录。

保存操作会写入 CLI 使用的同一份本地配置；在 Windows 上通常位于 `%APPDATA%\Orally\config.toml`。将 API Key 字段留空即可仅使用环境变量引用。

## 桌面端 AI 后处理

Orally 默认使用内置的本地清理器。请设置：

```toml
[postprocess]
mode = "llm"
base_url = "https://ws-xzr3kkbjij82s72f.cn-beijing.maas.aliyuncs.com/compatible-mode/v1"
model = "deepseek-v4-flash-0731"
api_key_env = "ORALLY_OPENAI_COMPAT_API_KEY"
```

在桌面流程中，AI 后处理器会把 ASR 转写文本发送到兼容 OpenAI 的 `/chat/completions` 端点，并期望响应中只包含最终文本。使用 `postprocess.system_prompt` 和 `postprocess.user_template` 可自定义清理行为。user template 支持 `{{transcript}}` 和 `{{locale}}`。

设置 `output.raw = true` 可绕过内置后处理和 AI 后处理。

如果 `postprocess.fallback_to_builtin = true`，桌面端 AI 处理器返回错误时，会回退到 Orally 的本地清理器。缺少 AI 凭据或处理器构造错误目前会在进入该回退前失败。CLI 语音命令会忽略 `postprocess.mode`；`process --ai` 是一个独立的开发者命令，不使用此回退设置。

## JSONL 历史记录原型与剪贴板隐私

当前默认值是 `privacy.history_enabled = true`。因此，桌面流程会在成功插入后，把本地
JSONL 历史记录追加到当前配置文件旁的 `history.jsonl` 中。每条记录同时保存原始 ASR
文本、最终插入文本、毫秒时间戳和 `desktop` provider 标记。设置
`privacy.history_path` 可使用自定义文件；设置 `privacy.history_enabled = false` 可禁用
这项保留。CLI 语音命令不会写入此文件。

在桌面流程中，设置 `privacy.allow_external_requests = false` 可阻止远程 ASR 和 AI 后处理请求。CLI 网络命令目前不会执行此设置。

在 Windows 上，Orally 使用剪贴板回退插入路径。当 `output.restore_clipboard = true` 时，Orally 会在粘贴后恢复剪贴板中原有的文本，并从剪贴板移除生成文本。此初始实现会保留剪贴板中原有的文本内容；尚不能恢复非文本剪贴板格式。

## 可执行文件本地化的 Windows 原型

Orally 目前支持无需注册为 Windows 输入法的可执行文件本地配置。配置路径优先级依次为：
显式指定的 `ORALLY_CONFIG` 路径、`Orally.exe` 旁已存在的 `config.toml`，最后是
`%APPDATA%\Orally\config.toml`。

这是一个配置本地化原型，不是延期产品设计中定义的完整 Portable Installation 合约。

构建便携目录：

```powershell
.\scripts\package-portable.ps1
```

该脚本会写入：

```text
dist\portable\Orally\Orally.exe
dist\portable\Orally\config.toml
dist\portable\Orally\config.example.toml
```

从该目录运行 `Orally.exe`，即可让设置保留在便携目录中。也可以通过 CLI 在可执行文件目录创建相同配置：

```powershell
cargo run -p orally-cli -- config init --provider aliyun-openai --portable
```

打包脚本不会覆盖已有的 `config.toml`；它始终会写入最新的 `config.example.toml` 作为参考。

## 方向

架构基线中保持行为不变的共享语音里程碑已经在当前工作树中实现。CLI 和桌面应用保留当前各自的外围行为，同时通过 `orally-speech` 共享从音频到 Final Text（最终文本）的处理。

后续重构以后端优先，并由文档设门。英文文档保持 canonical，简体中文配对文档随之同步更新。后续的每项能力都必须在实现前，以独立的最小里程碑进入 Active 基线；更广泛的产品说明并不是一个要一次性全部实现的合并 backlog。

TSF、Android、macOS、Linux、本地 ASR、同步、插件以及完整的 Portable Installation 设计仍是延期的可能方向，而非当前里程碑。
