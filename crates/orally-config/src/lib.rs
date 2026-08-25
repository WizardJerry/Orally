use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::PathBuf;

pub const ALIYUN_OPENAI_COMPAT_BASE_URL: &str =
    "https://ws-xzr3kkbjij82s72f.cn-beijing.maas.aliyuncs.com/compatible-mode/v1";
pub const OPENAI_COMPAT_API_KEY_ENV: &str = "ORALLY_OPENAI_COMPAT_API_KEY";
pub const ALIYUN_ASR_MODEL: &str = "qwen3-asr-flash";
pub const ALIYUN_POSTPROCESS_MODEL: &str = "deepseek-v4-flash-0731";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub asr: AsrConfig,
    #[serde(default)]
    pub postprocess: PostprocessConfig,
    #[serde(default)]
    pub output: OutputConfig,
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub hotkey: HotkeyConfig,
    #[serde(default)]
    pub privacy: PrivacyConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            asr: AsrConfig::default(),
            postprocess: PostprocessConfig::default(),
            output: OutputConfig::default(),
            audio: AudioConfig::default(),
            hotkey: HotkeyConfig::default(),
            privacy: PrivacyConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn aliyun_openai_preset() -> Self {
        Self {
            asr: AsrConfig {
                base_url: ALIYUN_OPENAI_COMPAT_BASE_URL.to_string(),
                model: ALIYUN_ASR_MODEL.to_string(),
                protocol: "chat-audio".to_string(),
                api_key: None,
                api_key_env: OPENAI_COMPAT_API_KEY_ENV.to_string(),
                language: None,
                prompt: None,
            },
            postprocess: PostprocessConfig {
                mode: "llm".to_string(),
                base_url: ALIYUN_OPENAI_COMPAT_BASE_URL.to_string(),
                model: ALIYUN_POSTPROCESS_MODEL.to_string(),
                api_key_env: OPENAI_COMPAT_API_KEY_ENV.to_string(),
                ..PostprocessConfig::default()
            },
            output: OutputConfig {
                locale: "zh-CN".to_string(),
                raw: false,
                show_changes: false,
                insert: false,
                paste_delay_ms: 300,
                restore_clipboard: true,
                restore_clipboard_delay_ms: 250,
            },
            ..Self::default()
        }
    }

    pub fn openrouter_preset() -> Self {
        Self {
            asr: AsrConfig {
                base_url: "https://openrouter.ai/api/v1".to_string(),
                model: "qwen/qwen3-asr-flash-2026-02-10".to_string(),
                protocol: "chat-audio".to_string(),
                api_key: None,
                api_key_env: "OPENROUTER_API_KEY".to_string(),
                language: None,
                prompt: None,
            },
            output: OutputConfig {
                locale: "zh-CN".to_string(),
                raw: false,
                show_changes: false,
                insert: false,
                paste_delay_ms: 300,
                restore_clipboard: true,
                restore_clipboard_delay_ms: 250,
            },
            ..Self::default()
        }
    }

    pub fn openai_preset() -> Self {
        Self {
            asr: AsrConfig {
                base_url: "https://api.openai.com/v1".to_string(),
                model: "whisper-1".to_string(),
                protocol: "openai-transcriptions".to_string(),
                api_key: None,
                api_key_env: "OPENAI_API_KEY".to_string(),
                language: None,
                prompt: None,
            },
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsrConfig {
    pub base_url: String,
    pub model: String,
    pub protocol: String,
    #[serde(default)]
    pub api_key: Option<String>,
    pub api_key_env: String,
    pub language: Option<String>,
    pub prompt: Option<String>,
}

impl Default for AsrConfig {
    fn default() -> Self {
        Self {
            base_url: ALIYUN_OPENAI_COMPAT_BASE_URL.to_string(),
            model: ALIYUN_ASR_MODEL.to_string(),
            protocol: "chat-audio".to_string(),
            api_key: None,
            api_key_env: OPENAI_COMPAT_API_KEY_ENV.to_string(),
            language: None,
            prompt: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostprocessConfig {
    pub mode: String,
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub api_key: Option<String>,
    pub api_key_env: String,
    pub system_prompt: String,
    pub user_template: String,
    #[serde(default = "default_postprocess_fallback_to_builtin")]
    pub fallback_to_builtin: bool,
}

impl Default for PostprocessConfig {
    fn default() -> Self {
        Self {
            mode: "builtin".to_string(),
            base_url: ALIYUN_OPENAI_COMPAT_BASE_URL.to_string(),
            model: ALIYUN_POSTPROCESS_MODEL.to_string(),
            api_key: None,
            api_key_env: OPENAI_COMPAT_API_KEY_ENV.to_string(),
            system_prompt: "You are Orally's AI postprocessor for raw speech-to-text transcripts. Produce text that is ready to paste into the user's active app. Preserve the speaker's meaning, intent, language, names, product terms, URLs, and code identifiers. Remove filler words, repeated fragments, false starts, and self-corrections unless they change the meaning. Add only punctuation and lightweight structure that are clearly implied by the transcript. Do not invent facts, explanations, headings, labels, quotes, or markdown fences. Return only the final text.".to_string(),
            user_template: "Locale: {{locale}}\nTask: cleanup\nTranscript:\n{{transcript}}\n\nClean the transcript into polished text in the original language. Keep normal prose unless the speaker explicitly asks for a list, translation, or another format.".to_string(),
            fallback_to_builtin: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputConfig {
    pub locale: String,
    pub raw: bool,
    pub show_changes: bool,
    pub insert: bool,
    pub paste_delay_ms: u64,
    #[serde(default = "default_restore_clipboard")]
    pub restore_clipboard: bool,
    #[serde(default = "default_restore_clipboard_delay_ms")]
    pub restore_clipboard_delay_ms: u64,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            locale: "zh-CN".to_string(),
            raw: false,
            show_changes: false,
            insert: false,
            paste_delay_ms: 750,
            restore_clipboard: true,
            restore_clipboard_delay_ms: 250,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioConfig {
    pub dictate_seconds: u64,
    pub record_output: String,
    #[serde(default = "default_input_mode")]
    pub input_mode: String,
    #[serde(default = "default_auto_stop_enabled")]
    pub auto_stop_enabled: bool,
    #[serde(default = "default_min_record_ms")]
    pub min_record_ms: u64,
    #[serde(default = "default_max_record_ms")]
    pub max_record_ms: u64,
    #[serde(default = "default_silence_timeout_ms")]
    pub silence_timeout_ms: u64,
    #[serde(default = "default_silence_threshold")]
    pub silence_threshold: f32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            dictate_seconds: 3,
            record_output: "orally-recording.wav".to_string(),
            input_mode: default_input_mode(),
            auto_stop_enabled: default_auto_stop_enabled(),
            min_record_ms: default_min_record_ms(),
            max_record_ms: default_max_record_ms(),
            silence_timeout_ms: default_silence_timeout_ms(),
            silence_threshold: default_silence_threshold(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub preset: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            preset: "ctrl-alt-space".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivacyConfig {
    pub allow_external_requests: bool,
    pub history_enabled: bool,
    pub history_path: Option<String>,
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            allow_external_requests: true,
            history_enabled: true,
            history_path: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderPreset {
    AliyunOpenAi,
    OpenRouter,
    OpenAi,
}

impl ProviderPreset {
    pub fn parse(value: &str) -> Result<Self, ConfigError> {
        match value {
            "aliyun-openai" | "aliyun" | "bailian" | "openai-compatible" => Ok(Self::AliyunOpenAi),
            "openrouter" => Ok(Self::OpenRouter),
            "openai" => Ok(Self::OpenAi),
            other => Err(ConfigError::InvalidPreset(other.to_string())),
        }
    }

    pub fn config(&self) -> AppConfig {
        match self {
            Self::AliyunOpenAi => AppConfig::aliyun_openai_preset(),
            Self::OpenRouter => AppConfig::openrouter_preset(),
            Self::OpenAi => AppConfig::openai_preset(),
        }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Serialize(toml::ser::Error),
    Deserialize(toml::de::Error),
    MissingConfigDir,
    InvalidPreset(String),
    InvalidKey(String),
    InvalidValue { key: String, value: String },
    AlreadyExists(PathBuf),
}

impl Display for ConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Serialize(error) => write!(f, "{error}"),
            Self::Deserialize(error) => write!(f, "{error}"),
            Self::MissingConfigDir => write!(f, "could not determine a user config directory"),
            Self::InvalidPreset(value) => write!(f, "unknown provider preset: {value}"),
            Self::InvalidKey(value) => write!(f, "unknown config key: {value}"),
            Self::InvalidValue { key, value } => {
                write!(f, "invalid value for {key}: {value}")
            }
            Self::AlreadyExists(path) => {
                write!(f, "config already exists: {}", path.display())
            }
        }
    }
}

impl Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<toml::ser::Error> for ConfigError {
    fn from(value: toml::ser::Error) -> Self {
        Self::Serialize(value)
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(value: toml::de::Error) -> Self {
        Self::Deserialize(value)
    }
}

pub fn config_path() -> Result<PathBuf, ConfigError> {
    if let Ok(path) = env::var("ORALLY_CONFIG") {
        return Ok(PathBuf::from(path));
    }

    if let Some(path) = existing_portable_config_path() {
        return Ok(path);
    }

    if let Ok(appdata) = env::var("APPDATA") {
        return Ok(PathBuf::from(appdata).join("Orally").join("config.toml"));
    }

    if let Ok(home) = env::var("HOME") {
        return Ok(PathBuf::from(home)
            .join(".config")
            .join("orally")
            .join("config.toml"));
    }

    Err(ConfigError::MissingConfigDir)
}

pub fn portable_config_path() -> Option<PathBuf> {
    env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join("config.toml")))
}

pub fn existing_portable_config_path() -> Option<PathBuf> {
    portable_config_path().filter(|path| path.exists())
}

pub fn init_portable_config(config: &AppConfig, force: bool) -> Result<PathBuf, ConfigError> {
    let path = portable_config_path().ok_or(ConfigError::MissingConfigDir)?;
    if path.exists() && !force {
        return Err(ConfigError::AlreadyExists(path));
    }

    save_to_path(path.clone(), config)?;
    Ok(path)
}

pub fn load_or_default() -> Result<AppConfig, ConfigError> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }

    load_from_path(path)
}

pub fn load_from_path(path: impl Into<PathBuf>) -> Result<AppConfig, ConfigError> {
    let text = fs::read_to_string(path.into())?;
    Ok(toml::from_str(&text)?)
}

pub fn save_to_path(path: impl Into<PathBuf>, config: &AppConfig) -> Result<(), ConfigError> {
    let path = path.into();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(path, to_toml(config)?)?;
    Ok(())
}

pub fn init_config(config: &AppConfig, force: bool) -> Result<PathBuf, ConfigError> {
    let path = config_path()?;
    if path.exists() && !force {
        return Err(ConfigError::AlreadyExists(path));
    }

    save_to_path(path.clone(), config)?;
    Ok(path)
}

pub fn to_toml(config: &AppConfig) -> Result<String, ConfigError> {
    Ok(toml::to_string_pretty(config)?)
}

pub fn set_value(config: &mut AppConfig, key: &str, value: &str) -> Result<(), ConfigError> {
    match key {
        "asr.base_url" => config.asr.base_url = value.to_string(),
        "asr.model" => config.asr.model = value.to_string(),
        "asr.protocol" => config.asr.protocol = value.to_string(),
        "asr.api_key" => config.asr.api_key = optional_string(value),
        "asr.api_key_env" => config.asr.api_key_env = value.to_string(),
        "asr.language" => config.asr.language = optional_string(value),
        "asr.prompt" => config.asr.prompt = optional_string(value),
        "postprocess.mode" => config.postprocess.mode = value.to_string(),
        "postprocess.base_url" => config.postprocess.base_url = value.to_string(),
        "postprocess.model" => config.postprocess.model = value.to_string(),
        "postprocess.api_key" => config.postprocess.api_key = optional_string(value),
        "postprocess.api_key_env" => config.postprocess.api_key_env = value.to_string(),
        "postprocess.system_prompt" => config.postprocess.system_prompt = value.to_string(),
        "postprocess.user_template" => config.postprocess.user_template = value.to_string(),
        "postprocess.fallback_to_builtin" => {
            config.postprocess.fallback_to_builtin = parse_bool(key, value)?
        }
        "output.locale" => config.output.locale = value.to_string(),
        "output.raw" => config.output.raw = parse_bool(key, value)?,
        "output.show_changes" => config.output.show_changes = parse_bool(key, value)?,
        "output.insert" => config.output.insert = parse_bool(key, value)?,
        "output.paste_delay_ms" => config.output.paste_delay_ms = parse_u64(key, value)?,
        "output.restore_clipboard" => config.output.restore_clipboard = parse_bool(key, value)?,
        "output.restore_clipboard_delay_ms" => {
            config.output.restore_clipboard_delay_ms = parse_u64(key, value)?
        }
        "audio.dictate_seconds" => config.audio.dictate_seconds = parse_u64(key, value)?,
        "audio.record_output" => config.audio.record_output = value.to_string(),
        "audio.input_mode" => config.audio.input_mode = value.to_string(),
        "audio.auto_stop_enabled" => config.audio.auto_stop_enabled = parse_bool(key, value)?,
        "audio.min_record_ms" => config.audio.min_record_ms = parse_u64(key, value)?,
        "audio.max_record_ms" => config.audio.max_record_ms = parse_u64(key, value)?,
        "audio.silence_timeout_ms" => config.audio.silence_timeout_ms = parse_u64(key, value)?,
        "audio.silence_threshold" => config.audio.silence_threshold = parse_f32(key, value)?,
        "hotkey.preset" => config.hotkey.preset = value.to_string(),
        "privacy.allow_external_requests" => {
            config.privacy.allow_external_requests = parse_bool(key, value)?
        }
        "privacy.history_enabled" => config.privacy.history_enabled = parse_bool(key, value)?,
        "privacy.history_path" => config.privacy.history_path = optional_string(value),
        other => return Err(ConfigError::InvalidKey(other.to_string())),
    }

    Ok(())
}

fn default_restore_clipboard() -> bool {
    true
}

fn default_restore_clipboard_delay_ms() -> u64 {
    250
}

fn default_postprocess_fallback_to_builtin() -> bool {
    true
}

fn default_input_mode() -> String {
    "toggle".to_string()
}

fn default_auto_stop_enabled() -> bool {
    false
}

fn default_min_record_ms() -> u64 {
    450
}

fn default_max_record_ms() -> u64 {
    120_000
}

fn default_silence_timeout_ms() -> u64 {
    1_200
}

fn default_silence_threshold() -> f32 {
    0.02
}

fn optional_string(value: &str) -> Option<String> {
    if value.eq_ignore_ascii_case("none") || value.trim().is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn parse_bool(key: &str, value: &str) -> Result<bool, ConfigError> {
    match value {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        other => Err(ConfigError::InvalidValue {
            key: key.to_string(),
            value: other.to_string(),
        }),
    }
}

fn parse_u64(key: &str, value: &str) -> Result<u64, ConfigError> {
    value.parse::<u64>().map_err(|_| ConfigError::InvalidValue {
        key: key.to_string(),
        value: value.to_string(),
    })
}

fn parse_f32(key: &str, value: &str) -> Result<f32, ConfigError> {
    value.parse::<f32>().map_err(|_| ConfigError::InvalidValue {
        key: key.to_string(),
        value: value.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliyun_openai_preset_uses_openai_compatible_defaults() {
        let config = AppConfig::aliyun_openai_preset();

        assert_eq!(config.asr.base_url, ALIYUN_OPENAI_COMPAT_BASE_URL);
        assert_eq!(config.asr.api_key_env, OPENAI_COMPAT_API_KEY_ENV);
        assert_eq!(config.asr.protocol, "chat-audio");
        assert_eq!(config.postprocess.mode, "llm");
        assert_eq!(config.postprocess.model, ALIYUN_POSTPROCESS_MODEL);
        assert_eq!(config.postprocess.api_key_env, OPENAI_COMPAT_API_KEY_ENV);
    }

    #[test]
    fn config_round_trips_through_toml() {
        let config = AppConfig::openrouter_preset();
        let text = to_toml(&config).expect("config should serialize");
        let parsed: AppConfig = toml::from_str(&text).expect("config should deserialize");

        assert_eq!(parsed, config);
    }

    #[test]
    fn automatic_stop_is_disabled_by_default() {
        let config = AppConfig::default();

        assert_eq!(config.audio.input_mode, "toggle");
        assert!(!config.audio.auto_stop_enabled);
    }

    #[test]
    fn set_value_updates_nested_fields() {
        let mut config = AppConfig::default();

        set_value(&mut config, "asr.model", "qwen3-asr-flash").expect("model should update");
        set_value(&mut config, "output.insert", "true").expect("insert should update");
        set_value(&mut config, "asr.language", "none").expect("language should clear");
        set_value(&mut config, "audio.input_mode", "hold").expect("mode should update");
        set_value(&mut config, "audio.silence_threshold", "0.015")
            .expect("threshold should update");
        set_value(&mut config, "postprocess.fallback_to_builtin", "false")
            .expect("fallback should update");

        assert_eq!(config.asr.model, "qwen3-asr-flash");
        assert!(config.output.insert);
        assert_eq!(config.asr.language, None);
        assert_eq!(config.audio.input_mode, "hold");
        assert_eq!(config.audio.silence_threshold, 0.015);
        assert!(!config.postprocess.fallback_to_builtin);
    }

    #[test]
    fn portable_config_path_uses_current_exe_dir() {
        let path = portable_config_path().expect("current exe path should be available");

        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("config.toml")
        );
    }
}
