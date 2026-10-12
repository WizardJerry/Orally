---
status: accepted
---

[English](0007-stream-long-recordings-through-audio-segments.md) |
[简体中文](0007-stream-long-recordings-through-audio-segments.zh-CN.md)

# Stream long recordings through one-minute audio segments

Orally writes long recordings progressively into temporary WAV Audio Segments, rotating at one-minute boundaries instead of retaining the complete recording in memory. After recording stops, segments are submitted to ASR sequentially, with prior transcript context supplied to the next request when the provider supports it, and their transcripts are combined in order before the Workflow continues.

## Consequences

Temporary segments must be cleaned after success, explicit discard, cancellation, or application exit; startup also removes abandoned temporary sessions. When Audio Retention is enabled, the segments become the related Local History attachment instead of being deleted. This limits memory and request size but does not reduce the combined transcript length sent to AI post-processing.
