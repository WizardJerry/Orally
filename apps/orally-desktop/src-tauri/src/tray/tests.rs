use super::*;
use orally_config::{ConfigProfile, PostprocessModelConfig, PromptNodeConfig};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_TEMP_PATH: AtomicUsize = AtomicUsize::new(0);

struct TempConfig {
    directory: PathBuf,
}

impl TempConfig {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "orally-tray-tests-{}-{}",
            std::process::id(),
            NEXT_TEMP_PATH.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        Self { directory }
    }

    fn path(&self) -> PathBuf {
        self.directory.join("config.toml")
    }
}

impl Drop for TempConfig {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn profiles_config() -> AppConfig {
    let mut first = ConfigProfile {
        id: "first".to_string(),
        name: "第一份配置".to_string(),
        ..ConfigProfile::default()
    };
    first.asr.model = "first-asr".to_string();
    first.asr.api_key = Some("first-private-key".to_string());
    first.postprocess.model = "first-refinement".to_string();
    let mut second = ConfigProfile {
        id: "second:中文/with spaces".to_string(),
        name: "写作 & 工作".to_string(),
        ..ConfigProfile::default()
    };
    second.asr.model = "second-asr".to_string();
    second.asr.api_key = Some("second-private-key".to_string());
    second.postprocess.mode = "llm".to_string();
    second.postprocess.models = vec![PostprocessModelConfig {
        id: "second-model".to_string(),
        enabled: false,
        base_url: "https://second.invalid/v1".to_string(),
        model: "second-refinement".to_string(),
        api_key: Some("second-refinement-key".to_string()),
        api_key_env: "SECOND_ENV_KEY".to_string(),
        system_prompt: "base instructions".to_string(),
        user_template: "second: {{transcript}}".to_string(),
        fallback_to_builtin: false,
        prompts: vec![PromptNodeConfig {
            id: "prompt".to_string(),
            content: "extra instructions".to_string(),
            ..PromptNodeConfig::default()
        }],
        ..PostprocessModelConfig::default()
    }];
    let mut config = AppConfig {
        active_profile_id: Some(first.id.clone()),
        asr: first.asr.clone(),
        postprocess: first.postprocess.clone(),
        profiles: vec![first, second],
        ..AppConfig::default()
    };
    config.output.locale = "en-GB".to_string();
    config.output.raw = true;
    config.output.paste_delay_ms = 123;
    config.audio.auto_stop_enabled = true;
    config.audio.silence_timeout_ms = 2345;
    config.hotkey.preset = "ctrl-shift-space".to_string();
    config.privacy.allow_external_requests = false;
    config.privacy.history_enabled = false;
    config.privacy.history_path = Some("keep-this-history-path".to_string());
    config
}

fn assert_globals_unchanged(before: &AppConfig, after: &AppConfig) {
    assert_eq!(after.output, before.output);
    assert_eq!(after.audio, before.audio);
    assert_eq!(after.hotkey, before.hotkey);
    assert_eq!(after.privacy, before.privacy);
}

#[test]
fn menu_lists_every_saved_profile_and_marks_only_the_active_name() {
    let config = profiles_config();
    let entries = profile_menu_entries(&config);

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].profile_id.as_deref(), Some("first"));
    assert_eq!(entries[0].text, "● 第一份配置");
    assert_eq!(
        entries[1].profile_id.as_deref(),
        Some("second:中文/with spaces")
    );
    assert_eq!(entries[1].text, "写作 & 工作");
    assert!(entries.iter().all(|entry| entry.enabled));
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.text.starts_with("● "))
            .count(),
        1
    );
}

#[test]
fn menu_labels_preserve_unicode_and_escape_windows_menu_control_characters() {
    let mut config = profiles_config();
    config.profiles[0].name = "中文 & Writing\tmode\nnext\0end".to_string();
    let entries = profile_menu_entries(&config);

    assert_eq!(
        menu_label(&entries[0].text),
        "● 中文 && Writing mode next end"
    );
    assert_eq!(entries[0].profile_id.as_deref(), Some("first"));
    assert_eq!(config.profiles[0].name, "中文 & Writing\tmode\nnext\0end");
}

#[test]
fn legacy_and_missing_active_profiles_have_disabled_current_status_entries() {
    let legacy = profile_menu_entries(&AppConfig::default());
    assert_eq!(legacy.len(), 1);
    assert_eq!(legacy[0].text, "● 默认配置");
    assert!(!legacy[0].enabled);
    assert_eq!(legacy[0].profile_id, None);

    let mut config = profiles_config();
    config.active_profile_id = Some("removed-profile".to_string());
    let entries = profile_menu_entries(&config);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].text, "● 当前配置（未保存）");
    assert!(!entries[0].enabled);
    assert!(entries[1..]
        .iter()
        .all(|entry| !entry.text.starts_with("● ")));
}

#[test]
fn selecting_profile_normalizes_raw_legacy_fields_and_preserves_nodes_and_globals() {
    let config = profiles_config();
    let original = config.clone();
    let id = config.profiles[1].id.clone();
    let selected = select_profile(&config, &id).unwrap();
    let first_model = &config.profiles[1].postprocess.models[0];

    assert_eq!(config, original);
    assert_eq!(selected.active_profile_id.as_deref(), Some(id.as_str()));
    assert_eq!(selected.asr, config.profiles[1].asr);
    assert_eq!(selected.postprocess, selected.profiles[1].postprocess);
    assert_eq!(selected.postprocess.base_url, first_model.base_url);
    assert_eq!(selected.postprocess.model, first_model.model);
    assert_eq!(selected.postprocess.api_key, first_model.api_key);
    assert_eq!(selected.postprocess.api_key_env, first_model.api_key_env);
    assert_eq!(selected.postprocess.system_prompt, "base instructions");
    assert_eq!(
        selected.postprocess.user_template,
        first_model.user_template
    );
    assert_eq!(
        selected.postprocess.fallback_to_builtin,
        first_model.fallback_to_builtin
    );
    assert_eq!(
        selected.postprocess.mode,
        config.profiles[1].postprocess.mode
    );
    assert_eq!(
        selected.postprocess.models,
        config.profiles[1].postprocess.models
    );
    assert_eq!(selected.profiles[0], config.profiles[0]);
    assert_eq!(selected.profiles[1].name, config.profiles[1].name);
    assert_eq!(selected.profiles[1].id, config.profiles[1].id);
    assert_globals_unchanged(&config, &selected);
    assert_eq!(profile_menu_entries(&selected)[1].text, "● 写作 & 工作");
}

#[test]
fn selecting_a_legacy_profile_without_models_keeps_its_existing_settings() {
    let mut config = profiles_config();
    config.active_profile_id = Some(config.profiles[1].id.clone());
    let selected = select_profile(&config, "first").unwrap();

    assert_eq!(selected.postprocess, config.profiles[0].postprocess);
    assert_eq!(selected.profiles, config.profiles);
    assert_globals_unchanged(&config, &selected);
}

#[test]
fn tray_selection_persists_active_id_and_runtime_mirrors_to_a_temporary_file() {
    let file = TempConfig::new();
    let config = profiles_config();
    let id = config.profiles[1].id.clone();
    orally_config::save_to_path(file.path(), &config).unwrap();

    let selected = persist_selection(&file.path(), &id).unwrap();
    let reloaded = orally_config::load_from_path(file.path()).unwrap();

    assert_eq!(reloaded, selected);
    assert_eq!(reloaded.active_profile_id.as_deref(), Some(id.as_str()));
    assert_eq!(reloaded.asr.model, "second-asr");
    assert_eq!(reloaded.postprocess.model, "second-refinement");
    assert_eq!(reloaded.postprocess.system_prompt, "base instructions");
    assert_eq!(
        reloaded.postprocess.models[0].prompts[0].content,
        "extra instructions"
    );
    assert!(!reloaded.postprocess.models[0].enabled);
    assert_globals_unchanged(&config, &reloaded);
}

#[test]
fn unknown_profile_fails_without_writing_or_changing_the_config() {
    let file = TempConfig::new();
    let config = profiles_config();
    orally_config::save_to_path(file.path(), &config).unwrap();
    let original_bytes = fs::read(file.path()).unwrap();

    assert!(select_profile(&config, "unknown-id").is_err());
    assert!(persist_selection(&file.path(), "unknown-id").is_err());
    assert_eq!(fs::read(file.path()).unwrap(), original_bytes);
    assert_eq!(orally_config::load_from_path(file.path()).unwrap(), config);
}

#[test]
fn corrupt_or_missing_config_does_not_write_and_produces_a_disabled_menu_status() {
    let file = TempConfig::new();
    let corrupt = "[asr]\napi_key = \"private-key-without-closing-quote\n";
    fs::write(file.path(), corrupt).unwrap();

    let error = persist_selection(&file.path(), "first").unwrap_err();
    assert!(error.contains("格式无效"));
    assert!(!error.contains("private-key"));
    assert_eq!(fs::read_to_string(file.path()).unwrap(), corrupt);
    let (entries, read_error) = entries_from_result(Err(error.clone()));
    assert_eq!(read_error.as_deref(), Some(error.as_str()));
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].text, "配置读取失败");
    assert!(!entries[0].enabled);
    assert_eq!(entries[0].profile_id, None);

    let missing_path = file.directory.join("missing.toml");
    assert!(persist_selection(&missing_path, "first").is_err());
    assert!(!missing_path.exists());
}

#[test]
fn reloading_saved_menu_entries_reflects_renames_and_new_imported_profiles() {
    let file = TempConfig::new();
    let mut config = profiles_config();
    orally_config::save_to_path(file.path(), &config).unwrap();
    config.profiles[0].name = "新的名字".to_string();
    config.profiles.push(ConfigProfile {
        id: "imported".to_string(),
        name: "导入的配置".to_string(),
        ..ConfigProfile::default()
    });
    config.active_profile_id = Some("imported".to_string());
    orally_config::save_to_path(file.path(), &config).unwrap();

    let (entries, error) = entries_from_result(
        orally_config::load_from_path(file.path()).map_err(config_error_message),
    );

    assert_eq!(error, None);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].text, "新的名字");
    assert_eq!(entries[2].text, "● 导入的配置");
    assert_eq!(entries[2].profile_id.as_deref(), Some("imported"));
}

#[test]
fn notification_contains_only_the_selected_profile_id() {
    let config = profiles_config();
    let notification = ActiveProfileChanged {
        profile_id: config.profiles[1].id.clone(),
    };

    assert_eq!(notification.profile_id, "second:中文/with spaces");
    let debug = format!("{notification:?}");
    assert!(!debug.contains("private-key"));
    assert!(!debug.contains("refinement-key"));
    assert_eq!(ACTIVE_PROFILE_EVENT, "active-profile-changed");
}
