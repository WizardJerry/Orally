---
status: accepted
---

[English](0007-stream-long-recordings-through-audio-segments.md) |
[简体中文](0007-stream-long-recordings-through-audio-segments.zh-CN.md)

# 通过一分钟音频分段流式处理长录音

Orally 将长录音逐步写入临时 WAV Audio Segments，在一分钟边界轮转，而不是将完整录音保留在内存中。录音停止后，音频片段按顺序提交给 ASR；当提供方支持时，会将之前的转写文本上下文提供给下一次请求，并按顺序合并各段转写文本，然后 Workflow 才继续执行。

## 后果

临时音频片段必须在成功、显式丢弃、取消或应用程序退出后清理；启动时也会移除被遗弃的临时会话。启用 Audio Retention 时，这些音频片段会成为相关 Local History 的附件，而不是被删除。这样会限制内存和请求大小，但不会减少发送给 AI 后处理的合并转写文本长度。

## 上传音频准备（2026-10-15）

在按现有一分钟边界拆分 ASR 请求前，PCM16 音频先混合为单声道，再使用抗混叠滤波降采样到最高 16 kHz。更低的采样率保持不变；已经符合条件的单声道 PCM16 不做修改。转换会移除滤波延迟并排空尾部，保证整段录音的时长误差不超过一个输出采样点。所有 ASR 请求格式及自动协议重试复用准备好的 WAV 分段。

这项转换仅用于上传：Pending Voice Input 和 Audio Retention 保留原始录音。PCM16 解码器不支持的 WAV 格式继续沿用提供方直接处理原文件的兼容行为。浏览器录音测试使用 Web Audio 离线重采样，并保持原有采集限制和取消行为。这项改动减少请求体和上传时间，不引入流式请求，也不保证远端服务的延迟上限。
