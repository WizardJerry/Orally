use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use orally_core::{AudioFormat, AudioInput, OrallyError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordingConfig {
    pub duration: Duration,
}

impl RecordingConfig {
    pub fn for_seconds(seconds: u64) -> Self {
        Self {
            duration: Duration::from_secs(seconds),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordedAudio {
    pub audio: AudioInput,
    pub metrics: AudioMetrics,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioMetrics {
    pub duration_ms: u64,
    pub peak_amplitude: f32,
    pub rms_amplitude: f32,
    pub voice_activity_ratio: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceActivityConfig {
    pub sample_threshold: f32,
}

impl Default for VoiceActivityConfig {
    fn default() -> Self {
        Self {
            sample_threshold: 0.02,
        }
    }
}

#[derive(Debug, Default)]
pub struct CpalAudioRecorder;

impl CpalAudioRecorder {
    pub fn record_for(&self, config: RecordingConfig) -> Result<RecordedAudio, OrallyError> {
        if config.duration.is_zero() {
            return Err(OrallyError::InvalidInput(
                "recording duration must be greater than zero".to_string(),
            ));
        }

        let session = CpalRecordingSession::start()?;
        std::thread::sleep(config.duration);
        session.stop()
    }
}

pub struct CpalRecordingSession {
    stream: cpal::Stream,
    samples: Arc<Mutex<Vec<i16>>>,
    sample_rate_hz: u32,
    channels: u16,
    started_at: Instant,
}

impl CpalRecordingSession {
    pub fn start() -> Result<Self, OrallyError> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| OrallyError::Audio("no default input device found".to_string()))?;
        let supported_config = device
            .default_input_config()
            .map_err(|error| OrallyError::Audio(error.to_string()))?;
        let sample_format = supported_config.sample_format();
        let stream_config: StreamConfig = supported_config.into();
        let sample_rate_hz = stream_config.sample_rate;
        let channels = stream_config.channels;
        let samples = Arc::new(Mutex::new(Vec::<i16>::new()));

        let stream = build_stream(&device, &stream_config, sample_format, Arc::clone(&samples))?;
        stream
            .play()
            .map_err(|error| OrallyError::Audio(error.to_string()))?;

        Ok(Self {
            stream,
            samples,
            sample_rate_hz,
            channels,
            started_at: Instant::now(),
        })
    }

    pub fn stop(self) -> Result<RecordedAudio, OrallyError> {
        let elapsed = self.started_at.elapsed();
        drop(self.stream);

        let captured_samples = self
            .samples
            .lock()
            .map_err(|error| OrallyError::Audio(error.to_string()))?
            .clone();
        let bytes = pcm16_samples_to_bytes(&captured_samples);
        let audio = AudioInput {
            bytes,
            sample_rate_hz: self.sample_rate_hz,
            channels: self.channels,
            format: AudioFormat::Pcm16,
        };
        let mut metrics = analyze_pcm16(&audio, VoiceActivityConfig::default())?;
        metrics.duration_ms = elapsed.as_millis().min(u128::from(u64::MAX)) as u64;

        Ok(RecordedAudio { audio, metrics })
    }

    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Returns the current PCM16 capture size without copying recorded samples.
    pub fn captured_byte_len(&self) -> Result<usize, OrallyError> {
        self.samples
            .lock()
            .map(|samples| samples.len().saturating_mul(std::mem::size_of::<i16>()))
            .map_err(|error| OrallyError::Audio(error.to_string()))
    }

    pub fn recent_metrics(
        &self,
        window: Duration,
        vad_config: VoiceActivityConfig,
    ) -> Result<AudioMetrics, OrallyError> {
        if window.is_zero() {
            return Err(OrallyError::InvalidInput(
                "metrics window must be greater than zero".to_string(),
            ));
        }

        let samples = self
            .samples
            .lock()
            .map_err(|error| OrallyError::Audio(error.to_string()))?;
        let channel_count = usize::from(self.channels.max(1));
        let max_frames = frames_for_duration(self.sample_rate_hz, window)?;
        let max_samples = max_frames
            .checked_mul(channel_count)
            .ok_or_else(|| OrallyError::InvalidInput("metrics window is too large".to_string()))?;
        let start = samples.len().saturating_sub(max_samples);
        let bytes = pcm16_samples_to_bytes(&samples[start..]);
        let audio = AudioInput {
            bytes,
            sample_rate_hz: self.sample_rate_hz,
            channels: self.channels,
            format: AudioFormat::Pcm16,
        };

        analyze_pcm16(&audio, vad_config)
    }
}

pub fn analyze_pcm16(
    audio: &AudioInput,
    vad_config: VoiceActivityConfig,
) -> Result<AudioMetrics, OrallyError> {
    if audio.format != AudioFormat::Pcm16 {
        return Err(OrallyError::InvalidInput(
            "audio metrics currently require PCM16 input".to_string(),
        ));
    }

    let samples = pcm16_bytes_to_samples(&audio.bytes)?;
    let sample_count = samples.len();
    let frame_count = sample_count / usize::from(audio.channels.max(1));
    let duration_ms = if audio.sample_rate_hz == 0 {
        0
    } else {
        ((frame_count as u64) * 1_000) / u64::from(audio.sample_rate_hz)
    };

    if samples.is_empty() {
        return Ok(AudioMetrics {
            duration_ms,
            peak_amplitude: 0.0,
            rms_amplitude: 0.0,
            voice_activity_ratio: 0.0,
        });
    }

    let mut peak = 0.0_f32;
    let mut square_sum = 0.0_f64;
    let mut active_samples = 0_usize;

    for sample in samples {
        let normalized = f32::from(sample).abs() / f32::from(i16::MAX);
        peak = peak.max(normalized);
        square_sum += f64::from(normalized * normalized);
        if normalized >= vad_config.sample_threshold {
            active_samples += 1;
        }
    }

    Ok(AudioMetrics {
        duration_ms,
        peak_amplitude: peak,
        rms_amplitude: (square_sum / sample_count as f64).sqrt() as f32,
        voice_activity_ratio: active_samples as f32 / sample_count as f32,
    })
}

pub fn encode_wav_pcm16(audio: &AudioInput) -> Result<Vec<u8>, OrallyError> {
    if audio.format != AudioFormat::Pcm16 {
        return Err(OrallyError::InvalidInput(
            "WAV encoding currently requires PCM16 input".to_string(),
        ));
    }

    let data_len = u32::try_from(audio.bytes.len()).map_err(|_| {
        OrallyError::InvalidInput("audio is too large to encode as WAV".to_string())
    })?;
    let riff_len = 36_u32
        .checked_add(data_len)
        .ok_or_else(|| OrallyError::InvalidInput("WAV file would be too large".to_string()))?;
    let byte_rate = audio
        .sample_rate_hz
        .checked_mul(u32::from(audio.channels))
        .and_then(|value| value.checked_mul(2))
        .ok_or_else(|| OrallyError::InvalidInput("invalid WAV byte rate".to_string()))?;
    let block_align = audio
        .channels
        .checked_mul(2)
        .ok_or_else(|| OrallyError::InvalidInput("invalid WAV block align".to_string()))?;

    let mut wav = Vec::with_capacity(44 + audio.bytes.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&riff_len.to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&audio.channels.to_le_bytes());
    wav.extend_from_slice(&audio.sample_rate_hz.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(&audio.bytes);

    Ok(wav)
}

pub fn decode_wav_pcm16(bytes: &[u8]) -> Result<AudioInput, OrallyError> {
    if bytes.len() < 44 {
        return Err(OrallyError::InvalidInput(
            "WAV input is too short".to_string(),
        ));
    }

    if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(OrallyError::InvalidInput(
            "WAV input must use RIFF/WAVE format".to_string(),
        ));
    }

    let mut cursor = 12_usize;
    let mut sample_rate_hz = None;
    let mut channels = None;
    let mut bits_per_sample = None;
    let mut audio_format = None;
    let mut data = None;

    while cursor.checked_add(8).is_some_and(|end| end <= bytes.len()) {
        let chunk_id = &bytes[cursor..cursor + 4];
        let chunk_len = u32::from_le_bytes([
            bytes[cursor + 4],
            bytes[cursor + 5],
            bytes[cursor + 6],
            bytes[cursor + 7],
        ]) as usize;
        cursor += 8;

        let chunk_end = cursor
            .checked_add(chunk_len)
            .ok_or_else(|| OrallyError::InvalidInput("WAV chunk is too large".to_string()))?;
        if chunk_end > bytes.len() {
            return Err(OrallyError::InvalidInput(
                "WAV chunk extends past end of input".to_string(),
            ));
        }

        match chunk_id {
            b"fmt " => {
                if chunk_len < 16 {
                    return Err(OrallyError::InvalidInput(
                        "WAV fmt chunk is too short".to_string(),
                    ));
                }
                audio_format = Some(u16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]));
                channels = Some(u16::from_le_bytes([bytes[cursor + 2], bytes[cursor + 3]]));
                sample_rate_hz = Some(u32::from_le_bytes([
                    bytes[cursor + 4],
                    bytes[cursor + 5],
                    bytes[cursor + 6],
                    bytes[cursor + 7],
                ]));
                bits_per_sample =
                    Some(u16::from_le_bytes([bytes[cursor + 14], bytes[cursor + 15]]));
            }
            b"data" => {
                data = Some(bytes[cursor..chunk_end].to_vec());
            }
            _ => {}
        }

        cursor = chunk_end + (chunk_len % 2);
    }

    if audio_format != Some(1) || bits_per_sample != Some(16) {
        return Err(OrallyError::InvalidInput(
            "only PCM16 WAV input is supported".to_string(),
        ));
    }

    let sample_rate_hz = sample_rate_hz
        .ok_or_else(|| OrallyError::InvalidInput("WAV input is missing sample rate".to_string()))?;
    let channels = channels
        .ok_or_else(|| OrallyError::InvalidInput("WAV input is missing channels".to_string()))?;
    let data = data
        .ok_or_else(|| OrallyError::InvalidInput("WAV input is missing data chunk".to_string()))?;

    Ok(AudioInput {
        bytes: data,
        sample_rate_hz,
        channels,
        format: AudioFormat::Pcm16,
    })
}

pub fn split_pcm16_audio(
    audio: &AudioInput,
    max_duration: Duration,
) -> Result<Vec<AudioInput>, OrallyError> {
    if audio.format != AudioFormat::Pcm16 {
        return Err(OrallyError::InvalidInput(
            "audio splitting currently requires PCM16 input".to_string(),
        ));
    }
    if max_duration.is_zero() {
        return Err(OrallyError::InvalidInput(
            "audio chunk duration must be greater than zero".to_string(),
        ));
    }
    if audio.sample_rate_hz == 0 {
        return Err(OrallyError::InvalidInput(
            "PCM16 audio sample rate must be greater than zero".to_string(),
        ));
    }
    if audio.channels == 0 {
        return Err(OrallyError::InvalidInput(
            "PCM16 audio channel count must be greater than zero".to_string(),
        ));
    }

    let frame_size = usize::from(audio.channels)
        .checked_mul(2)
        .ok_or_else(|| OrallyError::InvalidInput("invalid PCM16 frame size".to_string()))?;
    if audio.bytes.len() % frame_size != 0 {
        return Err(OrallyError::InvalidInput(
            "PCM16 byte length must align to complete frames".to_string(),
        ));
    }

    let max_frames = frames_for_duration(audio.sample_rate_hz, max_duration)?;
    let max_bytes = max_frames
        .checked_mul(frame_size)
        .ok_or_else(|| OrallyError::InvalidInput("audio chunk would be too large".to_string()))?;
    if max_bytes == 0 {
        return Err(OrallyError::InvalidInput(
            "audio chunk duration is too short for the sample rate".to_string(),
        ));
    }

    Ok(audio
        .bytes
        .chunks(max_bytes)
        .map(|chunk| AudioInput {
            bytes: chunk.to_vec(),
            sample_rate_hz: audio.sample_rate_hz,
            channels: audio.channels,
            format: AudioFormat::Pcm16,
        })
        .collect())
}

fn frames_for_duration(sample_rate_hz: u32, duration: Duration) -> Result<usize, OrallyError> {
    let frames = duration
        .as_nanos()
        .checked_mul(u128::from(sample_rate_hz))
        .ok_or_else(|| {
            OrallyError::InvalidInput("audio chunk duration is too large".to_string())
        })?
        / 1_000_000_000_u128;

    usize::try_from(frames)
        .map_err(|_| OrallyError::InvalidInput("audio chunk duration is too large".to_string()))
}

fn build_stream(
    device: &cpal::Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    samples: Arc<Mutex<Vec<i16>>>,
) -> Result<cpal::Stream, OrallyError> {
    let err_fn = |error| eprintln!("audio stream error: {error}");

    match sample_format {
        SampleFormat::I16 => device
            .build_input_stream(
                config,
                move |data: &[i16], _| push_i16_samples(data, &samples),
                err_fn,
                None,
            )
            .map_err(|error| OrallyError::Audio(error.to_string())),
        SampleFormat::U16 => device
            .build_input_stream(
                config,
                move |data: &[u16], _| push_u16_samples(data, &samples),
                err_fn,
                None,
            )
            .map_err(|error| OrallyError::Audio(error.to_string())),
        SampleFormat::F32 => device
            .build_input_stream(
                config,
                move |data: &[f32], _| push_f32_samples(data, &samples),
                err_fn,
                None,
            )
            .map_err(|error| OrallyError::Audio(error.to_string())),
        other => Err(OrallyError::Audio(format!(
            "unsupported input sample format: {other:?}"
        ))),
    }
}

fn push_i16_samples(data: &[i16], samples: &Arc<Mutex<Vec<i16>>>) {
    if let Ok(mut guard) = samples.lock() {
        guard.extend_from_slice(data);
    }
}

fn push_u16_samples(data: &[u16], samples: &Arc<Mutex<Vec<i16>>>) {
    if let Ok(mut guard) = samples.lock() {
        guard.extend(data.iter().map(|sample| {
            let centered = i32::from(*sample) - 32_768;
            centered.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
        }));
    }
}

fn push_f32_samples(data: &[f32], samples: &Arc<Mutex<Vec<i16>>>) {
    if let Ok(mut guard) = samples.lock() {
        guard.extend(data.iter().map(|sample| {
            let scaled = sample.clamp(-1.0, 1.0) * f32::from(i16::MAX);
            scaled.round() as i16
        }));
    }
}

fn pcm16_samples_to_bytes(samples: &[i16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn pcm16_bytes_to_samples(bytes: &[u8]) -> Result<Vec<i16>, OrallyError> {
    if bytes.len() % 2 != 0 {
        return Err(OrallyError::InvalidInput(
            "PCM16 byte length must be even".to_string(),
        ));
    }

    Ok(bytes
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_valid_wav_header() {
        let audio = AudioInput {
            bytes: vec![0, 0, 255, 127],
            sample_rate_hz: 16_000,
            channels: 1,
            format: AudioFormat::Pcm16,
        };

        let wav = encode_wav_pcm16(&audio).expect("WAV encoding should work");

        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(wav.len(), 48);
    }

    #[test]
    fn calculates_audio_metrics() {
        let audio = AudioInput {
            bytes: pcm16_samples_to_bytes(&[0, i16::MAX, 0, i16::MAX / 2]),
            sample_rate_hz: 4,
            channels: 1,
            format: AudioFormat::Pcm16,
        };

        let metrics = analyze_pcm16(
            &audio,
            VoiceActivityConfig {
                sample_threshold: 0.4,
            },
        )
        .expect("metrics should be calculated");

        assert_eq!(metrics.duration_ms, 1_000);
        assert_eq!(metrics.peak_amplitude, 1.0);
        assert_eq!(metrics.voice_activity_ratio, 0.5);
        assert!(metrics.rms_amplitude > 0.5);
    }

    #[test]
    fn decodes_pcm16_wav() {
        let audio = AudioInput {
            bytes: pcm16_samples_to_bytes(&[0, i16::MAX]),
            sample_rate_hz: 16_000,
            channels: 1,
            format: AudioFormat::Pcm16,
        };
        let wav = encode_wav_pcm16(&audio).expect("WAV encoding should work");

        let decoded = decode_wav_pcm16(&wav).expect("WAV decoding should work");

        assert_eq!(decoded, audio);
    }

    #[test]
    fn splits_pcm16_audio_on_frame_boundaries() {
        let audio = AudioInput {
            bytes: pcm16_samples_to_bytes(&[0, 1, 2, 3, 4, 5]),
            sample_rate_hz: 2,
            channels: 1,
            format: AudioFormat::Pcm16,
        };

        let chunks = split_pcm16_audio(&audio, Duration::from_secs(1)).expect("audio should split");

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].bytes, pcm16_samples_to_bytes(&[0, 1]));
        assert_eq!(chunks[1].bytes, pcm16_samples_to_bytes(&[2, 3]));
        assert_eq!(chunks[2].bytes, pcm16_samples_to_bytes(&[4, 5]));
    }

    #[test]
    fn analyzes_silence_as_inactive_voice() {
        let audio = AudioInput {
            bytes: pcm16_samples_to_bytes(&[0, 0, 0, 0]),
            sample_rate_hz: 4,
            channels: 1,
            format: AudioFormat::Pcm16,
        };

        let metrics = analyze_pcm16(
            &audio,
            VoiceActivityConfig {
                sample_threshold: 0.02,
            },
        )
        .expect("metrics should calculate");

        assert_eq!(metrics.voice_activity_ratio, 0.0);
        assert_eq!(metrics.rms_amplitude, 0.0);
    }
}
