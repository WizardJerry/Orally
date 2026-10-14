use orally_core::{AudioFormat, AudioInput, OrallyError};
use rubato::{FftFixedInOut, Resampler};

const TRANSCRIPTION_SAMPLE_RATE_HZ: u32 = 16_000;
const RESAMPLING_CHUNK_FRAMES: usize = 1_024;
// FFT ratios with very large coprime rates otherwise allocate enormous filters
// from a WAV header alone. This still admits arbitrary ordinary capture rates.
const MAX_FFT_INPUT_FRAMES: u32 = 1_048_576;

/// Prepare an independent PCM16 copy for speech recognition without altering capture data.
/// Channels are averaged to mono; rates above 16 kHz are downsampled with anti-aliasing.
pub fn prepare_transcription_audio(audio: &AudioInput) -> Result<AudioInput, OrallyError> {
    if audio.format != AudioFormat::Pcm16 {
        return Err(OrallyError::InvalidInput(
            "transcription audio preparation requires PCM16 input".to_string(),
        ));
    }
    if audio.sample_rate_hz == 0 || audio.channels == 0 {
        return Err(OrallyError::InvalidInput(
            "PCM16 sample rate and channel count must be greater than zero".to_string(),
        ));
    }
    let frame_bytes = usize::from(audio.channels) * 2;
    if audio.bytes.len() % frame_bytes != 0 {
        return Err(OrallyError::InvalidInput(
            "PCM16 byte length must align to complete frames".to_string(),
        ));
    }
    let output_rate = audio.sample_rate_hz.min(TRANSCRIPTION_SAMPLE_RATE_HZ);
    if audio.sample_rate_hz == output_rate && audio.channels == 1 {
        return Ok(audio.clone());
    }
    // Mix in floating point to avoid overflow for multiple full-scale channels.
    // Keep original PCM16 units until final rounding to avoid a second quantization.
    let mono: Vec<f32> = audio
        .bytes
        .chunks_exact(frame_bytes)
        .map(|frame| {
            frame
                .chunks_exact(2)
                .map(|bytes| f64::from(i16::from_le_bytes([bytes[0], bytes[1]])))
                .sum::<f64>() as f32
                / f32::from(audio.channels)
        })
        .collect();
    let mono = if output_rate == audio.sample_rate_hz || mono.is_empty() {
        mono
    } else {
        downsample(&mono, audio.sample_rate_hz, output_rate)?
    };
    let bytes = mono
        .iter()
        .flat_map(|sample| {
            (sample
                .round()
                .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16)
                .to_le_bytes()
        })
        .collect();
    Ok(AudioInput {
        bytes,
        sample_rate_hz: output_rate,
        channels: 1,
        format: AudioFormat::Pcm16,
    })
}

fn downsample(input: &[f32], input_rate: u32, output_rate: u32) -> Result<Vec<f32>, OrallyError> {
    let mut common_divisor = input_rate;
    let mut remainder = output_rate;
    while remainder != 0 {
        (common_divisor, remainder) = (remainder, common_divisor % remainder);
    }
    if input_rate / common_divisor > MAX_FFT_INPUT_FRAMES {
        return Err(OrallyError::InvalidInput(
            "PCM16 sample rate requires an excessively large resampling filter".to_string(),
        ));
    }
    let mut resampler = FftFixedInOut::<f32>::new(
        input_rate as usize,
        output_rate as usize,
        RESAMPLING_CHUNK_FRAMES,
        1,
    )
    .map_err(|error| OrallyError::Audio(format!("could not prepare audio resampling: {error}")))?;
    let target_frames = usize::try_from(
        (input.len() as u128 * u128::from(output_rate)).div_ceil(u128::from(input_rate)),
    )
    .map_err(|_| OrallyError::InvalidInput("resampled audio is too large".to_string()))?;
    let delay = resampler.output_delay();
    let needed_frames = target_frames
        .checked_add(delay)
        .ok_or_else(|| OrallyError::InvalidInput("resampled audio is too large".to_string()))?;
    let mut output = Vec::with_capacity(needed_frames + resampler.output_frames_max());
    let mut buffer = resampler.output_buffer_allocate(true);
    let chunk_frames = resampler.input_frames_next();
    let mut chunks = input.chunks_exact(chunk_frames);
    for chunk in &mut chunks {
        let (_, written) = resampler
            .process_into_buffer(&[chunk], &mut buffer, None)
            .map_err(|error| OrallyError::Audio(format!("audio resampling failed: {error}")))?;
        output.extend_from_slice(&buffer[0][..written]);
    }
    let tail = chunks.remainder();
    if !tail.is_empty() {
        let (_, written) = resampler
            .process_partial_into_buffer(Some(&[tail]), &mut buffer, None)
            .map_err(|error| OrallyError::Audio(format!("audio resampling failed: {error}")))?;
        output.extend_from_slice(&buffer[0][..written]);
    }
    // Flush delayed tail samples even when input ended on a full processing block.
    while output.len() < needed_frames {
        let (_, written) = resampler
            .process_partial_into_buffer::<&[f32], Vec<f32>>(None, &mut buffer, None)
            .map_err(|error| OrallyError::Audio(format!("audio resampling failed: {error}")))?;
        output.extend_from_slice(&buffer[0][..written]);
    }
    output.copy_within(delay..needed_frames, 0);
    output.truncate(target_frames);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    fn pcm(rate: u32, channels: u16, samples: &[i16]) -> AudioInput {
        AudioInput {
            bytes: samples
                .iter()
                .flat_map(|sample| sample.to_le_bytes())
                .collect(),
            sample_rate_hz: rate,
            channels,
            format: AudioFormat::Pcm16,
        }
    }

    fn samples(audio: &AudioInput) -> Vec<i16> {
        audio
            .bytes
            .chunks_exact(2)
            .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
            .collect()
    }

    fn tone(rate: u32, frames: usize, frequency: f64) -> AudioInput {
        pcm(
            rate,
            1,
            &(0..frames)
                .map(|index| {
                    (0.6 * 32768.0 * (TAU * frequency * index as f64 / rate as f64).sin()).round()
                        as i16
                })
                .collect::<Vec<_>>(),
        )
    }

    fn rms(values: &[i16]) -> f64 {
        (values
            .iter()
            .map(|value| (f64::from(*value) / 32768.0).powi(2))
            .sum::<f64>()
            / values.len() as f64)
            .sqrt()
    }

    #[test]
    fn prepared_capture_is_mono_16khz_and_original_capture_is_unchanged() {
        let source = tone(48_000, 48_000, 1_000.0);
        let original = source.clone();
        let prepared = prepare_transcription_audio(&source).unwrap();
        assert_eq!(prepared.sample_rate_hz, 16_000);
        assert_eq!(prepared.channels, 1);
        assert_eq!(prepared.format, AudioFormat::Pcm16);
        assert_eq!(prepared.bytes.len(), 32_000);
        assert_eq!(source, original);
    }

    #[test]
    fn mono_at_or_below_16khz_is_exact_and_empty_audio_stays_empty() {
        for rate in [1, 8_000, 16_000] {
            let source = pcm(rate, 1, &[i16::MIN, -1, 0, 1, i16::MAX]);
            assert_eq!(prepare_transcription_audio(&source).unwrap(), source);
        }
        let prepared = prepare_transcription_audio(&pcm(48_000, 2, &[])).unwrap();
        assert!(prepared.bytes.is_empty());
        assert_eq!((prepared.sample_rate_hz, prepared.channels), (16_000, 1));
    }

    #[test]
    fn mixing_full_scale_channels_does_not_overflow_or_amplify() {
        let prepared = prepare_transcription_audio(&pcm(
            16_000,
            2,
            &[
                i16::MAX,
                i16::MAX,
                i16::MIN,
                i16::MIN,
                i16::MAX,
                i16::MIN,
                2_000,
                -1_000,
            ],
        ))
        .unwrap();
        let mixed = samples(&prepared);
        assert_eq!((prepared.channels, prepared.sample_rate_hz), (1, 16_000));
        assert_eq!(mixed[0], i16::MAX);
        assert_eq!(mixed[1], i16::MIN);
        assert!(mixed[2].abs() <= 1);
        assert_eq!(mixed[3], 500);
        let source = pcm(8_000, 3, &[i16::MAX; 6]);
        assert_eq!(
            samples(&prepare_transcription_audio(&source).unwrap()),
            [i16::MAX; 2]
        );
    }

    #[test]
    fn invalid_pcm_metadata_and_partial_frames_fail_before_processing() {
        let mut odd = pcm(48_000, 1, &[1]);
        odd.bytes.push(0);
        for source in [
            pcm(0, 1, &[0]),
            pcm(16_000, 0, &[0]),
            pcm(16_000, 2, &[0, 0, 0]),
            odd,
            AudioInput::wav(vec![0; 44]),
        ] {
            assert!(prepare_transcription_audio(&source).is_err());
        }
    }

    #[test]
    fn arbitrary_rates_and_partial_final_blocks_preserve_duration_and_tail() {
        for rate in [22_050, 44_100, 47_999, 48_000, 96_000] {
            let frames = rate as usize + 113;
            let mut source = tone(rate, frames, 1_000.0);
            // A short burst at the very end catches missing resampler flushing.
            source.bytes[..(frames - rate as usize / 100) * 2].fill(0);
            let prepared = prepare_transcription_audio(&source).unwrap();
            let expected = (frames as u128 * 16_000).div_ceil(u128::from(rate)) as usize;
            assert_eq!(prepared.bytes.len() / 2, expected, "rate {rate}");
            let result = samples(&prepared);
            assert!(
                rms(&result[result.len() - 80..]) > 0.2,
                "tail lost at rate {rate}"
            );
        }
    }

    #[test]
    fn downsampling_preserves_speech_band_and_rejects_aliasing() {
        for (rate, stop_frequency) in [(44_100, 12_000.0), (48_000, 12_000.0), (96_000, 20_000.0)] {
            let pass =
                samples(&prepare_transcription_audio(&tone(rate, rate as usize, 1_000.0)).unwrap());
            let stop = samples(
                &prepare_transcription_audio(&tone(rate, rate as usize, stop_frequency)).unwrap(),
            );
            let pass_rms = rms(&pass[1_000..pass.len() - 1_000]);
            let stop_rms = rms(&stop[1_000..stop.len() - 1_000]);
            assert!(
                (0.40..0.44).contains(&pass_rms),
                "pass band changed at {rate}: {pass_rms}"
            );
            assert!(
                stop_rms < pass_rms / 100.0,
                "alias rejection below 40 dB at {rate}: {stop_rms}"
            );
        }
    }
}
