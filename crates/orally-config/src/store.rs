//! Executable-local settings and independently editable model profile documents.
use super::*;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceConfig {
    pub dark_mode: bool,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self { dark_mode: true }
    }
}

/// Calendar-day retention for content-free diagnostics. DEBUG metadata is always enabled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub auto_cleanup: bool,
    pub retention_days: std::num::NonZeroU16,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            auto_cleanup: true,
            retention_days: std::num::NonZeroU16::new(1).unwrap(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AppSettings {
    pub output: OutputConfig,
    pub audio: AudioConfig,
    pub hotkey: HotkeyConfig,
    pub privacy: PrivacyConfig,
    pub appearance: AppearanceConfig,
    pub logging: LoggingConfig,
}

impl AppSettings {
    pub fn from_config(config: &AppConfig) -> Self {
        Self {
            output: config.output.clone(),
            audio: config.audio.clone(),
            hotkey: config.hotkey.clone(),
            privacy: config.privacy.clone(),
            appearance: config.appearance.clone(),
            logging: config.logging.clone(),
        }
    }

    pub fn apply_to(&self, config: &mut AppConfig) {
        config.output = self.output.clone();
        config.audio = self.audio.clone();
        config.hotkey = self.hotkey.clone();
        config.privacy = self.privacy.clone();
        config.appearance = self.appearance.clone();
        config.logging = self.logging.clone();
    }

    /// Apply only fields edited by this client, retaining unrelated disk edits.
    pub fn merge_changes(&mut self, before: &Self, after: &Self) {
        fn merge_field<T: Clone + PartialEq>(value: &mut T, before: &T, after: &T) {
            if before != after {
                *value = after.clone();
            }
        }
        macro_rules! merge {
            ($section:ident; $($field:ident),+ $(,)?) => { $(
                merge_field(&mut self.$section.$field, &before.$section.$field, &after.$section.$field);
            )+ };
        }
        merge!(output; locale, insert, direct_output, auto_copy);
        merge!(audio; dictate_seconds, record_output, input_mode, auto_stop_enabled, min_record_ms, max_record_ms, silence_timeout_ms, silence_threshold);
        merge!(hotkey; preset);
        merge!(privacy; allow_external_requests, history_enabled, history_path);
        merge!(appearance; dark_mode);
        merge!(logging; auto_cleanup, retention_days);
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct SettingsDocument {
    active_profile_id: Option<String>,
    profile_order: Vec<String>,
    #[serde(flatten)]
    settings: AppSettings,
}

pub struct ConfigStore {
    directory: PathBuf,
}

impl ConfigStore {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }
    pub fn settings_path(&self) -> PathBuf {
        self.directory.join("setting.toml")
    }
    fn profiles_path(&self) -> PathBuf {
        self.directory.join("profiles")
    }

    /// The legacy file is retained. The settings file is written last and marks
    /// a completed migration; retries reuse any already migrated profiles.
    pub fn initialize(&self, legacy: Option<&Path>) -> Result<(), ConfigError> {
        if path_is_present(&self.settings_path())? {
            return Ok(());
        }
        fs::create_dir_all(self.profiles_path())?;
        let mut config = match legacy {
            Some(path) => load_from_path(path)?,
            None => AppConfig::default(),
        };
        normalize_profiles(&mut config)?;
        let existing = self.read_profiles()?;
        if existing.is_empty() || legacy.is_some() {
            for profile in &config.profiles {
                if !existing.iter().any(|(_, saved)| saved.id == profile.id) {
                    create_new_config(
                        &self.profile_path(&profile.id),
                        toml::to_string_pretty(profile)?.as_bytes(),
                    )?;
                }
            }
        }
        create_new_config(
            &self.settings_path(),
            toml::to_string_pretty(&document(&config))?.as_bytes(),
        )?;
        Ok(())
    }

    fn read_document(&self) -> Result<SettingsDocument, ConfigError> {
        Ok(toml::from_str(&fs::read_to_string(self.settings_path())?)?)
    }

    fn read_profiles(&self) -> Result<Vec<(PathBuf, ConfigProfile)>, ConfigError> {
        if !path_is_present(&self.profiles_path())? {
            return Ok(Vec::new());
        }
        let mut paths = fs::read_dir(self.profiles_path())?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()?;
        paths.sort();
        let mut profiles = Vec::new();
        let mut ids = HashSet::new();
        for path in paths {
            if !path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
            {
                continue;
            }
            let mut profile: ConfigProfile = toml::from_str(&fs::read_to_string(&path)?)?;
            validate_id(&profile.id)?;
            if !ids.insert(profile.id.clone()) {
                return Err(invalid("profile.id", "duplicate profile ID"));
            }
            profile.asr.protocol = OPENAI_COMPAT_ASR_PROTOCOL.to_string();
            profiles.push((path, profile));
        }
        Ok(profiles)
    }

    pub fn load(&self) -> Result<AppConfig, ConfigError> {
        let document = self.read_document()?;
        let mut profiles = self.read_profiles()?;
        profiles.sort_by_key(|(_, profile)| {
            document
                .profile_order
                .iter()
                .position(|id| id == &profile.id)
                .unwrap_or(usize::MAX)
        });
        let mut config = AppConfig {
            profiles: profiles.into_iter().map(|(_, profile)| profile).collect(),
            active_profile_id: document.active_profile_id,
            ..AppConfig::default()
        };
        document.settings.apply_to(&mut config);
        normalize_profiles(&mut config)?;
        Ok(config)
    }

    /// Used by CLI/init. Desktop settings and model saves use separate methods.
    pub fn save(&self, config: &AppConfig) -> Result<(), ConfigError> {
        let mut config = config.clone();
        normalize_profiles(&mut config)?;
        self.write_profiles(&config.profiles)?;
        atomic_write(
            &self.settings_path(),
            &toml::to_string_pretty(&document(&config))?,
        )
    }

    pub fn save_profiles(&self, incoming: &AppConfig) -> Result<AppConfig, ConfigError> {
        let mut config = self.load()?;
        // The UI has no profile-deletion action. Files added by another editor
        // must survive a save from a window that has not observed them yet.
        let added: Vec<_> = config
            .profiles
            .iter()
            .filter(|saved| {
                !incoming
                    .profiles
                    .iter()
                    .any(|profile| profile.id == saved.id)
            })
            .cloned()
            .collect();
        config.profiles = incoming.profiles.clone();
        config.profiles.extend(added);
        config.active_profile_id = incoming.active_profile_id.clone();
        if config.profiles.is_empty() {
            config.asr = incoming.asr.clone();
            config.postprocess = incoming.postprocess.clone();
        }
        self.save(&config)?;
        self.load()
    }

    /// Only setting.toml changes; credentials/model drafts never enter this path.
    pub fn settings(&self) -> Result<AppSettings, ConfigError> {
        Ok(self.read_document()?.settings)
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), ConfigError> {
        let mut document = self.read_document()?;
        document.settings = settings.clone();
        atomic_write(&self.settings_path(), &toml::to_string_pretty(&document)?)
    }

    pub fn select_profile(&self, id: &str) -> Result<AppConfig, ConfigError> {
        let config = self.load()?;
        if !config.profiles.iter().any(|profile| profile.id == id) {
            return Err(invalid("active_profile_id", "profile no longer exists"));
        }
        let mut document = self.read_document()?;
        document.active_profile_id = Some(id.to_string());
        atomic_write(&self.settings_path(), &toml::to_string_pretty(&document)?)?;
        self.load()
    }

    fn write_profiles(&self, profiles: &[ConfigProfile]) -> Result<(), ConfigError> {
        fs::create_dir_all(self.profiles_path())?;
        let previous = self.read_profiles()?;
        for profile in profiles {
            let path = previous
                .iter()
                .find(|(_, saved)| saved.id == profile.id)
                .map(|(path, _)| path.clone())
                .unwrap_or_else(|| self.profile_path(&profile.id));
            atomic_write(&path, &toml::to_string_pretty(profile)?)?;
        }
        // Files not included in an explicit model save are removed. Settings
        // saves and tray switches never touch any profile document.
        for (path, saved) in previous {
            if !profiles.iter().any(|profile| profile.id == saved.id) {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }

    fn profile_path(&self, id: &str) -> PathBuf {
        // Hex encoding is reversible, collision-free and cannot escape profiles/.
        let filename: String = id
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        self.profiles_path()
            .join(format!("profile-{filename}.toml"))
    }
}

fn document(config: &AppConfig) -> SettingsDocument {
    SettingsDocument {
        active_profile_id: config.active_profile_id.clone(),
        profile_order: config
            .profiles
            .iter()
            .map(|profile| profile.id.clone())
            .collect(),
        settings: AppSettings::from_config(config),
    }
}

fn invalid(key: &str, reason: &str) -> ConfigError {
    ConfigError::InvalidValue {
        key: key.into(),
        value: reason.into(),
    }
}
fn validate_id(id: &str) -> Result<(), ConfigError> {
    if id.is_empty() || id.len() > 100 {
        return Err(invalid("profile.id", "must contain 1–100 bytes"));
    }
    Ok(())
}

fn normalize_profiles(config: &mut AppConfig) -> Result<(), ConfigError> {
    if config.profiles.is_empty() {
        config.profiles.push(ConfigProfile {
            id: "profile-1".into(),
            name: "默认配置".into(),
            asr: config.asr.clone(),
            postprocess: config.postprocess.clone(),
        });
    }
    let mut ids = HashSet::new();
    for profile in &config.profiles {
        validate_id(&profile.id)?;
        if !ids.insert(&profile.id) {
            return Err(invalid("profile.id", "duplicate profile ID"));
        }
    }
    config.normalize_asr_protocols();
    let profile = config
        .profiles
        .iter()
        .find(|profile| Some(&profile.id) == config.active_profile_id.as_ref())
        .unwrap_or(&config.profiles[0]);
    config.active_profile_id = Some(profile.id.clone());
    config.asr = profile.asr.clone();
    config.postprocess = profile.postprocess.clone();
    Ok(())
}

fn atomic_write(path: &Path, text: &str) -> Result<(), ConfigError> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let temporary = path.with_extension(format!(
        "{}.{}.tmp",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(ConfigError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logging_defaults_migrate_and_settings_round_trip_without_touching_profiles() {
        let fixture = Fixture::new();
        fixture.0.initialize(None).unwrap();
        let original = fixture.0.load().unwrap();
        assert!(original.logging.auto_cleanup);
        assert_eq!(original.logging.retention_days.get(), 1);
        let mut settings = AppSettings::from_config(&original);
        settings.logging.auto_cleanup = false;
        settings.logging.retention_days = std::num::NonZeroU16::new(7).unwrap();
        fixture.0.save_settings(&settings).unwrap();
        let loaded = fixture.0.load().unwrap();
        assert_eq!(loaded.logging, settings.logging);
        assert_eq!(loaded.profiles, original.profiles);
        fs::write(
            fixture.0.settings_path(),
            "[appearance]\ndark_mode = false\n",
        )
        .unwrap();
        assert_eq!(fixture.0.load().unwrap().logging, LoggingConfig::default());
        for invalid in ["0", "-1", "65536", "1.5"] {
            fs::write(
                fixture.0.settings_path(),
                format!("[logging]\nretention_days = {invalid}\n"),
            )
            .unwrap();
            assert!(fixture.0.load().is_err(), "{invalid}");
        }
    }

    #[test]
    fn logging_setting_merge_preserves_independent_external_edits() {
        let base = AppSettings::default();
        let mut disk = base.clone();
        disk.logging.retention_days = std::num::NonZeroU16::new(30).unwrap();
        let mut edited = base.clone();
        edited.logging.auto_cleanup = false;
        disk.merge_changes(&base, &edited);
        assert_eq!(disk.logging.retention_days.get(), 30);
        assert!(!disk.logging.auto_cleanup);
    }

    struct Fixture(ConfigStore);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let directory = env::temp_dir().join(format!(
                "orally-split-store-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&directory).unwrap();
            Self(ConfigStore::new(directory))
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0.directory);
        }
    }
    fn two_profiles() -> AppConfig {
        let mut config = AppConfig::openai_compatible_preset();
        for id in ["a", "b"] {
            let mut profile = ConfigProfile {
                id: id.into(),
                name: format!("配置 {id}"),
                ..ConfigProfile::default()
            };
            profile.asr.api_key = Some(format!("fake-{id}-credential"));
            profile.asr.model = format!("speech-{id}");
            config.profiles.push(profile);
        }
        config.active_profile_id = Some("b".into());
        config
    }

    #[test]
    fn settings_merge_only_changed_fields_and_retains_concurrent_external_edits() {
        let base = AppSettings::default();
        let mut disk = base.clone();
        disk.hotkey.preset = "ctrl-shift-a".into();
        disk.privacy.history_enabled = false;
        disk.output.locale = "en-GB".into();
        let mut edited = base.clone();
        edited.appearance.dark_mode = false;
        edited.output.direct_output = false;
        edited.output.auto_copy = true;
        disk.merge_changes(&base, &edited);
        assert_eq!(disk.hotkey.preset, "ctrl-shift-a");
        assert!(!disk.privacy.history_enabled);
        assert_eq!(disk.output.locale, "en-GB");
        assert!(!disk.appearance.dark_mode);
        assert!(!disk.output.direct_output);
        assert!(disk.output.auto_copy);
    }

    #[test]
    fn model_save_retains_a_profile_added_externally_before_ui_sync() {
        let fixture = Fixture::new();
        let store = &fixture.0;
        let stale = two_profiles();
        store.save(&stale).unwrap();
        let added = ConfigProfile {
            id: "external".into(),
            name: "External".into(),
            ..ConfigProfile::default()
        };
        fs::write(
            store.profiles_path().join("added.toml"),
            toml::to_string_pretty(&added).unwrap(),
        )
        .unwrap();
        let saved = store.save_profiles(&stale).unwrap();
        assert_eq!(saved.profiles.len(), 3);
        assert!(saved.profiles.contains(&added));
    }

    #[test]
    fn migrates_all_profiles_and_retains_legacy_bytes_without_credentials_in_settings() {
        let fixture = Fixture::new();
        let store = &fixture.0;
        let legacy = store.directory.join("config.toml");
        let text = to_toml(&two_profiles()).unwrap();
        fs::write(&legacy, &text).unwrap();
        store.initialize(Some(&legacy)).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.profiles.len(), 2);
        assert_eq!(loaded.asr.model, "speech-b");
        assert_eq!(loaded.asr.api_key.as_deref(), Some("fake-b-credential"));
        assert!(loaded.appearance.dark_mode);
        assert_eq!(store.read_profiles().unwrap().len(), 2);
        assert_eq!(fs::read_to_string(&legacy).unwrap(), text);
        let settings = fs::read_to_string(store.settings_path()).unwrap();
        assert!(!settings.contains("api_key"));
        assert!(!settings.contains("fake-"));
        fs::write(&legacy, "invalid legacy now").unwrap();
        store.initialize(Some(&legacy)).unwrap();
        assert_eq!(store.load().unwrap(), loaded);
    }

    #[test]
    fn settings_and_selection_do_not_rewrite_profiles_and_model_saves_keep_latest_settings() {
        let fixture = Fixture::new();
        let store = &fixture.0;
        store.save(&two_profiles()).unwrap();
        let stale_editor = store.load().unwrap();
        let before: Vec<_> = store
            .read_profiles()
            .unwrap()
            .iter()
            .map(|(path, _)| (path.clone(), fs::read(path).unwrap()))
            .collect();
        let mut settings = AppSettings::from_config(&stale_editor);
        settings.appearance.dark_mode = false;
        settings.privacy.history_enabled = false;
        settings.output.direct_output = false;
        settings.output.auto_copy = true;
        store.save_settings(&settings).unwrap();
        store.select_profile("a").unwrap();
        for (path, contents) in before {
            assert_eq!(fs::read(path).unwrap(), contents);
        }
        let saved = store.save_profiles(&stale_editor).unwrap();
        assert!(!saved.appearance.dark_mode);
        assert!(!saved.privacy.history_enabled);
        assert!(!saved.output.direct_output);
        assert!(saved.output.auto_copy);
        assert_eq!(saved.profiles, stale_editor.profiles);
        assert!(!fs::read_dir(&store.directory).unwrap().any(|entry| entry
            .unwrap()
            .path()
            .extension()
            .is_some_and(|ext| ext == "tmp")));
    }

    #[test]
    fn external_profile_edits_additions_and_deletions_are_loaded_from_individual_files() {
        let fixture = Fixture::new();
        let store = &fixture.0;
        store.save(&two_profiles()).unwrap();
        let path = store
            .read_profiles()
            .unwrap()
            .into_iter()
            .find(|(_, profile)| profile.id == "a")
            .unwrap()
            .0;
        let mut edited = two_profiles().profiles.remove(0);
        edited.asr.model = "external-model".into();
        fs::write(&path, toml::to_string_pretty(&edited).unwrap()).unwrap();
        assert_eq!(
            store.select_profile("a").unwrap().asr.model,
            "external-model"
        );
        edited.id = "external".into();
        let added = store.profiles_path().join("my-profile.toml");
        fs::write(&added, toml::to_string_pretty(&edited).unwrap()).unwrap();
        assert_eq!(store.load().unwrap().profiles.len(), 3);
        fs::remove_file(path).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.active_profile_id.as_deref(), Some("b"));
        assert_eq!(loaded.profiles.len(), 2);
        assert!(store.select_profile("a").is_err());
    }

    #[test]
    fn sparse_settings_use_defaults_and_invalid_files_never_reset_saved_profiles() {
        let fixture = Fixture::new();
        let store = &fixture.0;
        store.initialize(None).unwrap();
        fs::write(
            store.settings_path(),
            "[appearance]\ndark_mode = false\n[output]\nlocale = 'en-GB'\n",
        )
        .unwrap();
        let loaded = store.load().unwrap();
        assert!(!loaded.appearance.dark_mode);
        assert_eq!(loaded.output.locale, "en-GB");
        assert!(loaded.output.direct_output);
        assert!(!loaded.output.auto_copy);
        let path = store.read_profiles().unwrap()[0].0.clone();
        let original = fs::read(&path).unwrap();
        fs::write(&path, "api_key = \"fake-secret\"\n[asr\n").unwrap();
        let error = store.load().unwrap_err().to_string();
        assert!(!error.contains("fake-secret"));
        assert!(store.save_profiles(&loaded).is_err());
        fs::write(&path, original).unwrap();
        assert_eq!(store.load().unwrap(), loaded);
    }

    #[test]
    fn profile_ids_cannot_escape_directory_and_duplicates_fail_before_writing() {
        let fixture = Fixture::new();
        let store = &fixture.0;
        let mut config = two_profiles();
        config.profiles[0].id = "../setting.toml".into();
        store.save(&config).unwrap();
        assert_eq!(store.read_profiles().unwrap().len(), 2);
        assert!(store
            .read_profiles()
            .unwrap()
            .iter()
            .all(|(path, _)| path.parent() == Some(store.profiles_path().as_path())));
        let before = fs::read(store.settings_path()).unwrap();
        config.profiles[1].id = config.profiles[0].id.clone();
        assert!(store.save(&config).is_err());
        assert_eq!(fs::read(store.settings_path()).unwrap(), before);
    }

    #[test]
    fn legacy_single_profile_is_split_and_invalid_migration_does_not_publish_settings() {
        let fixture = Fixture::new();
        let store = &fixture.0;
        let legacy = store.directory.join("config.toml");
        fs::write(&legacy, "[asr\n").unwrap();
        assert!(store.initialize(Some(&legacy)).is_err());
        assert!(!store.settings_path().exists());
        let mut config = AppConfig::default();
        config.asr.model = "legacy-speech".into();
        config.asr.api_key = Some("fake-secret".into());
        fs::write(&legacy, to_toml(&config).unwrap()).unwrap();
        store.initialize(Some(&legacy)).unwrap();
        let loaded = load_from_path(store.settings_path()).unwrap();
        assert_eq!(loaded.profiles.len(), 1);
        assert_eq!(loaded.asr, config.asr);
        assert!(loaded.appearance.dark_mode);
    }
}
