---
status: accepted
---

[English](0007-stream-long-recordings-through-audio-segments.md) |
[简体中文](0007-stream-long-recordings-through-audio-segments.zh-CN.md)

# 通过一分钟音频分段流式处理长录音

Orally 将长录音逐步写入临时 WAV Audio Segments，在一分钟边界轮转，而不是将完整录音保留在内存中。录音停止后，音频片段按顺序提交给 ASR；当提供方支持时，会将之前的转写文本上下文提供给下一次请求，并按顺序合并各段转写文本，然后 Workflow 才继续执行。

## 后果

临时音频片段必须在成功、显式丢弃、取消或应用程序退出后清理；启动时也会移除被遗弃的临时会话。启用 Audio Retention 时，这些音频片段会成为相关 Local History 的附件，而不是被删除。这样会限制内存和请求大小，但不会减少发送给 AI 后处理的合并转写文本长度。
