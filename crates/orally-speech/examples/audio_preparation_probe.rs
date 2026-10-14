//! Offline release benchmark of production audio normalization and WAV encoding.
//! Uses only generated target/latency-probe/synthetic-*.wav fixtures. No network,
//! configuration, microphone, credentials, or transcript content is accessed.
//! Base64 uses the production library; JSON uses an equivalent data-URL payload.
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use orally_audio::{decode_wav_pcm16, encode_wav_pcm16, prepare_transcription_audio};
use orally_core::AudioInput;
use serde_json::{json, Value};
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const STAGES: [&str; 5] = ["prepare", "wav", "base64", "json", "total"];
const METHODS: [&str; 2] = ["original_48k", "normalized_16k"];

struct Options {
    audio: PathBuf,
    output: PathBuf,
    export_wav: Option<PathBuf>,
    runs: usize,
    warmup: usize,
}

#[derive(Clone)]
struct Measurement {
    nanos: [u128; 5],
    sizes: Value,
}

impl Measurement {
    fn json(&self) -> Value {
        let values = STAGES
            .iter()
            .zip(self.nanos)
            .map(|(stage, value)| (format!("{stage}_ns"), json!(value)))
            .collect();
        Value::Object(values)
    }
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/latency-probe")
}

fn options() -> Result<Options, &'static str> {
    let mut result = Options {
        audio: fixture_root().join("synthetic-48000.wav"),
        output: fixture_root().join("audio-preparation-rust.json"),
        export_wav: None,
        runs: 50,
        warmup: 5,
    };
    let mut args = std::env::args().skip(1);
    while let Some(option) = args.next() {
        let value = args.next().ok_or("missing_option_value")?;
        match option.as_str() {
            "--audio" => result.audio = value.into(),
            "--output" => result.output = value.into(),
            "--export-wav" => result.export_wav = Some(value.into()),
            "--runs" => result.runs = value.parse().map_err(|_| "invalid_runs")?,
            "--warmup" => result.warmup = value.parse().map_err(|_| "invalid_warmup")?,
            _ => return Err("unknown_option"),
        }
    }
    if !(30..=1_000).contains(&result.runs) || !(1..=100).contains(&result.warmup) {
        return Err("runs_must_be_30_to_1000_and_warmup_1_to_100");
    }
    Ok(result)
}

fn synthetic_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("synthetic-") && name.ends_with(".wav"))
}

fn output_path(root: &Path, path: &Path) -> Result<PathBuf, &'static str> {
    let parent = path.parent().ok_or("output_parent_missing")?;
    let parent = parent.canonicalize().map_err(|_| "output_parent_missing")?;
    if !parent.starts_with(root) {
        return Err("output_must_stay_in_synthetic_fixture_directory");
    }
    let path = parent.join(path.file_name().ok_or("output_filename_missing")?);
    if path.exists()
        && !path
            .canonicalize()
            .map_err(|_| "output_path_invalid")?
            .starts_with(root)
    {
        return Err("output_must_stay_in_synthetic_fixture_directory");
    }
    Ok(path)
}

fn measure(input: &AudioInput, normalize: bool) -> Result<Measurement, &'static str> {
    let started = Instant::now();
    let prepared = if normalize {
        Some(prepare_transcription_audio(black_box(input)).map_err(|_| "preparation_failed")?)
    } else {
        None
    };
    let audio = prepared.as_ref().unwrap_or(input);
    let preparation_done = Instant::now();
    let wav = encode_wav_pcm16(black_box(audio)).map_err(|_| "wav_encoding_failed")?;
    let wav_done = Instant::now();
    let encoded = STANDARD.encode(black_box(&wav));
    let base64_done = Instant::now();
    let encoded_bytes = encoded.len();
    let payload = json!({
        "model": "qwen3-asr-flash",
        "messages": [
            {"role": "system", "content": "Language hint: zh-CN"},
            {"role": "user", "content": [{
                "type": "input_audio",
                "input_audio": {"data": format!("data:audio/wav;base64,{encoded}")}
            }]}
        ],
        "stream": false
    });
    let body = serde_json::to_vec(&payload).map_err(|_| "json_encoding_failed")?;
    black_box(&body);
    let json_done = Instant::now();
    if normalize && (audio.sample_rate_hz != 16_000 || audio.channels != 1) {
        return Err("normalization_did_not_produce_16k_mono");
    }
    let frames = audio.bytes.len() / (usize::from(audio.channels) * 2);
    if normalize && frames != (input.bytes.len() / 2).div_ceil(3) {
        return Err("normalized_duration_changed");
    }
    Ok(Measurement {
        nanos: [
            preparation_done.duration_since(started).as_nanos(),
            wav_done.duration_since(preparation_done).as_nanos(),
            base64_done.duration_since(wav_done).as_nanos(),
            json_done.duration_since(base64_done).as_nanos(),
            json_done.duration_since(started).as_nanos(),
        ],
        sizes: json!({
            "pcm_bytes": audio.bytes.len(),
            "wav_bytes": wav.len(),
            "base64_ascii_bytes": encoded_bytes,
            "request_json_utf8_bytes": body.len(),
            "sample_rate_hz": audio.sample_rate_hz,
            "channels": audio.channels,
            "frames": frames,
            "duration_seconds": frames as f64 / f64::from(audio.sample_rate_hz),
        }),
    })
}

fn summarize(rows: &[Measurement]) -> Value {
    let result = STAGES
        .iter()
        .enumerate()
        .map(|(index, stage)| {
            let mut sorted: Vec<u128> = rows.iter().map(|row| row.nanos[index]).collect();
            sorted.sort_unstable();
            let count = sorted.len();
            let median = if count % 2 == 0 {
                (sorted[count / 2 - 1] as f64 + sorted[count / 2] as f64) / 2.0
            } else {
                sorted[count / 2] as f64
            };
            (
                stage.to_string(),
                json!({
                    "median_ms": median / 1_000_000.0,
                    "p95_ms": sorted[(count * 95).div_ceil(100) - 1] as f64 / 1_000_000.0,
                    "max_ms": sorted[count - 1] as f64 / 1_000_000.0,
                }),
            )
        })
        .collect();
    Value::Object(result)
}

fn run() -> Result<(), &'static str> {
    if cfg!(debug_assertions) {
        return Err("release_build_required_use_cargo_run_release");
    }
    let options = options()?;
    let root = fixture_root()
        .canonicalize()
        .map_err(|_| "synthetic_fixture_directory_missing")?;
    let input_path = options
        .audio
        .canonicalize()
        .map_err(|_| "audio_read_failed")?;
    if !input_path.starts_with(&root) || !synthetic_name(&input_path) {
        return Err("only_synthetic_fixture_audio_is_allowed");
    }
    let output_path = output_path(&root, &options.output)?;
    if output_path.extension().and_then(|ext| ext.to_str()) != Some("json") {
        return Err("report_must_use_json_extension");
    }
    let export_path = options
        .export_wav
        .as_ref()
        .map(|path| {
            let path = self::output_path(&root, path)?;
            if !synthetic_name(&path) || path == input_path {
                return Err("export_must_be_a_different_synthetic_wav");
            }
            Ok(path)
        })
        .transpose()?;
    let bytes = std::fs::read(&input_path).map_err(|_| "audio_read_failed")?;
    let input = decode_wav_pcm16(&bytes).map_err(|_| "invalid_pcm16_wav")?;
    if input.sample_rate_hz != 48_000 || input.channels != 1 || input.bytes.is_empty() {
        return Err("comparison_requires_nonempty_48k_mono_pcm16_fixture");
    }
    let mut warmups: [Vec<Measurement>; 2] = [Vec::new(), Vec::new()];
    let mut measured: [Vec<Measurement>; 2] = [Vec::new(), Vec::new()];
    for (method, rows) in warmups.iter_mut().enumerate() {
        for _ in 0..options.warmup {
            rows.push(measure(&input, method == 1)?);
        }
    }
    for run in 0..options.runs {
        // Alternate the first method to reduce bias from changing system load.
        for offset in 0..METHODS.len() {
            let method = (run + offset) % METHODS.len();
            measured[method].push(measure(&input, method == 1)?);
        }
    }
    let mut methods = serde_json::Map::new();
    for (method, name) in METHODS.iter().enumerate() {
        let expected_sizes = &warmups[method][0].sizes;
        if measured[method]
            .iter()
            .any(|row| &row.sizes != expected_sizes)
        {
            return Err("output_sizes_changed_between_runs");
        }
        methods.insert((*name).to_string(), json!({
            "output": expected_sizes,
            "first_preparation_call_ns": warmups[method][0].json(),
            "warmup_samples_ns": warmups[method].iter().map(Measurement::json).collect::<Vec<_>>(),
            "samples_ns": measured[method].iter().map(Measurement::json).collect::<Vec<_>>(),
            "summary": summarize(&measured[method]),
        }));
    }
    if let Some(path) = &export_path {
        let prepared = prepare_transcription_audio(&input).map_err(|_| "preparation_failed")?;
        let wav = encode_wav_pcm16(&prepared).map_err(|_| "wav_encoding_failed")?;
        std::fs::write(path, wav).map_err(|_| "export_write_failed")?;
    }
    let mut report = json!({
        "benchmark": "offline_production_rust_audio_preparation",
        "unix_time_seconds": SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| "clock_failed")?.as_secs(),
        "build": "release",
        "platform": {"os": std::env::consts::OS, "arch": std::env::consts::ARCH},
        "input": {"fixture": input_path.file_name().and_then(|name| name.to_str()), "file_bytes": bytes.len(),
                  "pcm_bytes": input.bytes.len(), "sample_rate_hz": input.sample_rate_hz,
                  "channels": input.channels, "duration_seconds": input.bytes.len() as f64 / 96_000.0},
        "runs_per_method": options.runs,
        "warmup_runs_per_method": options.warmup,
        "percentile_method": "nearest_rank",
        "timing_scope": {
            "prepare": "production prepare_transcription_audio on PCM16; original_48k is an unchanged borrowed input",
            "wav": "production encode_wav_pcm16",
            "base64": "production base64 0.22 STANDARD library",
            "json": "data URL concatenation, equivalent chat payload construction and serde_json::to_vec",
            "total": "four contiguous timed stages, excluding final buffer destruction",
            "excluded": ["disk I/O", "WAV input decode", "reqwest request construction", "network", "ASR", "LLM", "insertion", "metadata"]
        },
        "limitations": ["one machine and one synthetic fixture", "JSON payload uses Value rather than provider-private structs; this is not a measurement of the entire provider call", "first preparation call is not cold application startup"],
        "exported_fixture": export_path.as_ref().and_then(|path| path.file_name()).and_then(|name| name.to_str()),
        "methods": methods,
    });
    let serialized = serde_json::to_vec_pretty(&report).map_err(|_| "report_encoding_failed")?;
    std::fs::write(output_path, serialized).map_err(|_| "report_write_failed")?;
    // Keep console output compact; the local report retains raw nanosecond samples.
    for name in METHODS {
        if let Some(method) = report["methods"][name].as_object_mut() {
            method.remove("samples_ns");
            method.remove("warmup_samples_ns");
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|_| "report_encoding_failed")?
    );
    Ok(())
}

fn main() {
    if let Err(code) = run() {
        println!(
            "{}",
            json!({"type": "audio_preparation_probe_error", "code": code})
        );
        std::process::exit(2);
    }
}
