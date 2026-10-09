//! Saved-profile selection and the Windows tray menu.

use orally_config::{AppConfig, ConfigError};
use serde::Serialize;
use std::path::Path;
use std::sync::{mpsc, Mutex};
use std::time::Duration;
use tauri::menu::{Menu, MenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter};

const TRAY_ID: &str = "orally-main";
const PROFILE_MENU_PREFIX: &str = "profile:";
const ACTIVE_PROFILE_EVENT: &str = "active-profile-changed";
static CONFIG_IO_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, PartialEq, Eq)]
struct ProfileMenuEntry {
    profile_id: Option<String>,
    text: String,
    enabled: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ActiveProfileChanged {
    profile_id: String,
}

pub(super) fn with_config_lock<T>(action: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let _guard = CONFIG_IO_LOCK
        .lock()
        .map_err(|_| "配置文件暂时不可用，请重启 Orally 后重试。".to_string())?;
    action()
}

fn config_error_message(error: ConfigError) -> String {
    match error {
        ConfigError::Deserialize(_) => "配置文件格式无效，请打开设置检查配置文件。".to_string(),
        error => format!("配置读取或保存失败：{error}"),
    }
}

fn profile_menu_entries(config: &AppConfig) -> Vec<ProfileMenuEntry> {
    if config.profiles.is_empty() {
        return vec![ProfileMenuEntry {
            profile_id: None,
            text: "● 默认配置".to_string(),
            enabled: false,
        }];
    }
    let active_id = config.active_profile_id.as_deref();
    let mut entries = Vec::new();
    if !config
        .profiles
        .iter()
        .any(|profile| Some(profile.id.as_str()) == active_id)
    {
        entries.push(ProfileMenuEntry {
            profile_id: None,
            text: "● 当前配置（未保存）".to_string(),
            enabled: false,
        });
    }
    entries.extend(config.profiles.iter().map(|profile| {
        let name = if profile.name.trim().is_empty() {
            "未命名配置"
        } else {
            &profile.name
        };
        ProfileMenuEntry {
            profile_id: Some(profile.id.clone()),
            text: if Some(profile.id.as_str()) == active_id {
                format!("● {name}")
            } else {
                name.to_string()
            },
            enabled: true,
        }
    }));
    entries
}

fn entries_from_result(
    config: Result<AppConfig, String>,
) -> (Vec<ProfileMenuEntry>, Option<String>) {
    match config {
        Ok(config) => (profile_menu_entries(&config), None),
        Err(error) => (
            vec![ProfileMenuEntry {
                profile_id: None,
                text: "配置读取失败".to_string(),
                enabled: false,
            }],
            Some(error),
        ),
    }
}

fn load_menu_entries() -> (Vec<ProfileMenuEntry>, Option<String>) {
    entries_from_result(with_config_lock(|| {
        orally_config::load_or_default().map_err(config_error_message)
    }))
}

fn menu_label(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .replace('&', "&&")
}

fn make_menu(app: &AppHandle, entries: &[ProfileMenuEntry]) -> tauri::Result<Menu<tauri::Wry>> {
    let toggle = MenuItem::with_id(
        app,
        "toggle-dictation",
        "Start/Stop Dictation",
        true,
        None::<&str>,
    )?;
    let pause = MenuItem::with_id(
        app,
        "toggle-pause",
        "Pause/Resume Hotkey",
        true,
        None::<&str>,
    )?;
    let profiles = Submenu::with_id(app, "profiles", "配置", true)?;
    for entry in entries {
        let id = entry
            .profile_id
            .as_ref()
            .map(|id| format!("{PROFILE_MENU_PREFIX}{id}"))
            .unwrap_or_else(|| "profiles-status".to_string());
        let item = MenuItem::with_id(
            app,
            id,
            // Windows interprets ampersands as mnemonic markers.
            menu_label(&entry.text),
            entry.enabled,
            None::<&str>,
        )?;
        profiles.append(&item)?;
    }
    let show = MenuItem::with_id(app, "show", "Open Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Orally", true, None::<&str>)?;
    Menu::with_items(app, &[&toggle, &pause, &profiles, &show, &quit])
}

pub(super) fn build(
    app: &mut tauri::App,
    sender: mpsc::Sender<super::DictationCommand>,
) -> tauri::Result<()> {
    let (entries, load_error) = load_menu_entries();
    let menu = make_menu(app.handle(), &entries)?;
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Orally")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| {
            let id = event.id.as_ref();
            match id {
                "toggle-dictation" => {
                    let _ = sender.send(super::DictationCommand::ToggleFromUi);
                }
                "toggle-pause" => {
                    let _ = sender.send(super::DictationCommand::TogglePause);
                }
                "show" => super::show_settings(app),
                "quit" => app.exit(0),
                _ => {
                    if let Some(profile_id) = id.strip_prefix(PROFILE_MENU_PREFIX) {
                        switch_saved_profile(app, profile_id);
                    }
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                super::show_settings(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }
    let _tray = tray.build(app)?;
    if let Some(error) = load_error {
        notify_failure(app.handle(), "无法读取托盘配置", &error);
    }
    Ok(())
}

fn refresh_menu(app: &AppHandle) -> Result<(), String> {
    let (entries, load_error) = load_menu_entries();
    let menu = make_menu(app, &entries).map_err(|error| error.to_string())?;
    app.tray_by_id(TRAY_ID)
        .ok_or_else(|| "托盘图标暂时不可用。".to_string())?
        .set_menu(Some(menu))
        .map_err(|error| error.to_string())?;
    match load_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

pub(super) fn refresh_menu_or_notify(app: &AppHandle) {
    let handle = app.clone();
    if app
        .run_on_main_thread(move || {
            if let Err(error) = refresh_menu(&handle) {
                notify_failure(&handle, "托盘菜单更新失败", &error);
            }
        })
        .is_err()
    {
        notify_failure(
            app,
            "托盘菜单更新失败",
            "无法更新托盘菜单，请重启 Orally 后重试。",
        );
    }
}

fn select_profile(config: &AppConfig, profile_id: &str) -> Result<AppConfig, String> {
    let index = config
        .profiles
        .iter()
        .position(|profile| profile.id == profile_id)
        .ok_or_else(|| "该配置已不存在，请在设置中重新选择配置。".to_string())?;
    let mut selected = config.clone();
    let profile = &mut selected.profiles[index];
    if let Some(model) = profile.postprocess.models.first() {
        profile.postprocess.base_url = model.base_url.clone();
        profile.postprocess.model = model.model.clone();
        profile.postprocess.api_key = model.api_key.clone();
        profile.postprocess.api_key_env = model.api_key_env.clone();
        profile.postprocess.system_prompt = model.system_prompt.clone();
        profile.postprocess.user_template = model.user_template.clone();
        profile.postprocess.fallback_to_builtin = model.fallback_to_builtin;
    }
    selected.active_profile_id = Some(profile.id.clone());
    selected.asr = profile.asr.clone();
    selected.postprocess = profile.postprocess.clone();
    Ok(selected)
}

fn persist_selection(path: &Path, profile_id: &str) -> Result<AppConfig, String> {
    let config = orally_config::load_from_path(path).map_err(config_error_message)?;
    let selected = select_profile(&config, profile_id)?;
    orally_config::save_to_path(path, &selected).map_err(config_error_message)?;
    Ok(selected)
}

fn switch_saved_profile(app: &AppHandle, profile_id: &str) {
    let selected = with_config_lock(|| {
        let path = orally_config::config_path().map_err(config_error_message)?;
        persist_selection(&path, profile_id)
    });
    match selected {
        Ok(selected) => {
            refresh_menu_or_notify(app);
            if app
                .emit(
                    ACTIVE_PROFILE_EVENT,
                    ActiveProfileChanged {
                        profile_id: selected.active_profile_id.unwrap_or_default(),
                    },
                )
                .is_err()
            {
                notify_failure(app, "配置已切换", "设置窗口未能更新，请重新打开设置。");
            }
        }
        Err(error) => notify_failure(app, "配置切换失败", &error),
    }
}

fn notify_failure(app: &AppHandle, title: &str, detail: &str) {
    super::show_transient_overlay(app, title, detail, Duration::from_secs(5));
}

#[cfg(test)]
mod tests;
