# Orally

[English](README.md) | [简体中文](README.zh-CN.md)

Orally 是一个可实际工作的 AI 辅助语音输入早期原型，也是一个面向学习的 Rust 重构项目。

当前可用的功能链路可以录制或接收音频，将其发送至兼容 OpenAI 的语音识别提供方，在本地或通过 AI Post-processing（AI 后处理）优化转写文本，并生成可直接粘贴的文本。Windows Desktop App（Windows 桌面应用）还提供 Toggle Mode、托盘控制、浮层提示和剪贴板插入功能。

## 开发方式

当前进行中的重构通过 `orally-speech` 共享现有的 ASR 与后处理链路。CLI 语音命令和 Windows Desktop App（Windows 桌面应用）都使用这一接缝，同时保留各自不同的外围策略。其目的是以小步、保持行为不变的方式改进职责归属，而不是实现产品说明中的所有能力。

开始实现工作前，请阅读[重构架构基线](docs/engineering/refactor-baseline.zh-CN.md)；有关各文档的职责，请参阅[文档导航](docs/README.zh-CN.md)。

## 本地开发（Windows）

安装 Rust、Node.js/npm、Windows Tauri 构建所需工具和 `just` 后，在仓库根目录运行：

```powershell
just dev
```

此命令在缺少 Tauri 或 Vite 时通过 `npm ci` 安装前端开发依赖，然后通过 `tauri dev` 启动 Desktop App。Tauri 会在 `http://localhost:1420` 启动 Vite，前端修改支持热更新，Rust 修改会触发应用重新编译。后续启动复用已安装的依赖；修改 `package.json` 或 `package-lock.json` 后，运行 `just setup` 刷新依赖。首次 Rust 编译可能较慢。在终端按 `Ctrl+C` 可停止开发进程。

运行 `just` 或 `just --list` 查看命令：

| 命令 | 用途 |
| --- | --- |
| `just dev` | 按需准备前端依赖，启动桌面开发模式 |
| `just dev-ui` | 按需准备前端依赖，仅启动 Vite |
| `just setup` | 安装或刷新锁定的前端开发依赖 |
| `just build` | 构建内嵌界面的 Release 桌面应用，生成 `dist/portable/Orally` |
| `just build-debug` | 构建全部 Rust workspace 包的 Debug 版本 |
| `just test` | 运行 Rust workspace 测试 |
| `just check` | 检查 Rust 格式和编译 |
| `just fmt` | 格式化 Rust workspace |

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
cargo run -p orally-cli -- config init --provider openai-compatible
cargo run -p orally-cli -- config set output.paste_delay_ms 300
cargo run -p orally-cli -- config show
cargo run -p orally-desktop
```

`record` 命令从默认麦克风采集音频，并写入 PCM16 WAV 文件。`transcribe` 将已有 WAV 文件发送至已配置的 ASR 提供方。`dictate` 则在一个步骤中完成麦克风录音并将采集到的音频发送至 ASR。

默认情况下，远程 ASR 输出会经过 Orally 内置的后处理流水线。使用 `--raw` 可直接输出提供方返回的转写文本而不做后处理，使用 `--show-changes` 可显示本地清理操作。

在 Windows 上，使用 `--insert` 可将最终文本粘贴到当前获得焦点的输入框。CLI 会先等待 `--paste-delay-ms` 指定的时长再粘贴，以便你在录音或转写完成后将焦点切换到目标应用。

在 Windows 上使用 `listen` 可让 Orally 持续运行，并通过 `Ctrl+Alt+Space` 触发听写。按一次开始录音，再按一次相同热键即可停止录音、转写、后处理并粘贴。在终端中按 `Ctrl+C` 可停止监听器。

对于 ASR，请配置兼容 OpenAI 的语音服务端点：

```powershell
$env:ORALLY_ASR_API_KEY="..."
$env:ORALLY_ASR_BASE_URL="https://api.openai.com/v1"
$env:ORALLY_ASR_MODEL="whisper-1"
cargo run -p orally-cli -- transcribe --file orally-recording.wav
```

ASR 默认使用 `auto`：先尝试 multipart `/audio/transcriptions`，收到 HTTP 404 或 405
后尝试 `/chat/completions` 的 Chat Audio。音频以 WAV Data URL 传入，已有语言提示
作为可选系统上下文；收到 HTTP 400 或 422 时再尝试原始 Base64、`format: "wav"`
及固定转写指令。
原生 multipart 上传先在内存中准备完整请求体，确保服务提前拒绝上传时仍能根据 HTTP
状态切换协议。错误响应正文读取失败时，也保留已经收到的 HTTP 状态。
鉴权失败、限流、服务器错误、网络异常和成功响应解析失败均不触发协议重试。成功选择
的格式会在后续音频分块中复用，不根据域名或模型名猜测。CLI 可显式选择
`--protocol openai-transcriptions` 或 `--protocol chat-audio`。LLM 后处理使用文本
`/chat/completions`；两者可以共用地址和 API Key，但 LLM 成功不能验证语音模型。

## 配置

Orally 统一读写可执行文件旁的 `config.toml`。若该文件不存在，会依次检查旧
`ORALLY_CONFIG`、`%APPDATA%\Orally\config.toml` 和 `$HOME/.config/orally/config.toml`
并复制已有配置。迁移保留原文件，不覆盖已有同目录配置；旧文件无法读取或解析时明确
报错，不静默重置设置。

初始化提供方预设：

```powershell
cargo run -p orally-cli -- config init --provider openai-compatible
```

支持的预设：

- `openai-compatible`（默认，使用 `ORALLY_OPENAI_COMPAT_API_KEY`）
- `openai`

显示当前生效的配置：

```powershell
cargo run -p orally-cli -- config show
```

更新一个配置值：

```powershell
cargo run -p orally-cli -- config set asr.model whisper-1
cargo run -p orally-cli -- config set asr.api_key_env ORALLY_OPENAI_COMPAT_API_KEY
cargo run -p orally-cli -- config set postprocess.mode llm
cargo run -p orally-cli -- config set postprocess.model gpt-4o-mini
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
屏幕底部附近会出现一个黑灰色、半透明的圆角置顶录音框。底部条形随麦克风音量波动，
静音时恢复为一排短点。再次按下相同热键，或单击停止图标，即可停止录音、转写、
后处理，并将最终文本粘贴回录音开始时处于活动状态的窗口。

桌面应用目前仅使用 Toggle Mode。静音、松开按键、固定时长和最长时长都不会停止录音；只有再次按下热键或执行显式停止操作才会结束录音。旧有的自动停止字段仍保留在配置 schema 中以兼容现有配置，但桌面运行时会忽略这些字段。

托盘菜单也可以启动或停止听写、暂停或恢复热键、打开设置以及退出 Orally。“配置”子菜单
列出所有已保存配置，当前配置前有圆点。点击即可立即启用，并同步设置窗口的选择，同时
保留未保存草稿。保存新配置或名称后，菜单会自动刷新。

在设置页点击听写快捷键后，直接按下需要的组合，按 Esc 可取消。保存时立即注册新组合，
注册冲突或保存失败会保留旧绑定。录入期间临时释放旧热键，结束录入后恢复。快捷键设置
只保留开始/停止听写；设置窗口内的 `Ctrl+S` 继续作为固定保存操作。

设置界面可以编辑：

- OpenAI 兼容语音服务的地址、模型和 API Key。
- AI 后处理模式及有序的 OpenAI 兼容模型和 Prompt 节点。
- 全局快捷键、粘贴延迟、Raw Transcript（原始转写文本）模式和剪贴板恢复。
- CLI 语音命令使用的共享 `output.insert` 与 `output.show_changes` 设置。Desktop 当前始终
  插入 Final Text（最终文本），且不显示处理变更列表。
- 外部请求隐私开关和本地历史记录。

保存操作统一写入可执行文件旁的 `config.toml`，桌面端与 CLI 放在同一目录时共用此文件。
设置页不再展示输出语言与配置路径控件，已有 locale 值仍保留。将 API Key 字段留空即可
仅使用环境变量引用。

## 桌面端 AI 后处理

模型配置页合并了配置选择和管线编辑。顶部可以切换、新建、导入、导出或保存配置，下方
可以修改当前配置名称。切换会保留未保存草稿；保存会写入配置列表，并启用当前选择的
配置。语音识别在上，后处理在下。语音识别填写服务地址、模型和 API Key；后处理还提供
Prompt。桌面 ASR、连通性测试及录音测试共用转写与 Chat Audio 的自动协商。
旧配置规范为 `auto`，保留原有服务字段；界面不展示识别协议选项。
语音识别不再提供可配置的 Prompt。旧配置中的 ASR Prompt 在载入时忽略，再次保存时
移除。后续计划通过词典功能提供识别词汇，当前尚未实现。
API Key 默认用圆点遮挡，可通过眼睛
按钮切换显示，仍以明文保存。
输出、隐私和快捷键仍是设置页中的全局选项。

每个后处理模型拥有独立的服务地址、凭据、Prompt 及有序 Prompt 子节点。模型和 Prompt
支持添加、移除、启用与排序。AI 模式下，启用的模型依次运行，每一步接收前一步输出。
Prompt 子节点追加到所属模型的系统指令中。本地模式忽略 AI 节点；全部 AI 模型禁用时
直接保留原始转写。现有单模型 TOML 仍可载入，隐藏的语言、环境变量、模板和回退设置
保留原有值。

语音和每个 LLM 服务都提供连通性测试，使用当前填写的字段。顶部“测试”默认直接模拟
一次语音输入：开始录音，说话，再停止录音，执行语音识别和已配置的完整后处理链。
开始录音时读取当前配置和全局设置，包括所有未保存的修改。语音卡片中的“录音测试”
只运行 ASR；弹窗也保留仅运行后处理的文本测试，无需上传音频。结果展示识别文本和
最终输出，不保存配置、插入文本或写入历史。关闭或取消会释放麦克风；每次测试录音
最多两分钟、15 MiB。
连通性测试直接报告服务失败；输入输出测试使用已配置的本地回退时会显示警告。禁止
外部请求的隐私设置也适用于测试。浏览器预览需授权麦克风并直接请求服务，需要服务
允许 CORS；原生应用复用现有麦克风录音能力，不受浏览器 CORS 限制。

导出会保存一套 JSON 配置，包括直接填写的 API Key。导入支持 JSON 配置或完整配置文档，
Desktop App 也支持现有 TOML。导入内容先成为新草稿，保存后生效。浏览器预览单独使用
本地存储，TOML 解析需要原生应用。CLI 保留单模型流程。参阅
[编辑器存储决策](docs/adr/0008-edit-self-contained-model-profiles.zh-CN.md)。

Orally 默认使用内置的本地清理器。请设置：

```toml
[postprocess]
mode = "llm"
base_url = "https://api.openai.com/v1"
model = "gpt-4o-mini"
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

Orally 无需注册为 Windows 输入法，`config.toml` 始终位于可执行文件旁，包括首次保存。
旧路径仅作为迁移来源；程序目录不可写时会明确报错，不改用其他目录保存。

这是一个配置本地化原型，不是延期产品设计中定义的完整 Portable Installation 合约。

构建便携目录：

```powershell
just build
```

该命令编译内嵌界面的优化版 Release 可执行文件，运行时无需 Vite 开发服务器。
编译失败时会立即停止打包。

该脚本会写入：

```text
dist\portable\Orally\Orally.exe
dist\portable\Orally\config.toml
dist\portable\Orally\config.example.toml
```

从该目录运行 `Orally.exe`，即可让设置保留在便携目录中。也可以通过 CLI 在可执行文件目录创建相同配置：

```powershell
cargo run -p orally-cli -- config init --provider openai-compatible --portable
```

打包脚本不会覆盖已有的 `config.toml`；它始终会写入最新的 `config.example.toml` 作为参考。

## 方向

架构基线中保持行为不变的共享语音里程碑已经在当前工作树中实现。CLI 和桌面应用保留当前各自的外围行为，同时通过 `orally-speech` 共享从音频到 Final Text（最终文本）的处理。

后续重构以后端优先，并由文档设门。英文文档保持 canonical，简体中文配对文档随之同步更新。后续的每项能力都必须在实现前，以独立的最小里程碑进入 Active 基线；更广泛的产品说明并不是一个要一次性全部实现的合并 backlog。

TSF、Android、macOS、Linux、本地 ASR、同步、插件以及完整的 Portable Installation 设计仍是延期的可能方向，而非当前里程碑。
