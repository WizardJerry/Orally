use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    pub asr: AsrConfig,
    pub output: OutputConfig,
    pub audio: AudioConfig,
    pub hotkey: HotkeyConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            asr: AsrConfig::default(),
            output: OutputConfig::default(),
            audio: AudioConfig::default(),
            hotkey: HotkeyConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn dashscope_preset() -> Self {
        Self {
            asr: AsrConfig {
                base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
                model: "qwen3-asr-flash".to_string(),
                protocol: "chat-audio".to_string(),
                api_key: None,
                api_key_env: "DASHSCOPE_API_KEY".to_string(),
                language: None,
                prompt: None,
            },
            output: OutputConfig {
                locale: "zh-CN".to_string(),
                raw: false,
                show_changes: false,
                insert: false,
                paste_delay_ms: 300,
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
            base_url: "https://api.openai.com/v1".to_string(),
            model: String::new(),
            protocol: "auto".to_string(),
            api_key: None,
            api_key_env: "ORALLY_ASR_API_KEY".to_string(),
            language: None,
            prompt: None,
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
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            locale: "zh-CN".to_string(),
            raw: false,
            show_changes: false,
            insert: false,
            paste_delay_ms: 750,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioConfig {
    pub dictate_seconds: u64,
    pub record_output: String,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            dictate_seconds: 3,
            record_output: "orally-recording.wav".to_string(),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderPreset {
    DashScope,
    OpenRouter,
    OpenAi,
}

impl ProviderPreset {
    pub fn parse(value: &str) -> Result<Self, ConfigError> {
        match value {
            "dashscope" | "aliyun" | "bailian" => Ok(Self::DashScope),
            "openrouter" => Ok(Self::OpenRouter),
            "openai" => Ok(Self::OpenAi),
            other => Err(ConfigError::InvalidPreset(other.to_string())),
        }
    }

    pub fn config(&self) -> AppConfig {
        match self {
            Self::DashScope => AppConfig::dashscope_preset(),
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
        "output.locale" => config.output.locale = value.to_string(),
        "output.raw" => config.output.raw = parse_bool(key, value)?,
        "output.show_changes" => config.output.show_changes = parse_bool(key, value)?,
        "output.insert" => config.output.insert = parse_bool(key, value)?,
        "output.paste_delay_ms" => config.output.paste_delay_ms = parse_u64(key, value)?,
        "audio.dictate_seconds" => config.audio.dictate_seconds = parse_u64(key, value)?,
        "audio.record_output" => config.audio.record_output = value.to_string(),
        "hotkey.preset" => config.hotkey.preset = value.to_string(),
        other => return Err(ConfigError::InvalidKey(other.to_string())),
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashscope_preset_uses_dashscope_env_name() {
        let config = AppConfig::dashscope_preset();

        assert_eq!(config.asr.api_key_env, "DASHSCOPE_API_KEY");
        assert_eq!(config.asr.protocol, "chat-audio");
    }

    #[test]
    fn config_round_trips_through_toml() {
        let config = AppConfig::openrouter_preset();
        let text = to_toml(&config).expect("config should serialize");
        let parsed: AppConfig = toml::from_str(&text).expect("config should deserialize");

        assert_eq!(parsed, config);
    }

    #[test]
    fn set_value_updates_nested_fields() {
        let mut config = AppConfig::default();

        set_value(&mut config, "asr.model", "qwen3-asr-flash").expect("model should update");
        set_value(&mut config, "output.insert", "true").expect("insert should update");
        set_value(&mut config, "asr.language", "none").expect("language should clear");

        assert_eq!(config.asr.model, "qwen3-asr-flash");
        assert!(config.output.insert);
        assert_eq!(config.asr.language, None);
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
