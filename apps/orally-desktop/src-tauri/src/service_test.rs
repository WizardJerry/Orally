//! Draft-only service checks. These functions never load or persist application data.

use orally_audio::encode_wav_pcm16;
use orally_config::AppConfig;
use orally_core::{AudioFormat, AudioInput, OrallyError, Transcript};
use orally_speech::{
    refine_transcript, AiPostprocessPlan, CredentialSource, RefinementPlan, SpeechOutcome,
    SpeechProcessor,
};
use serde::Serialize;
use std::time::{Duration, Instant};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_AUDIO_BYTES: usize = 15 * 1024 * 1024;
const MAX_TEXT_CHARS: usize = 100_000;

#[derive(Debug, Serialize)]
pub(super) struct ConnectionTestResult {
    pub message: String,
    pub elapsed_ms: u64,
}

#[derive(Debug, Serialize)]
pub(super) struct InputTestResult {
    pub raw_transcript: String,
    pub final_text: String,
    pub elapsed_ms: u64,
    pub warnings: Vec<String>,
}

#[tauri::command]
pub(super) async fn test_service_connection(
    config: AppConfig,
    model_id: Option<String>,
) -> Result<ConnectionTestResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        check_service_connection(&config, model_id.as_deref())
    })
    .await
    .map_err(|_| "服务测试任务执行失败，请重试。".to_string())?
}

#[tauri::command]
pub(super) async fn test_config_input(
    config: AppConfig,
    input_text: Option<String>,
    audio_bytes: Option<Vec<u8>>,
    audio_name: Option<String>,
) -> Result<InputTestResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        check_config_input(&config, input_text, audio_bytes, audio_name.as_deref())
    })
    .await
    .map_err(|_| "配置测试任务执行失败，请重试。".to_string())?
}

fn elapsed_ms(start: Instant) -> u64 {
    start.elapsed().as_millis().min(u64::MAX as u128) as u64
}

pub(super) fn require_external_requests(config: &AppConfig) -> Result<(), String> {
    if config.privacy.allow_external_requests {
        Ok(())
    } else {
        Err("隐私设置已禁止外部请求，请先允许外部请求再测试服务。".to_string())
    }
}

fn check_service_connection(
    config: &AppConfig,
    model_id: Option<&str>,
) -> Result<ConnectionTestResult, String> {
    require_external_requests(config)?;
    let start = Instant::now();
    let result = match model_id {
        Some(model_id) => connection_model_plan(config, model_id).and_then(|plan| {
            refine_transcript(
                RefinementPlan::AiPostprocessing(plan),
                text_transcript("Connection test".to_string()),
                &config.output.locale,
                REQUEST_TIMEOUT,
            )
            .map(|_| ())
        }),
        None => {
            let mut plan = super::build_speech_plan(config);
            plan.refinement = RefinementPlan::Raw;
            SpeechProcessor::new_with_timeout(plan, REQUEST_TIMEOUT)
                .map_err(OrallyError::from)
                .and_then(|speech| speech.process(silent_wav()?).map(|_| ()))
        }
    };
    // A parsed successful response with no transcript is normal for silent audio.
    // Only this exact adapter error confirms that the protocol request succeeded.
    let silent_response = model_id.is_none()
        && matches!(&result, Err(OrallyError::Asr(message)) if message == "provider returned an empty transcript");
    if !silent_response {
        result.map_err(|error| sanitize_test_error(&error, config))?;
    }
    Ok(ConnectionTestResult {
        message: if silent_response {
            "服务连接成功，端点、凭据、模型和协议已通过实际请求验证；静音音频未产生转写文本。"
                .to_string()
        } else {
            "服务连接成功，端点、凭据、模型和协议已通过实际请求验证。".to_string()
        },
        elapsed_ms: elapsed_ms(start),
    })
}

fn connection_model_plan(
    config: &AppConfig,
    model_id: &str,
) -> Result<AiPostprocessPlan, OrallyError> {
    let (base_url, model, api_key, api_key_env) = if config.postprocess.models.is_empty() {
        (
            &config.postprocess.base_url,
            &config.postprocess.model,
            &config.postprocess.api_key,
            &config.postprocess.api_key_env,
        )
    } else {
        let selected = config
            .postprocess
            .models
            .iter()
            .find(|model| model.id == model_id)
            .ok_or_else(|| OrallyError::InvalidInput("要测试的模型节点不存在。".to_string()))?;
        (
            &selected.base_url,
            &selected.model,
            &selected.api_key,
            &selected.api_key_env,
        )
    };
    Ok(AiPostprocessPlan {
        base_url: base_url.clone(),
        model: model.clone(),
        credential: CredentialSource::new(api_key.clone(), api_key_env.clone()),
        system_prompt: "This is a service connection test. Return only OK.".to_string(),
        user_template: "Return OK.".to_string(),
        fallback_to_local_basic_cleanup: false,
    })
}

fn silent_wav() -> Result<AudioInput, OrallyError> {
    encode_wav_pcm16(&AudioInput {
        bytes: vec![0; 16_000],
        sample_rate_hz: 16_000,
        channels: 1,
        format: AudioFormat::Pcm16,
    })
    .map(AudioInput::wav)
}

fn text_transcript(text: String) -> Transcript {
    Transcript {
        text,
        language: None,
        segments: Vec::new(),
    }
}

fn needs_ai_request(plan: &RefinementPlan) -> bool {
    match plan {
        RefinementPlan::AiPostprocessing(_) => true,
        RefinementPlan::AiPipeline(models) => !models.is_empty(),
        RefinementPlan::Raw | RefinementPlan::LocalBasicCleanup => false,
    }
}

fn validate_audio(bytes: &[u8], name: Option<&str>) -> Result<(), String> {
    if bytes.len() > MAX_AUDIO_BYTES {
        return Err("测试音频不能超过 15 MiB。".to_string());
    }
    if name.is_some_and(|name| !name.trim().is_empty() && !name.to_lowercase().ends_with(".wav"))
        || bytes.len() < 44
        || &bytes[..4] != b"RIFF"
        || &bytes[8..12] != b"WAVE"
    {
        return Err("请提供 WAV 格式的测试音频。".to_string());
    }
    Ok(())
}

fn check_config_input(
    config: &AppConfig,
    input_text: Option<String>,
    audio_bytes: Option<Vec<u8>>,
    audio_name: Option<&str>,
) -> Result<InputTestResult, String> {
    let start = Instant::now();
    let outcome: Result<SpeechOutcome, OrallyError> = match (input_text, audio_bytes) {
        (Some(text), None) => {
            if text.trim().is_empty() {
                return Err("请输入用于测试的文字。".to_string());
            }
            if text.chars().count() > MAX_TEXT_CHARS {
                return Err("测试文字不能超过 100000 个字符。".to_string());
            }
            let plan = super::build_refinement_plan(config);
            if needs_ai_request(&plan) {
                require_external_requests(config)?;
            }
            refine_transcript(
                plan,
                text_transcript(text),
                &config.output.locale,
                REQUEST_TIMEOUT,
            )
        }
        (None, Some(bytes)) => {
            validate_audio(&bytes, audio_name)?;
            require_external_requests(config)?;
            SpeechProcessor::new_with_timeout(super::build_speech_plan(config), REQUEST_TIMEOUT)
                .map_err(OrallyError::from)
                .and_then(|speech| speech.process(AudioInput::wav(bytes)))
        }
        (Some(_), Some(_)) => return Err("文字和音频请选择其中一种进行测试。".to_string()),
        (None, None) => return Err("请提供测试文字或 WAV 音频。".to_string()),
    };
    outcome_result(config, outcome, start)
}

pub(super) fn process_recorded_input(
    config: &AppConfig,
    audio: AudioInput,
    asr_only: bool,
) -> Result<InputTestResult, String> {
    require_external_requests(config)?;
    if audio.bytes.is_empty() {
        return Err("麦克风未采集到音频，请检查输入设备后重新录音。".to_string());
    }
    if audio.bytes.len() > MAX_AUDIO_BYTES {
        return Err("测试录音不能超过 15 MiB，请缩短录音后重试。".to_string());
    }
    let start = Instant::now();
    let mut plan = super::build_speech_plan(config);
    if asr_only {
        plan.refinement = RefinementPlan::Raw;
    }
    let outcome = SpeechProcessor::new_with_timeout(plan, REQUEST_TIMEOUT)
        .map_err(OrallyError::from)
        .and_then(|speech| speech.process(audio));
    outcome_result(config, outcome, start)
}

fn outcome_result(
    config: &AppConfig,
    outcome: Result<SpeechOutcome, OrallyError>,
    start: Instant,
) -> Result<InputTestResult, String> {
    let outcome = outcome.map_err(|error| sanitize_test_error(&error, config))?;
    Ok(InputTestResult {
        raw_transcript: outcome.raw_transcript.text,
        final_text: outcome.final_text,
        elapsed_ms: elapsed_ms(start),
        warnings: outcome
            .changes
            .iter()
            .filter(|change| change.contains("AI postprocessor failed"))
            .map(|warning| sanitize_error(warning, config))
            .collect(),
    })
}

fn sanitize_test_error(error: &OrallyError, config: &AppConfig) -> String {
    let message = sanitize_error(&error.to_string(), config);
    if matches!(error, OrallyError::Asr(reason) if reason != "provider returned an empty transcript")
    {
        format!("{message}。请检查语音服务地址、模型名称及音频输入支持；ASR 支持 OpenAI 兼容转写和 Chat Audio。")
    } else {
        message
    }
}

fn sanitize_error(message: &str, config: &AppConfig) -> String {
    let mut message = message.to_string();
    let node_prefix = "post-processing model node ";
    if let Some(node_start) = message.find(node_prefix) {
        let number: String = message[node_start + node_prefix.len()..]
            .chars()
            .take_while(|value| value.is_ascii_digit())
            .collect();
        if let Some(model) = number
            .parse::<usize>()
            .ok()
            .and_then(|number| number.checked_sub(1))
            .and_then(|index| {
                config
                    .postprocess
                    .models
                    .iter()
                    .filter(|model| model.enabled)
                    .nth(index)
            })
        {
            let label = if model.name.trim().is_empty() {
                model.model.as_str()
            } else {
                model.name.as_str()
            };
            message = message.replacen(
                &format!("{node_prefix}{number}"),
                &format!("后处理节点「{label}」（{}）", model.id),
                1,
            );
        }
    }
    // Provider error bodies are untrusted and may echo request credentials.
    if let Some(status_start) = message.find("returned HTTP ") {
        if let Some(body_start) = message[status_start..].find(':') {
            message.truncate(status_start + body_start);
        }
    }
    if let Some(url_start) = message.find(" for url (") {
        message.truncate(url_start);
        message.push_str("（请检查端点、网络或代理）");
    }
    let mut credentials = vec![
        (&config.asr.api_key, &config.asr.api_key_env),
        (&config.postprocess.api_key, &config.postprocess.api_key_env),
    ];
    credentials.extend(
        config
            .postprocess
            .models
            .iter()
            .map(|model| (&model.api_key, &model.api_key_env)),
    );
    let mut secrets: Vec<String> = credentials
        .into_iter()
        .flat_map(|(direct, variable)| {
            direct
                .clone()
                .into_iter()
                .chain(std::env::var(variable).ok())
        })
        .filter(|secret| !secret.trim().is_empty())
        .collect();
    secrets.sort_by_key(|secret| std::cmp::Reverse(secret.len()));
    secrets.dedup();
    for secret in secrets {
        message = message.replace(&secret, "[redacted]");
    }
    message.chars().take(1000).collect()
}

#[cfg(test)]
mod tests;
