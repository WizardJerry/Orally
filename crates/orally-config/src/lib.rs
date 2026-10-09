use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const OPENAI_COMPAT_BASE_URL: &str = "https://api.openai.com/v1";
pub const OPENAI_COMPAT_API_KEY_ENV: &str = "ORALLY_OPENAI_COMPAT_API_KEY";
pub const OPENAI_COMPAT_ASR_MODEL: &str = "whisper-1";
pub const OPENAI_COMPAT_ASR_PROTOCOL: &str = "auto";
pub const OPENAI_COMPAT_POSTPROCESS_MODEL: &str = "gpt-4o-mini";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub profiles: Vec<ConfigProfile>,
    #[serde(default)]
    pub active_profile_id: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ConfigProfile {
    pub id: String,
    pub name: String,
    pub asr: AsrConfig,
    pub postprocess: PostprocessConfig,
}

impl AppConfig {
    pub fn openai_compatible_preset() -> Self {
        Self {
            postprocess: PostprocessConfig {
                mode: "llm".to_string(),
                ..PostprocessConfig::default()
            },
            output: OutputConfig {
                paste_delay_ms: 300,
                ..OutputConfig::default()
            },
            ..Self::default()
        }
    }

    pub fn openai_preset() -> Self {
        let mut config = Self::openai_compatible_preset();
        config.asr.api_key_env = "OPENAI_API_KEY".to_string();
        config.postprocess.api_key_env = "OPENAI_API_KEY".to_string();
        config
    }

    fn normalize_asr_protocols(&mut self) {
        // Persisted profiles negotiate compatible request shapes; CLI overrides are per run.
        self.asr.protocol = OPENAI_COMPAT_ASR_PROTOCOL.to_string();
        for profile in &mut self.profiles {
            profile.asr.protocol = OPENAI_COMPAT_ASR_PROTOCOL.to_string();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsrConfig {
    pub base_url: String,
    pub model: String,
    #[serde(default = "default_asr_protocol")]
    pub protocol: String,
    #[serde(default)]
    pub api_key: Option<String>,
    pub api_key_env: String,
    pub language: Option<String>,
}

impl Default for AsrConfig {
    fn default() -> Self {
        Self {
            base_url: OPENAI_COMPAT_BASE_URL.to_string(),
            model: OPENAI_COMPAT_ASR_MODEL.to_string(),
            protocol: default_asr_protocol(),
            api_key: None,
            api_key_env: OPENAI_COMPAT_API_KEY_ENV.to_string(),
            language: None,
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
    #[serde(default)]
    pub models: Vec<PostprocessModelConfig>,
}

impl Default for PostprocessConfig {
    fn default() -> Self {
        Self {
            mode: "builtin".to_string(),
            base_url: OPENAI_COMPAT_BASE_URL.to_string(),
            model: OPENAI_COMPAT_POSTPROCESS_MODEL.to_string(),
            api_key: None,
            api_key_env: OPENAI_COMPAT_API_KEY_ENV.to_string(),
            system_prompt: "You are Orally's AI postprocessor for raw speech-to-text transcripts. Produce text that is ready to paste into the user's active app. Preserve the speaker's meaning, intent, language, names, product terms, URLs, and code identifiers. Remove filler words, repeated fragments, false starts, and self-corrections unless they change the meaning. Add only punctuation and lightweight structure that are clearly implied by the transcript. Do not invent facts, explanations, headings, labels, quotes, or markdown fences. Return only the final text.".to_string(),
            user_template: "Locale: {{locale}}\nTask: cleanup\nTranscript:\n{{transcript}}\n\nClean the transcript into polished text in the original language. Keep normal prose unless the speaker explicitly asks for a list, translation, or another format.".to_string(),
            fallback_to_builtin: true,
            models: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PostprocessModelConfig {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub api_key_env: String,
    pub system_prompt: String,
    pub user_template: String,
    pub fallback_to_builtin: bool,
    pub prompts: Vec<PromptNodeConfig>,
}

impl Default for PostprocessModelConfig {
    fn default() -> Self {
        let config = PostprocessConfig::default();
        Self {
            id: String::new(),
            name: String::new(),
            enabled: true,
            base_url: config.base_url,
            model: config.model,
            api_key: config.api_key,
            api_key_env: config.api_key_env,
            system_prompt: config.system_prompt,
            user_template: config.user_template,
            fallback_to_builtin: config.fallback_to_builtin,
            prompts: Vec::new(),
        }
    }
}

impl PostprocessModelConfig {
    pub fn composed_system_prompt(&self) -> String {
        std::iter::once(self.system_prompt.as_str())
            .chain(
                self.prompts
                    .iter()
                    .filter(|prompt| prompt.enabled)
                    .map(|prompt| prompt.content.as_str()),
            )
            .filter(|content| !content.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PromptNodeConfig {
    pub id: String,
    pub name: String,
    pub content: String,
    pub enabled: bool,
}

impl Default for PromptNodeConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            content: String::new(),
            enabled: true,
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
    OpenAiCompatible,
    OpenAi,
}

impl ProviderPreset {
    pub fn parse(value: &str) -> Result<Self, ConfigError> {
        match value {
            "openai-compatible" => Ok(Self::OpenAiCompatible),
            "openai" => Ok(Self::OpenAi),
            other => Err(ConfigError::InvalidPreset(other.to_string())),
        }
    }

    pub fn config(&self) -> AppConfig {
        match self {
            Self::OpenAiCompatible => AppConfig::openai_compatible_preset(),
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
    InvalidValue {
        key: String,
        value: String,
    },
    AlreadyExists(PathBuf),
    Migration {
        source: PathBuf,
        destination: PathBuf,
        error: Box<ConfigError>,
    },
}

impl Display for ConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Serialize(error) => write!(f, "{error}"),
            Self::Deserialize(error) => {
                // TOML error excerpts can include plaintext credentials.
                if let Some(span) = error.span() {
                    write!(f, "invalid TOML configuration near byte {}", span.start)
                } else {
                    write!(f, "invalid TOML configuration")
                }
            }
            Self::MissingConfigDir => write!(f, "could not determine the executable directory"),
            Self::InvalidPreset(value) => write!(f, "unknown provider preset: {value}"),
            Self::InvalidKey(value) => write!(f, "unknown config key: {value}"),
            Self::InvalidValue { key, value } => {
                write!(f, "invalid value for {key}: {value}")
            }
            Self::AlreadyExists(path) => {
                write!(f, "config already exists: {}", path.display())
            }
            Self::Migration {
                source,
                destination,
                error,
            } => write!(
                f,
                "could not migrate configuration from {} to {}: {error}; the original file was retained",
                source.display(),
                destination.display()
            ),
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
    let executable = env::current_exe()?;
    let legacy_paths = legacy_config_paths(
        env::var_os("ORALLY_CONFIG"),
        env::var_os("APPDATA"),
        env::var_os("HOME"),
    );
    config_path_for(&executable, &legacy_paths)
}

fn beside_executable(executable: &Path) -> Option<PathBuf> {
    executable
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(|parent| parent.join("config.toml"))
}

fn legacy_config_paths(
    override_path: Option<std::ffi::OsString>,
    appdata: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Vec<PathBuf> {
    let nonempty = |value: Option<std::ffi::OsString>| value.filter(|value| !value.is_empty());
    let mut paths = Vec::new();
    if let Some(path) = nonempty(override_path) {
        paths.push(PathBuf::from(path));
    }
    if let Some(path) = nonempty(appdata) {
        paths.push(PathBuf::from(path).join("Orally").join("config.toml"));
    }
    if let Some(path) = nonempty(home) {
        paths.push(
            PathBuf::from(path)
                .join(".config")
                .join("orally")
                .join("config.toml"),
        );
    }
    paths
}

fn path_is_present(path: &Path) -> Result<bool, std::io::Error> {
    // Treat a broken symlink as an existing file, rather than replacing it or
    // silently returning defaults when its target cannot be read.
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn config_path_for(executable: &Path, legacy_paths: &[PathBuf]) -> Result<PathBuf, ConfigError> {
    let destination = beside_executable(executable).ok_or(ConfigError::MissingConfigDir)?;
    if path_is_present(&destination)? {
        return Ok(destination);
    }

    for source in legacy_paths {
        let migrate = || -> Result<bool, ConfigError> {
            if !path_is_present(source)? {
                return Ok(false);
            }
            let text = fs::read_to_string(source)?;
            from_toml(&text)?;
            // Preserve the source bytes, including keys, comments and fields
            // not yet represented by AppConfig. Never delete the old file.
            create_new_config(&destination, text.as_bytes())?;
            Ok(true)
        };
        match migrate() {
            Ok(true) => break,
            Ok(false) => continue,
            Err(error) => {
                return Err(ConfigError::Migration {
                    source: source.clone(),
                    destination,
                    error: Box::new(error),
                })
            }
        }
    }
    Ok(destination)
}

fn create_new_config(destination: &Path, contents: &[u8]) -> Result<bool, ConfigError> {
    let mut file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
    {
        Ok(file) => file,
        // A concurrent initialization or migration must win without being
        // overwritten. Its contents will be checked by the normal loader.
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let result = file.write_all(contents).and_then(|_| file.sync_all());
    drop(file);
    if let Err(error) = result {
        // Only this call created the destination; don't leave a partial copy
        // that could hide the intact legacy configuration on the next load.
        fs::remove_file(destination)?;
        return Err(error.into());
    }
    Ok(true)
}

/// Compatibility alias for the configuration beside the executable.
pub fn portable_config_path() -> Option<PathBuf> {
    env::current_exe()
        .ok()
        .and_then(|path| beside_executable(&path))
}

pub fn existing_portable_config_path() -> Option<PathBuf> {
    portable_config_path().filter(|path| path.exists())
}

pub fn init_portable_config(config: &AppConfig, force: bool) -> Result<PathBuf, ConfigError> {
    init_config(config, force)
}

pub fn load_or_default() -> Result<AppConfig, ConfigError> {
    let path = config_path()?;
    if !path_is_present(&path)? {
        return Ok(AppConfig::default());
    }

    load_from_path(path)
}

pub fn load_from_path(path: impl Into<PathBuf>) -> Result<AppConfig, ConfigError> {
    let text = fs::read_to_string(path.into())?;
    from_toml(&text)
}

pub fn from_toml(text: &str) -> Result<AppConfig, ConfigError> {
    let mut config: AppConfig = toml::from_str(text)?;
    config.normalize_asr_protocols();
    Ok(config)
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
    init_config_at(path, config, force)
}

fn init_config_at(path: PathBuf, config: &AppConfig, force: bool) -> Result<PathBuf, ConfigError> {
    if force {
        save_to_path(path.clone(), config)?;
    } else if !create_new_config(&path, to_toml(config)?.as_bytes())? {
        return Err(ConfigError::AlreadyExists(path));
    }
    Ok(path)
}

pub fn to_toml(config: &AppConfig) -> Result<String, ConfigError> {
    let mut config = config.clone();
    config.normalize_asr_protocols();
    Ok(toml::to_string_pretty(&config)?)
}

pub fn set_value(config: &mut AppConfig, key: &str, value: &str) -> Result<(), ConfigError> {
    match key {
        "asr.base_url" => config.asr.base_url = value.to_string(),
        "asr.model" => config.asr.model = value.to_string(),
        "asr.protocol" => config.asr.protocol = default_asr_protocol(),
        "asr.api_key" => config.asr.api_key = optional_string(value),
        "asr.api_key_env" => config.asr.api_key_env = value.to_string(),
        "asr.language" => config.asr.language = optional_string(value),
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

    if key.starts_with("postprocess.") {
        let postprocess = &mut config.postprocess;
        if let Some(model) = postprocess.models.first_mut() {
            match key {
                "postprocess.base_url" => model.base_url = postprocess.base_url.clone(),
                "postprocess.model" => model.model = postprocess.model.clone(),
                "postprocess.api_key" => model.api_key = postprocess.api_key.clone(),
                "postprocess.api_key_env" => model.api_key_env = postprocess.api_key_env.clone(),
                "postprocess.system_prompt" => {
                    model.system_prompt = postprocess.system_prompt.clone()
                }
                "postprocess.user_template" => {
                    model.user_template = postprocess.user_template.clone()
                }
                "postprocess.fallback_to_builtin" => {
                    model.fallback_to_builtin = postprocess.fallback_to_builtin
                }
                _ => {}
            }
        }
    }

    if let Some(active_id) = config.active_profile_id.as_deref() {
        if let Some(profile) = config
            .profiles
            .iter_mut()
            .find(|profile| profile.id == active_id)
        {
            if key.starts_with("asr.") {
                profile.asr = config.asr.clone();
            } else if key.starts_with("postprocess.") {
                profile.postprocess = config.postprocess.clone();
            }
        }
    }

    Ok(())
}

fn default_asr_protocol() -> String {
    OPENAI_COMPAT_ASR_PROTOCOL.to_string()
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
    fn openai_compatible_preset_uses_shared_defaults() {
        let config = AppConfig::openai_compatible_preset();

        assert_eq!(config.asr.base_url, OPENAI_COMPAT_BASE_URL);
        assert_eq!(config.asr.model, OPENAI_COMPAT_ASR_MODEL);
        assert_eq!(config.asr.api_key_env, OPENAI_COMPAT_API_KEY_ENV);
        assert_eq!(config.asr.protocol, OPENAI_COMPAT_ASR_PROTOCOL);
        assert_eq!(config.postprocess.mode, "llm");
        assert_eq!(config.postprocess.base_url, OPENAI_COMPAT_BASE_URL);
        assert_eq!(config.postprocess.model, OPENAI_COMPAT_POSTPROCESS_MODEL);
        assert_eq!(config.postprocess.api_key_env, OPENAI_COMPAT_API_KEY_ENV);
    }

    #[test]
    fn automatic_protocol_is_the_persisted_default_without_changing_service_settings() {
        assert_eq!(AppConfig::default().asr.protocol, "auto");
        let mut config = AppConfig::default();
        config.asr = AsrConfig {
            base_url: "https://custom-asr.example/v1".to_string(),
            model: "existing-asr-model".to_string(),
            protocol: "openai-transcriptions".to_string(),
            api_key: Some("test-asr-credential".to_string()),
            api_key_env: "CUSTOM_ASR_KEY".to_string(),
            language: Some("zh".to_string()),
        };
        config.profiles = vec![ConfigProfile {
            id: "active".to_string(),
            asr: config.asr.clone(),
            ..ConfigProfile::default()
        }];
        config.active_profile_id = Some("active".to_string());
        let original = config.clone();

        let loaded = from_toml(&toml::to_string_pretty(&config).unwrap()).unwrap();
        let saved: AppConfig = toml::from_str(&to_toml(&config).unwrap()).unwrap();

        assert_eq!(config, original);
        config.asr.protocol = "auto".to_string();
        config.profiles[0].asr.protocol = "auto".to_string();
        assert_eq!(loaded, config);
        assert_eq!(saved, config);
    }

    #[test]
    fn provider_presets_share_endpoints_and_models_with_consistent_credentials() {
        let compatible = ProviderPreset::parse("openai-compatible").unwrap().config();
        let openai = ProviderPreset::parse("openai").unwrap().config();

        assert_eq!(compatible, AppConfig::openai_compatible_preset());
        assert_eq!(openai.asr.base_url, compatible.asr.base_url);
        assert_eq!(openai.asr.model, compatible.asr.model);
        assert_eq!(openai.asr.protocol, compatible.asr.protocol);
        assert_eq!(openai.postprocess.base_url, compatible.postprocess.base_url);
        assert_eq!(openai.postprocess.model, compatible.postprocess.model);
        assert_eq!(openai.asr.api_key_env, "OPENAI_API_KEY");
        assert_eq!(openai.postprocess.api_key_env, "OPENAI_API_KEY");
        assert!(ProviderPreset::parse("unsupported-provider").is_err());
    }

    #[test]
    fn config_round_trips_through_toml() {
        let config = AppConfig::openai_compatible_preset();
        let text = to_toml(&config).expect("config should serialize");
        let parsed = from_toml(&text).expect("config should deserialize");

        assert_eq!(parsed, config);
    }

    #[test]
    fn legacy_config_without_profiles_or_models_loads() {
        let text = r#"
            [asr]
            base_url = "https://asr.example/v1"
            model = "legacy-asr"
            protocol = "openai-transcriptions"
            api_key_env = "LEGACY_ASR_KEY"
            language = "en"
            prompt = "Recognize product names."

            [postprocess]
            mode = "llm"
            base_url = "https://llm.example/v1"
            model = "legacy-cleanup"
            api_key_env = "LEGACY_LLM_KEY"
            system_prompt = "Clean the transcript."
            user_template = "{{transcript}}"
        "#;

        let config = from_toml(text).expect("legacy config should deserialize");

        assert!(config.profiles.is_empty());
        assert_eq!(config.active_profile_id, None);
        assert!(config.postprocess.models.is_empty());
        assert_eq!(config.asr.model, "legacy-asr");
        assert_eq!(config.postprocess.model, "legacy-cleanup");
        assert!(config.postprocess.fallback_to_builtin);
    }

    #[test]
    fn legacy_asr_prompts_are_ignored_for_top_level_and_profile_configs() {
        let mut config = AppConfig::default();
        config.asr.model = "existing-asr".to_string();
        config.postprocess.system_prompt = "Preserve this LLM instruction.".to_string();
        config.profiles.push(ConfigProfile {
            id: "existing".to_string(),
            name: "Existing profile".to_string(),
            asr: config.asr.clone(),
            postprocess: config.postprocess.clone(),
        });
        config.active_profile_id = Some("existing".to_string());
        let legacy = to_toml(&config)
            .unwrap()
            .replace("[asr]\n", "[asr]\nprompt = 'Retired top-level ASR hint'\n")
            .replace(
                "[profiles.asr]\n",
                "[profiles.asr]\nprompt = 'Retired profile ASR hint'\n",
            );
        assert!(legacy.contains("Retired top-level ASR hint"));
        assert!(legacy.contains("Retired profile ASR hint"));

        let loaded = from_toml(&legacy).expect("retired ASR fields must not prevent loading");
        assert_eq!(loaded, config);
        let saved: toml::Value = toml::from_str(&to_toml(&loaded).unwrap()).unwrap();
        assert!(saved["asr"].get("prompt").is_none());
        assert!(saved["profiles"][0]["asr"].get("prompt").is_none());
        assert_eq!(
            saved["postprocess"]["system_prompt"].as_str(),
            Some("Preserve this LLM instruction.")
        );
    }

    #[test]
    fn loading_legacy_protocols_preserves_service_settings_and_profile_selection() {
        for protocol in ["auto", "chat-audio", "openai-transcriptions"] {
            let mut config = AppConfig::default();
            config.asr = AsrConfig {
                base_url: "https://custom-asr.example/v1".to_string(),
                model: "existing-asr-model".to_string(),
                protocol: protocol.to_string(),
                api_key: Some("test-asr-credential".to_string()),
                api_key_env: "CUSTOM_ASR_KEY".to_string(),
                language: Some("zh".to_string()),
            };
            config.profiles = vec![ConfigProfile {
                id: "active".to_string(),
                name: "Existing profile".to_string(),
                asr: config.asr.clone(),
                postprocess: config.postprocess.clone(),
            }];
            config.active_profile_id = Some("active".to_string());
            let legacy_text = toml::to_string_pretty(&config).unwrap();

            let loaded = from_toml(&legacy_text).unwrap();

            config.normalize_asr_protocols();
            assert_eq!(loaded, config);
            assert_eq!(loaded.profiles[0].asr, loaded.asr);
        }
    }

    #[test]
    fn serialization_normalizes_all_profiles_without_mutating_the_draft() {
        let mut config = AppConfig::default();
        config.asr.protocol = "auto".to_string();
        config.profiles = vec![
            ConfigProfile {
                id: "active".to_string(),
                asr: AsrConfig {
                    protocol: "chat-audio".to_string(),
                    ..AsrConfig::default()
                },
                ..ConfigProfile::default()
            },
            ConfigProfile {
                id: "inactive".to_string(),
                asr: config.asr.clone(),
                ..ConfigProfile::default()
            },
        ];
        config.active_profile_id = Some("active".to_string());
        let original = config.clone();

        let text = to_toml(&config).unwrap();
        let saved: AppConfig = toml::from_str(&text).unwrap();

        assert_eq!(config, original);
        config.normalize_asr_protocols();
        assert_eq!(saved, config);
    }

    #[test]
    fn missing_protocol_defaults_to_automatic_negotiation() {
        let text = r#"
            [asr]
            base_url = "https://custom-asr.example/v1"
            model = "existing-asr-model"
            api_key_env = "CUSTOM_ASR_KEY"
        "#;

        let config = from_toml(text).unwrap();

        assert_eq!(config.asr.protocol, OPENAI_COMPAT_ASR_PROTOCOL);
        assert_eq!(config.asr.model, "existing-asr-model");
    }

    #[test]
    fn setting_legacy_protocol_keeps_the_active_profile_on_automatic_negotiation() {
        let mut config = AppConfig::default();
        config.profiles = vec![ConfigProfile {
            id: "active".to_string(),
            asr: config.asr.clone(),
            ..ConfigProfile::default()
        }];
        config.active_profile_id = Some("active".to_string());

        set_value(&mut config, "asr.protocol", "chat-audio").unwrap();

        assert_eq!(config.asr.protocol, OPENAI_COMPAT_ASR_PROTOCOL);
        assert_eq!(config.profiles[0].asr, config.asr);
    }

    #[test]
    fn profiles_models_and_prompts_round_trip_through_toml() {
        let model = PostprocessModelConfig {
            id: "cleanup".to_string(),
            name: "语音整理".to_string(),
            enabled: true,
            base_url: "https://llm.example/v1".to_string(),
            model: "cleanup-model".to_string(),
            api_key: Some("test-model-credential".to_string()),
            api_key_env: "TEST_LLM_KEY".to_string(),
            system_prompt: "Preserve meaning.".to_string(),
            user_template: "Locale: {{locale}}\n{{transcript}}".to_string(),
            fallback_to_builtin: false,
            prompts: vec![
                PromptNodeConfig {
                    id: "punctuation".to_string(),
                    name: "标点".to_string(),
                    content: "Add punctuation.\nPreserve URLs.".to_string(),
                    enabled: true,
                },
                PromptNodeConfig {
                    id: "translation".to_string(),
                    name: "翻译".to_string(),
                    content: "Translate to English.".to_string(),
                    enabled: false,
                },
            ],
        };
        let mut config = AppConfig::openai_compatible_preset();
        config.asr.api_key = Some("test-asr-credential".to_string());
        config.asr.language = Some("zh".to_string());
        config.postprocess.models = vec![
            model.clone(),
            PostprocessModelConfig {
                id: "polish".to_string(),
                name: "润色".to_string(),
                enabled: false,
                ..model
            },
        ];
        config.profiles = vec![
            ConfigProfile {
                id: "work".to_string(),
                name: "工作".to_string(),
                asr: config.asr.clone(),
                postprocess: config.postprocess.clone(),
            },
            ConfigProfile {
                id: "personal".to_string(),
                name: "个人".to_string(),
                ..ConfigProfile::default()
            },
        ];
        config.active_profile_id = Some("work".to_string());

        let text = to_toml(&config).expect("profiles should serialize");
        let parsed = from_toml(&text).expect("profiles should deserialize");

        assert_eq!(parsed, config);
    }

    #[test]
    fn new_models_and_prompts_default_to_enabled() {
        let model: PostprocessModelConfig =
            toml::from_str("id = 'cleanup'").expect("partial model should deserialize");
        let prompt: PromptNodeConfig =
            toml::from_str("content = 'Preserve names.'").expect("prompt should deserialize");

        assert!(model.enabled);
        assert!(model.fallback_to_builtin);
        assert_eq!(model.model, OPENAI_COMPAT_POSTPROCESS_MODEL);
        assert!(model.prompts.is_empty());
        assert!(prompt.enabled);
    }

    #[test]
    fn composed_prompt_preserves_order_and_skips_disabled_or_empty_prompts() {
        let model = PostprocessModelConfig {
            system_prompt: "Preserve meaning.".to_string(),
            prompts: vec![
                PromptNodeConfig {
                    content: "Add punctuation.".to_string(),
                    ..PromptNodeConfig::default()
                },
                PromptNodeConfig {
                    content: "Translate to English.".to_string(),
                    enabled: false,
                    ..PromptNodeConfig::default()
                },
                PromptNodeConfig {
                    content: " \n\t ".to_string(),
                    ..PromptNodeConfig::default()
                },
                PromptNodeConfig {
                    content: "Preserve product names.".to_string(),
                    ..PromptNodeConfig::default()
                },
            ],
            ..PostprocessModelConfig::default()
        };

        assert_eq!(
            model.composed_system_prompt(),
            "Preserve meaning.\n\nAdd punctuation.\n\nPreserve product names."
        );
    }

    #[test]
    fn composed_prompt_does_not_add_a_separator_for_an_empty_base_prompt() {
        let model = PostprocessModelConfig {
            system_prompt: String::new(),
            prompts: vec![PromptNodeConfig {
                content: "Preserve meaning.".to_string(),
                ..PromptNodeConfig::default()
            }],
            ..PostprocessModelConfig::default()
        };

        assert_eq!(model.composed_system_prompt(), "Preserve meaning.");
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

        set_value(&mut config, "asr.model", "test-asr-model").expect("model should update");
        set_value(&mut config, "output.insert", "true").expect("insert should update");
        set_value(&mut config, "asr.language", "none").expect("language should clear");
        set_value(&mut config, "audio.input_mode", "hold").expect("mode should update");
        set_value(&mut config, "audio.silence_threshold", "0.015")
            .expect("threshold should update");
        set_value(&mut config, "postprocess.fallback_to_builtin", "false")
            .expect("fallback should update");

        assert_eq!(config.asr.model, "test-asr-model");
        assert!(config.output.insert);
        assert_eq!(config.asr.language, None);
        assert_eq!(config.audio.input_mode, "hold");
        assert_eq!(config.audio.silence_threshold, 0.015);
        assert!(!config.postprocess.fallback_to_builtin);
    }

    #[test]
    fn set_value_keeps_active_profile_and_primary_model_in_sync() {
        let mut config = AppConfig::default();
        config.postprocess.models = vec![
            PostprocessModelConfig {
                id: "primary".to_string(),
                enabled: false,
                prompts: vec![PromptNodeConfig {
                    content: "Preserve names.".to_string(),
                    ..PromptNodeConfig::default()
                }],
                ..PostprocessModelConfig::default()
            },
            PostprocessModelConfig {
                id: "secondary".to_string(),
                model: "secondary-model".to_string(),
                ..PostprocessModelConfig::default()
            },
        ];
        config.profiles = vec![
            ConfigProfile {
                id: "inactive".to_string(),
                ..ConfigProfile::default()
            },
            ConfigProfile {
                id: "active".to_string(),
                asr: config.asr.clone(),
                postprocess: config.postprocess.clone(),
                ..ConfigProfile::default()
            },
        ];
        config.active_profile_id = Some("active".to_string());
        let inactive = config.profiles[0].clone();
        let secondary = config.postprocess.models[1].clone();
        let prompts = config.postprocess.models[0].prompts.clone();

        set_value(&mut config, "asr.model", "new-asr").unwrap();
        for (key, value) in [
            ("postprocess.base_url", "https://new.example/v1"),
            ("postprocess.model", "new-primary"),
            ("postprocess.api_key", "new-key"),
            ("postprocess.api_key_env", "NEW_KEY_ENV"),
            ("postprocess.system_prompt", "New instructions."),
            ("postprocess.user_template", "New: {{transcript}}"),
            ("postprocess.fallback_to_builtin", "false"),
        ] {
            set_value(&mut config, key, value).unwrap();
        }

        let primary = &config.postprocess.models[0];
        assert_eq!(config.profiles[1].asr, config.asr);
        assert_eq!(config.profiles[1].asr.model, "new-asr");
        assert_eq!(config.profiles[1].postprocess, config.postprocess);
        assert_eq!(primary.base_url, config.postprocess.base_url);
        assert_eq!(primary.model, config.postprocess.model);
        assert_eq!(primary.api_key, config.postprocess.api_key);
        assert_eq!(primary.api_key_env, config.postprocess.api_key_env);
        assert_eq!(primary.system_prompt, config.postprocess.system_prompt);
        assert_eq!(primary.user_template, config.postprocess.user_template);
        assert_eq!(
            primary.fallback_to_builtin,
            config.postprocess.fallback_to_builtin
        );
        assert!(!primary.enabled);
        assert_eq!(primary.prompts, prompts);
        assert_eq!(config.postprocess.models[1], secondary);
        assert_eq!(config.profiles[0], inactive);

        set_value(&mut config, "postprocess.api_key", "none").unwrap();
        assert_eq!(config.postprocess.models[0].api_key, None);
        assert_eq!(config.profiles[1].postprocess, config.postprocess);
    }

    #[test]
    fn set_value_mode_does_not_change_model_nodes() {
        let mut config = AppConfig::default();
        config.postprocess.models = vec![PostprocessModelConfig {
            enabled: false,
            ..PostprocessModelConfig::default()
        }];
        config.profiles = vec![ConfigProfile {
            id: "active".to_string(),
            asr: config.asr.clone(),
            postprocess: config.postprocess.clone(),
            ..ConfigProfile::default()
        }];
        config.active_profile_id = Some("active".to_string());
        let models = config.postprocess.models.clone();

        set_value(&mut config, "postprocess.mode", "llm").unwrap();

        assert_eq!(config.postprocess.mode, "llm");
        assert_eq!(config.postprocess.models, models);
        assert_eq!(config.profiles[0].postprocess, config.postprocess);
    }

    #[test]
    fn set_value_does_not_update_profiles_without_a_matching_active_id() {
        let mut config = AppConfig {
            profiles: vec![ConfigProfile {
                id: "profile".to_string(),
                ..ConfigProfile::default()
            }],
            ..AppConfig::default()
        };
        let profiles = config.profiles.clone();

        for active_id in [None, Some("missing".to_string())] {
            config.active_profile_id = active_id;
            set_value(&mut config, "asr.model", "new-asr").unwrap();
            set_value(&mut config, "postprocess.model", "new-llm").unwrap();
            assert_eq!(config.profiles, profiles);
        }
    }

    #[test]
    fn set_value_global_fields_does_not_change_profiles_or_model_nodes() {
        let mut config = AppConfig::default();
        config.postprocess.models = vec![PostprocessModelConfig::default()];
        config.profiles = vec![ConfigProfile {
            id: "active".to_string(),
            ..ConfigProfile::default()
        }];
        config.active_profile_id = Some("active".to_string());
        let profiles = config.profiles.clone();
        let postprocess = config.postprocess.clone();

        set_value(&mut config, "output.locale", "en-US").unwrap();

        assert_eq!(config.output.locale, "en-US");
        assert_eq!(config.profiles, profiles);
        assert_eq!(config.postprocess, postprocess);
    }

    #[test]
    fn portable_config_path_uses_current_exe_dir() {
        let path = portable_config_path().expect("current exe path should be available");

        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("config.toml")
        );
    }

    struct ConfigFixture(PathBuf);

    impl ConfigFixture {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = env::temp_dir().join(format!(
                "orally-config-test-{}-{unique}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            fs::create_dir(path.join("app")).unwrap();
            Self(path)
        }

        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }

        fn write(&self, name: &str, text: &str) -> PathBuf {
            let path = self.path(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, text).unwrap();
            path
        }

        fn executable(&self) -> PathBuf {
            self.path("app/Orally.exe")
        }

        fn write_model(&self, name: &str, model: &str) -> PathBuf {
            let mut config = AppConfig::default();
            config.asr.model = model.to_string();
            self.write(name, &to_toml(&config).unwrap())
        }
    }

    impl Drop for ConfigFixture {
        fn drop(&mut self) {
            // Only the unique fixture directory created above is removed.
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn configuration_destination_is_always_beside_executable() {
        let fixture = ConfigFixture::new();
        let override_path = fixture.path("override/config.toml");
        let appdata = fixture.path("appdata");
        let home = fixture.path("home");
        let candidates = legacy_config_paths(
            Some(override_path.clone().into_os_string()),
            Some(appdata.clone().into_os_string()),
            Some(home.clone().into_os_string()),
        );
        assert_eq!(
            candidates,
            vec![
                override_path,
                appdata.join("Orally/config.toml"),
                home.join(".config/orally/config.toml"),
            ]
        );

        let destination = config_path_for(&fixture.executable(), &candidates).unwrap();
        assert_eq!(destination, fixture.path("app/config.toml"));
        assert!(!destination.exists());
        assert_eq!(
            legacy_config_paths(Some("".into()), None, None),
            Vec::<PathBuf>::new()
        );
    }

    #[test]
    fn migration_preserves_source_bytes_profiles_credentials_and_unknown_fields() {
        let fixture = ConfigFixture::new();
        let mut config = AppConfig::openai_compatible_preset();
        config.asr.base_url = "https://service.example/custom/v1".to_string();
        config.asr.model = "existing-model".to_string();
        config.asr.api_key = Some("fake-asr-migration-key".to_string());
        config.postprocess.models = vec![PostprocessModelConfig {
            id: "model-existing".to_string(),
            api_key: Some("fake-model-migration-key".to_string()),
            prompts: vec![PromptNodeConfig {
                id: "prompt-existing".to_string(),
                content: "保留原有 Prompt".to_string(),
                ..PromptNodeConfig::default()
            }],
            ..PostprocessModelConfig::default()
        }];
        config.profiles = vec![ConfigProfile {
            id: "profile-existing".to_string(),
            name: "旧配置".to_string(),
            asr: config.asr.clone(),
            postprocess: config.postprocess.clone(),
        }];
        config.active_profile_id = Some("profile-existing".to_string());
        let text = format!(
            "# Keep comments and unknown future fields\n{}\n[future]\nvalue = 'keep me'\n",
            to_toml(&config).unwrap()
        );
        let source = fixture.write("old/config.toml", &text);

        let destination =
            config_path_for(&fixture.executable(), std::slice::from_ref(&source)).unwrap();

        assert_eq!(fs::read(&source).unwrap(), text.as_bytes());
        assert_eq!(fs::read(&destination).unwrap(), text.as_bytes());
        assert_eq!(load_from_path(destination).unwrap(), config);
    }

    #[test]
    fn migration_uses_first_existing_legacy_path_in_order() {
        let fixture = ConfigFixture::new();
        let preferred = fixture.write_model("override.toml", "preferred");
        let other = fixture.write_model("home.toml", "other");
        let destination = config_path_for(
            &fixture.executable(),
            &[fixture.path("missing.toml"), preferred.clone(), other],
        )
        .unwrap();
        assert_eq!(fs::read(destination).unwrap(), fs::read(preferred).unwrap());
    }

    #[test]
    fn existing_beside_executable_config_wins_over_legacy_even_if_legacy_is_invalid() {
        let fixture = ConfigFixture::new();
        let destination = fixture.write_model("app/config.toml", "current");
        let legacy = fixture.write("old.toml", "invalid legacy configuration");
        assert_eq!(
            config_path_for(&fixture.executable(), &[legacy]).unwrap(),
            destination
        );
        assert_eq!(load_from_path(&destination).unwrap().asr.model, "current");
    }

    #[test]
    fn invalid_legacy_config_is_not_replaced_or_skipped_and_error_hides_credentials() {
        let fixture = ConfigFixture::new();
        let text = "[asr]\napi_key = \"fake-invalid-secret\n";
        let source = fixture.write("bad.toml", text);
        let fallback = fixture.write_model("other.toml", "other");
        let error =
            config_path_for(&fixture.executable(), &[source.clone(), fallback]).unwrap_err();

        assert!(matches!(error, ConfigError::Migration { .. }));
        let message = error.to_string();
        assert!(message.contains("bad.toml"));
        assert!(message.contains("invalid TOML configuration"));
        assert!(!message.contains("fake-invalid-secret"));
        assert_eq!(fs::read(&source).unwrap(), text.as_bytes());
        assert!(!fixture.path("app/config.toml").exists());
    }

    #[test]
    fn unreadable_legacy_config_is_not_silently_skipped() {
        let fixture = ConfigFixture::new();
        let source = fixture.path("directory-not-a-file");
        fs::create_dir(&source).unwrap();
        let fallback = fixture.write_model("other.toml", "other");
        let error =
            config_path_for(&fixture.executable(), &[source.clone(), fallback]).unwrap_err();
        assert!(matches!(error, ConfigError::Migration { .. }));
        assert!(error.to_string().contains("directory-not-a-file"));
        assert!(source.is_dir());
        assert!(!fixture.path("app/config.toml").exists());
    }

    #[test]
    fn migration_does_not_overwrite_a_destination_created_concurrently() {
        let fixture = ConfigFixture::new();
        let destination = fixture.write("app/config.toml", "current content");
        assert!(!create_new_config(&destination, b"legacy content").unwrap());
        assert_eq!(fs::read(&destination).unwrap(), b"current content");
    }

    #[test]
    fn invalid_current_config_is_reported_instead_of_using_valid_legacy() {
        let fixture = ConfigFixture::new();
        let destination = fixture.write("app/config.toml", "[asr]\nmodel = [\n");
        let legacy = fixture.write_model("old.toml", "old");
        let selected = config_path_for(&fixture.executable(), &[legacy]).unwrap();
        assert_eq!(selected, destination);
        assert!(matches!(
            load_from_path(selected),
            Err(ConfigError::Deserialize(_))
        ));
        assert_eq!(fs::read(destination).unwrap(), b"[asr]\nmodel = [\n");
    }

    #[test]
    fn ai_script_fixture_is_valid_and_disables_history_without_saved_keys() {
        let script = include_str!("../../../scripts/test-ai-postprocess.ps1");
        let (_, after_start) = script.split_once("$fixtureConfig = @'").unwrap();
        let (fixture, _) = after_start.split_once("'@").unwrap();
        let config = from_toml(fixture).unwrap();
        assert!(!config.privacy.history_enabled);
        assert!(!config.output.insert);
        assert!(config.asr.api_key.is_none());
        assert!(config.postprocess.api_key.is_none());
    }

    #[test]
    fn init_requires_explicit_force_to_replace_existing_beside_executable_config() {
        let fixture = ConfigFixture::new();
        let path = fixture.path("app/config.toml");
        let original = AppConfig::default();
        assert_eq!(
            init_config_at(path.clone(), &original, false).unwrap(),
            path
        );
        let original_bytes = fs::read(&path).unwrap();
        let mut replacement = original;
        replacement.asr.model = "replacement-model".to_string();

        assert!(matches!(
            init_config_at(path.clone(), &replacement, false),
            Err(ConfigError::AlreadyExists(_))
        ));
        assert_eq!(fs::read(&path).unwrap(), original_bytes);
        init_config_at(path.clone(), &replacement, true).unwrap();
        assert_eq!(load_from_path(path).unwrap(), replacement);
    }
}
