# Windows IME 开发

[English](windows-ime.md) | [简体中文](windows-ime.zh-CN.md)

> Status: Deferred
>
> 文档类型：研究说明
>
> Windows TSF 工作不属于当前以后端优先的重构。在启用这项工作前，请重新阅读[重构架构基线](refactor-baseline.zh-CN.md)。

Orally 当前的 Windows 原型使用前台进程、全局热键、剪贴板和模拟的 `Ctrl+V`。真正的 Windows 输入法应当是一个 Text Services Framework (TSF) 文本服务。

## 目标结构

```text
TSF 文本服务 DLL
  -> 接收激活与组合回调
  -> 与 Orally 后台服务通信
  -> 将最终文本提交到获得焦点的 TSF 上下文

Orally 后台服务
  -> 录制音频
  -> 调用 ASR
  -> 运行后处理
  -> 存储设置/历史记录
```

TSF DLL 应保持精简。音频采集、网络提供方、提示词、历史记录和设置都应位于 DLL 之外，这样可以让输入法集成更易调试，并降低其导致宿主应用不稳定的可能性。

## 无需重新安装的调试方式

开发期间不应反复运行完整安装程序。

推荐的循环：

1. 构建 TSF DLL。
2. 为当前用户注册调试版本。
3. 如有需要，重启文本输入宿主或注销后重新登录。
4. 从语言/输入切换器中选择 Orally 输入法。
5. 将 Visual Studio、WinDbg 或其他调试器附加到宿主进程。
6. 当 COM/TSF 导出项发生变化时，重新构建、取消注册并注册新的调试 DLL。

最终安装程序应执行机器级或用户级注册，但开发循环应当可以通过脚本完成。

## 注册模型

TSF 输入法包含两层注册：

- DLL 的 COM 进程内服务器注册。
- 通过输入处理器配置文件 API 完成的 TSF 文本服务/配置文件注册。

Microsoft 的 TSF 注册文档将其描述为标准 COM 注册，以及通过 `ITfInputProcessorProfiles::Register` 完成的 TSF 注册。

开发期间，应尽可能优先使用 `HKCU` 下的当前用户注册。这样可将注册限制在当前用户，并避免每次编辑都需要管理员权限。基于注册表的注册不属于 Portable Installation 数据。

## 当前仓库状态

- `crates/orally-windows`：前台热键和剪贴板粘贴原型。
- `apps/windows-ime`：占位 TSF DLL crate，导出标准 COM DLL 入口点。它目前还不是可工作的输入法。

## 未来评审的研究检查清单

1. 为 TSF 文本服务实现 COM 类工厂。
2. 实现 `ITfTextInputProcessorEx::ActivateEx` 和停用路径。
3. 添加按用户注册的脚本。
4. 添加最小语言配置文件，使 Windows 可以将 Orally 列为输入法。
5. 将固定文本提交到当前 TSF 上下文。
6. 将 TSF 触发事件连接到现有 Orally 后台流水线。

## 常用命令

构建占位 DLL：

```powershell
cargo build -p orally-windows-ime
```

注册导出函数目前仍为存根，因此在实现注册 TODO 前不要使用 `regsvr32`。
