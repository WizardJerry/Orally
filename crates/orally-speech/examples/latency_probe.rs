//! Opt-in, real-provider latency probe. Configuration is read without migration.
//! Only generated target/latency-probe/synthetic-*.wav fixtures are accepted.
//! No transcripts, prompts, credentials, provider error bodies, or URLs are printed.
use orally_config::{AppConfig, ConfigStore};
use orally_core::{AudioInput, OrallyError, Transcript};
use orally_speech::{
    refine_transcript, AiPostprocessPlan, AsrProtocol, CredentialSource, RecognitionPlan,
    RefinementPlan, SpeechOutcome, SpeechPlan, SpeechProcessor,
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const FIXED_TEXT: &str = "嗯请把明天下午三点的会议改到四点然后提醒大家提前准备项目进展谢谢";
// Matches scripts/generate-latency-fixture.ps1; opt-in so other synthetic fixtures remain usable.
const AUDIO_FIXTURE_TEXT: &str = "今天我们测试语音输入的响应速度。用户说完一段话以后，希望系统尽快把准确的文字输入到当前窗口。第一步检查录音文件的长度和大小，第二步测量语音识别服务的处理时间，第三步测量文字整理服务的响应时间。测试过程中使用固定的公开文本，不包含真实用户的私人信息。我们会分别记录每个请求的开始时间和结束时间，比较不同方案的速度和识别结果。最终目标是让日常输入更加流畅，同时保持原有的意思，正确处理中文标点和常见的技术词汇。";

struct Options {
    directory: PathBuf,
    audio: Option<PathBuf>,
    mode: String,
    runs: usize,
    reuse: bool,
    threshold_ms: u128,
    timeout: Duration,
    protocol: AsrProtocol,
    text_repetitions: usize,
    check_fixture: bool,
}

fn main() {
    match run() {
        Ok(true) => (),
        Ok(false) => std::process::exit(1),
        Err(code) => {
            println!("{}", json!({"type":"probe_error", "code":code}));
            std::process::exit(2);
        }
    }
}

fn options() -> Result<Options, &'static str> {
    let mut args = std::env::args().skip(1);
    let mut options = Options {
        directory: PathBuf::new(),
        audio: None,
        mode: "full".into(),
        runs: 3,
        reuse: false,
        threshold_ms: 5000,
        timeout: Duration::from_secs(120),
        protocol: AsrProtocol::Auto,
        text_repetitions: 1,
        check_fixture: false,
    };
    while let Some(arg) = args.next() {
        if arg == "--reuse" {
            options.reuse = true;
            continue;
        }
        if arg == "--check-fixture" {
            options.check_fixture = true;
            continue;
        }
        let value = args.next().ok_or("missing_option_value")?;
        match arg.as_str() {
            "--config-dir" => options.directory = value.into(),
            "--audio" => options.audio = Some(value.into()),
            "--mode" => options.mode = value,
            "--runs" => options.runs = value.parse().map_err(|_| "invalid_runs")?,
            "--threshold-ms" => {
                options.threshold_ms = value.parse().map_err(|_| "invalid_threshold")?
            }
            "--request-timeout-ms" => {
                options.timeout =
                    Duration::from_millis(value.parse().map_err(|_| "invalid_timeout")?)
            }
            "--protocol" => options.protocol = value.parse().map_err(|_| "invalid_protocol")?,
            "--text-repetitions" => {
                options.text_repetitions = value.parse().map_err(|_| "invalid_text_repetitions")?
            }
            _ => return Err("unknown_option"),
        }
    }
    if options.directory.as_os_str().is_empty() {
        return Err("explicit_config_directory_required");
    }
    if !matches!(options.mode.as_str(), "full" | "recognize" | "refine") {
        return Err("mode_must_be_full_recognize_or_refine");
    }
    if options.runs == 0 || options.runs > 20 {
        return Err("runs_must_be_between_1_and_20");
    }
    if options.text_repetitions == 0 || options.text_repetitions > 30 {
        return Err("text_repetitions_must_be_between_1_and_30");
    }
    if options.check_fixture && options.mode == "refine" {
        return Err("fixture_check_requires_audio");
    }
    Ok(options)
}

fn workspace_fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/latency-probe")
}

fn load_audio(path: &Path) -> Result<(AudioInput, Value), &'static str> {
    let root = workspace_fixture_dir()
        .canonicalize()
        .map_err(|_| "synthetic_fixture_directory_missing")?;
    let path = path.canonicalize().map_err(|_| "audio_read_failed")?;
    let allowed_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("synthetic-") && name.ends_with(".wav"));
    if !path.starts_with(root) || !allowed_name {
        return Err("only_synthetic_fixture_audio_is_allowed");
    }
    let bytes = std::fs::read(path).map_err(|_| "audio_read_failed")?;
    let decoded = orally_audio::decode_wav_pcm16(&bytes).map_err(|_| "invalid_pcm16_wav")?;
    let duration_ms = (decoded.bytes.len() as u128 * 1000)
        / (u128::from(decoded.sample_rate_hz) * u128::from(decoded.channels) * 2);
    let metadata = json!({
        "bytes": bytes.len(), "duration_ms": duration_ms,
        "sample_rate_hz": decoded.sample_rate_hz, "channels": decoded.channels,
    });
    Ok((AudioInput::wav(bytes), metadata))
}

// Keep these plan builders aligned with the desktop entry point. No settings are saved.
fn desktop_refinement(config: &AppConfig) -> RefinementPlan {
    if !matches!(config.postprocess.mode.as_str(), "llm" | "ai") {
        return RefinementPlan::LocalBasicCleanup;
    }
    if config.postprocess.models.is_empty() {
        return RefinementPlan::AiPostprocessing(AiPostprocessPlan {
            base_url: config.postprocess.base_url.clone(),
            model: config.postprocess.model.clone(),
            credential: CredentialSource::new(
                config.postprocess.api_key.clone(),
                config.postprocess.api_key_env.clone(),
            ),
            system_prompt: config.postprocess.system_prompt.clone(),
            user_template: config.postprocess.user_template.clone(),
            enable_thinking: config.postprocess.thinking_mode.enable_thinking(),
            fallback_to_local_basic_cleanup: config.postprocess.fallback_to_builtin,
        });
    }
    RefinementPlan::AiPipeline(
        config
            .postprocess
            .models
            .iter()
            .filter(|model| model.enabled)
            .map(|model| AiPostprocessPlan {
                base_url: model.base_url.clone(),
                model: model.model.clone(),
                credential: CredentialSource::new(model.api_key.clone(), model.api_key_env.clone()),
                system_prompt: model.composed_system_prompt(),
                user_template: model.user_template.clone(),
                enable_thinking: model.thinking_mode.enable_thinking(),
                fallback_to_local_basic_cleanup: model.fallback_to_builtin,
            })
            .collect(),
    )
}

fn desktop_plan(config: &AppConfig, options: &Options) -> SpeechPlan {
    SpeechPlan {
        recognition: RecognitionPlan {
            base_url: config.asr.base_url.clone(),
            model: config.asr.model.clone(),
            credential: CredentialSource::new(
                config.asr.api_key.clone(),
                config.asr.api_key_env.clone(),
            ),
            protocol: options.protocol,
            language: config.asr.language.clone(),
        },
        refinement: if options.mode == "recognize" {
            RefinementPlan::Raw
        } else {
            desktop_refinement(config)
        },
        locale: config.output.locale.clone(),
    }
}

fn error_class(error: &OrallyError) -> &'static str {
    match error {
        OrallyError::InvalidInput(_) => "invalid_input",
        OrallyError::Audio(_) => "audio",
        OrallyError::Asr(_) => "recognition",
        OrallyError::Processing(_) => "refinement",
        OrallyError::Insertion(_) => "insertion",
    }
}

fn requests(directory: &Path, first_id: u64) -> Result<Vec<Value>, &'static str> {
    let mut events = Vec::new();
    let mut request_sizes = std::collections::BTreeMap::new();
    for entry in std::fs::read_dir(directory).map_err(|_| "trace_read_failed")? {
        let path = entry.map_err(|_| "trace_read_failed")?.path();
        if !path
            .extension()
            .is_some_and(|extension| extension == "jsonl")
        {
            continue;
        }
        for line in std::fs::read_to_string(path)
            .map_err(|_| "trace_read_failed")?
            .lines()
        {
            let record: Value = serde_json::from_str(line).map_err(|_| "trace_parse_failed")?;
            let event = &record["event"];
            if record["pid"].as_u64() == Some(u64::from(std::process::id()))
                && event["type"] == "request_started"
            {
                request_sizes.insert(
                    event["request_id"].as_u64().unwrap_or(0),
                    event["request_bytes"].clone(),
                );
            }
            if record["pid"].as_u64() == Some(u64::from(std::process::id()))
                && event["type"] == "request_finished"
                && event["request_id"].as_u64().unwrap_or(0) >= first_id
            {
                // Select fields explicitly even though the diagnostics logger is content-free.
                events.push(json!({
                    "id": event["request_id"], "protocol": event["protocol"],
                    "status": event["status"], "outcome": event["outcome"],
                    "elapsed_ms": event["elapsed_ms"],
                }));
            }
        }
    }
    for event in &mut events {
        event["request_bytes"] = request_sizes
            .get(&event["id"].as_u64().unwrap_or(0))
            .cloned()
            .unwrap_or(Value::Null);
    }
    events.sort_by_key(|event| event["id"].as_u64().unwrap_or(0));
    Ok(events)
}

fn normalized_text(text: &str) -> String {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn run() -> Result<bool, &'static str> {
    let options = options()?;
    // ConfigStore::load is read-only, unlike load_or_default's executable-local migration.
    let config = ConfigStore::new(&options.directory)
        .load()
        .map_err(|_| "config_load_failed")?;
    if !config.privacy.allow_external_requests {
        return Err("external_requests_disabled_in_config");
    }
    let (audio, audio_metadata) = if options.mode != "refine" {
        let path = options.audio.as_ref().ok_or("synthetic_audio_required")?;
        let (audio, metadata) = load_audio(path)?;
        (Some(audio), metadata)
    } else {
        (None, Value::Null)
    };
    let trace_dir = workspace_fixture_dir().join(format!(
        "trace-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "clock_failed")?
            .as_millis()
    ));
    orally_diagnostics::configure(trace_dir.clone(), false, 1).map_err(|_| "trace_setup_failed")?;
    println!(
        "{}",
        json!({
            "type": "probe_configuration", "mode": options.mode, "runs": options.runs,
            "reuse_speech_processor": options.reuse && options.mode != "refine",
            "refinement_adapter_lifetime": "per_refinement_request_as_in_desktop",
            "recognition_protocol": format!("{:?}", options.protocol),
            "threshold_ms": options.threshold_ms, "audio": audio_metadata,
            "fixed_text_chars": FIXED_TEXT.chars().count() * options.text_repetitions,
            "boundary": "processor construction plus audio-to-final-text; excludes recording and insertion",
            "stage_timings": "HTTP attempts including protocol negotiation; local preparation is in other_ms",
            "check_fixture": options.check_fixture,
        })
    );

    let fixed_text = FIXED_TEXT.repeat(options.text_repetitions);
    let mut processor = None;
    let mut first_id = 1;
    let mut all_pass = true;
    let mut measurements = Vec::new();
    for run in 1..=options.runs {
        let total_start = Instant::now();
        let mut construction_ms = 0;
        if options.mode != "refine" && (processor.is_none() || !options.reuse) {
            // Drop a prior adapter outside the measured construction interval.
            drop(processor.take());
            let construction_start = Instant::now();
            processor = Some(
                SpeechProcessor::new_with_timeout(desktop_plan(&config, &options), options.timeout)
                    .map_err(|_| "processor_build_failed")?,
            );
            construction_ms = construction_start.elapsed().as_millis();
        }
        let process_start = Instant::now();
        let outcome: Result<SpeechOutcome, OrallyError> = if options.mode == "refine" {
            refine_transcript(
                desktop_refinement(&config),
                Transcript {
                    text: fixed_text.clone(),
                    language: None,
                    segments: Vec::new(),
                },
                &config.output.locale,
                options.timeout,
            )
        } else {
            processor
                .as_ref()
                .ok_or("processor_missing")?
                .process(audio.as_ref().ok_or("audio_missing")?.clone())
        };
        let process_ms = process_start.elapsed().as_millis();
        let elapsed_ms = total_start.elapsed().as_millis();
        let trace = requests(&trace_dir, first_id)?;
        if let Some(last) = trace.last() {
            first_id = last["id"].as_u64().unwrap_or(first_id) + 1;
        }
        let recognize_http_ms: u128 = trace
            .iter()
            .filter(|event| event["protocol"] != "chat_text")
            .map(|event| u128::from(event["elapsed_ms"].as_u64().unwrap_or(0)))
            .sum();
        let refine_http_ms: u128 = trace
            .iter()
            .filter(|event| event["protocol"] == "chat_text")
            .map(|event| u128::from(event["elapsed_ms"].as_u64().unwrap_or(0)))
            .sum();
        let degraded = outcome.as_ref().ok().is_some_and(|result| {
            result
                .changes
                .iter()
                .any(|change| change.contains("AI postprocessor failed"))
        });
        let fixture_match = options.check_fixture.then(|| {
            outcome.as_ref().ok().is_some_and(|result| {
                normalized_text(&result.raw_transcript.text) == normalized_text(AUDIO_FIXTURE_TEXT)
            })
        });
        let success = outcome.is_ok() && !degraded && fixture_match != Some(false);
        let pass = success && elapsed_ms < options.threshold_ms;
        all_pass &= pass;
        measurements.push(elapsed_ms);
        println!(
            "{}",
            json!({
                "type": "measurement", "run": run, "construction_ms": construction_ms,
                "process_ms": process_ms, "elapsed_ms": elapsed_ms,
                "recognize_http_ms": recognize_http_ms, "refine_http_ms": refine_http_ms,
                "other_ms": elapsed_ms.saturating_sub(construction_ms + recognize_http_ms + refine_http_ms),
                "success": success, "degraded_to_local_cleanup": degraded,
                "asr_fixture_normalized_exact_match": fixture_match,
                "error_class": outcome.as_ref().err().map(error_class),
                "raw_chars": outcome.as_ref().ok().map(|result| result.raw_transcript.text.chars().count()),
                "final_chars": outcome.as_ref().ok().map(|result| result.final_text.chars().count()),
                "pass": pass, "requests": trace,
            })
        );
    }
    measurements.sort_unstable();
    let middle = measurements.len() / 2;
    let median_ms = if measurements.len() % 2 == 0 {
        (measurements[middle - 1] as f64 + measurements[middle] as f64) / 2.0
    } else {
        measurements[middle] as f64
    };
    println!(
        "{}",
        json!({
            "type": "verdict", "pass": all_pass, "threshold_ms": options.threshold_ms,
            "min_ms": measurements[0], "max_ms": measurements[measurements.len() - 1],
            "median_ms": median_ms,
            "criterion": "every run succeeds without local fallback and elapsed_ms < threshold_ms",
        })
    );
    Ok(all_pass)
}
