use super::*;

fn stereo_recording(frames: usize) -> AudioInput {
    AudioInput {
        bytes: [12_000_i16.to_le_bytes(), (-4_000_i16).to_le_bytes()]
            .concat()
            .repeat(frames),
        sample_rate_hz: 48_000,
        channels: 2,
        format: AudioFormat::Pcm16,
    }
}

fn multipart_audio(request: &[u8]) -> AudioInput {
    let start = request.windows(4).position(|part| part == b"RIFF").unwrap();
    let length = u32::from_le_bytes(request[start + 4..start + 8].try_into().unwrap()) as usize + 8;
    decode_wav_pcm16(&request[start..start + length]).unwrap()
}

fn assert_prepared(audio: &AudioInput, frames: usize) {
    assert_eq!(audio.sample_rate_hz, 16_000);
    assert_eq!(audio.channels, 1);
    assert_eq!(audio.bytes.len(), frames * 2);
    // Ignore filter boundaries: the two channels average to 4,000, not 8,000.
    for sample in audio.bytes[400..audio.bytes.len() - 400].chunks_exact(2) {
        assert!((i32::from(i16::from_le_bytes([sample[0], sample[1]])) - 4_000).abs() <= 5);
    }
}

#[test]
fn multipart_and_explicit_chat_upload_normalized_audio() {
    for chat in [false, true] {
        let response = if chat {
            "{\"choices\":[{\"message\":{\"content\":\"heard\"}}]}"
        } else {
            "{\"text\":\"heard\"}"
        };
        let (base_url, server) = serve_script(vec![("200 OK", response)]);
        let provider: Box<dyn AsrProvider> = if chat {
            Box::new(
                ChatAudioAsrProvider::new(ChatAudioAsrConfig::new(
                    base_url,
                    "fixture-key",
                    "transcriber",
                ))
                .unwrap(),
            )
        } else {
            Box::new(
                OpenAiCompatibleAsrProvider::new(OpenAiCompatibleAsrConfig::new(
                    base_url,
                    "fixture-key",
                    "transcriber",
                ))
                .unwrap(),
            )
        };
        let recording = stereo_recording(48_000);
        let retained = recording.clone();
        assert_eq!(
            provider.transcribe(recording.clone()).unwrap().text,
            "heard"
        );
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 1);
        let uploaded = if chat {
            standard_chat_audio(&requests[0])
        } else {
            multipart_audio(&requests[0])
        };
        assert_prepared(&uploaded, 16_000);
        assert_eq!(recording, retained);
    }
}

#[test]
fn protocol_fallback_reuses_identical_prepared_audio() {
    let (base_url, server) = serve_script(vec![
        ("404 Not Found", "{}"),
        ("400 Bad Request", "{}"),
        (
            "200 OK",
            "{\"choices\":[{\"message\":{\"content\":\"heard\"}}]}",
        ),
    ]);
    let wav = encode_wav_pcm16(&stereo_recording(48_000)).unwrap();
    assert_eq!(
        auto_provider(base_url)
            .transcribe(AudioInput::wav(wav))
            .unwrap()
            .text,
        "heard"
    );
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 3);
    let multipart = multipart_audio(&requests[0]);
    assert_prepared(&multipart, 16_000);
    assert_eq!(data_url_audio(&requests[1]), multipart);
    assert_eq!(standard_chat_audio(&requests[2]), multipart);
}

#[test]
fn long_audio_is_normalized_before_splitting_without_losing_tail() {
    let recording = stereo_recording(48_000 * 60 + 3);
    let chunks = audio_to_wav_chunks(recording).unwrap();
    assert_eq!(chunks.len(), 2);
    let first = decode_wav_pcm16(&chunks[0]).unwrap();
    let tail = decode_wav_pcm16(&chunks[1]).unwrap();
    assert_prepared(&first, 16_000 * 60);
    assert_eq!(
        (tail.sample_rate_hz, tail.channels, tail.bytes.len()),
        (16_000, 1, 2)
    );
}

#[test]
fn unsupported_wav_encoding_keeps_existing_provider_passthrough() {
    let mut wav = encode_wav_pcm16(&fixture_audio()).unwrap();
    wav[20..22].copy_from_slice(&3_u16.to_le_bytes()); // IEEE float, not PCM16.
    assert_eq!(
        audio_to_wav_chunks(AudioInput::wav(wav.clone())).unwrap(),
        vec![wav]
    );
}
