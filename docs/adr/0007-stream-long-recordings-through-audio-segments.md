---
status: accepted
---

[English](0007-stream-long-recordings-through-audio-segments.md) |
[简体中文](0007-stream-long-recordings-through-audio-segments.zh-CN.md)

# Stream long recordings through one-minute audio segments

Orally writes long recordings progressively into temporary WAV Audio Segments, rotating at one-minute boundaries instead of retaining the complete recording in memory. After recording stops, segments are submitted to ASR sequentially, with prior transcript context supplied to the next request when the provider supports it, and their transcripts are combined in order before the Workflow continues.

## Consequences

Temporary segments must be cleaned after success, explicit discard, cancellation, or application exit; startup also removes abandoned temporary sessions. When Audio Retention is enabled, the segments become the related Local History attachment instead of being deleted. This limits memory and request size but does not reduce the combined transcript length sent to AI post-processing.

## Upload audio preparation (2026-10-15)

Before the existing one-minute ASR request boundaries are applied, PCM16 audio is mixed to mono and downsampled to at most 16 kHz with an anti-aliasing filter. Lower sample rates are preserved; already suitable mono PCM16 is unchanged. Filter delay is removed and the tail is flushed so preparation preserves the full recording duration to within one output sample. All ASR request formats and automatic protocol retries reuse the prepared WAV chunks.

This is an upload transformation: Pending Voice Input and Audio Retention keep the original recording. WAV formats that the PCM16 decoder does not support retain the existing provider passthrough behavior. The browser recording test uses Web Audio's offline resampling, preserving its capture limits and cancellation behavior. This change reduces request bytes and upload time; it does not introduce streaming requests or bound remote provider latency.
