#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use orally_audio::CpalRecordingSession;
use orally_config::AppConfig;
use orally_core::{InsertMode, OrallyError, TextInserter};
use orally_speech::{
    AiPostprocessPlan, AsrProtocol, CredentialSource, RecognitionPlan, RefinementPlan, SpeechPlan,
    SpeechProcessor,
};
use orally_storage::{default_history_path, HistoryEntry, HistoryStore};
use orally_windows::{run_hotkey_loop, Hotkey, WindowsClipboardPasteInserter, WindowsPasteConfig};
use serde::Serialize;
use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow, WindowEvent,
};

#[derive(Debug)]
struct DictationController {
    sender: Mutex<mpsc::Sender<DictationCommand>>,
}

#[derive(Debug, Clone, Copy)]
enum DictationCommand {
    ToggleFromHotkey,
    ToggleFromUi,
    TogglePause,
}

struct ActiveRecording {
    session: CpalRecordingSession,
    target: Option<ForegroundWindow>,
}

#[derive(Debug, Clone, Copy)]
struct ForegroundWindow(isize);

#[derive(Debug, Clone, Serialize)]
struct OverlayStatus {
    title: String,
    detail: String,
    can_stop: bool,
}

#[tauri::command]
fn get_config_path() -> Result<String, String> {
    orally_config::config_path()
        .map(|path| path.display().to_string())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_config() -> Result<AppConfig, String> {
    orally_config::load_or_default().map_err(|error| error.to_string())
}

#[tauri::command]
fn save_config(config: AppConfig) -> Result<(), String> {
    let path = orally_config::config_path().map_err(|error| error.to_string())?;
    orally_config::save_to_path(path, &config).map_err(|error| error.to_string())
}

#[tauri::command]
fn get_portable_config_path() -> Result<Option<String>, String> {
    Ok(orally_config::portable_config_path().map(|path| path.display().to_string()))
}

#[tauri::command]
fn enable_portable_config(config: AppConfig) -> Result<String, String> {
    orally_config::init_portable_config(&config, true)
        .map(|path| path.display().to_string())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn stop_recording(state: State<'_, DictationController>) -> Result<(), String> {
    state
        .sender
        .lock()
        .map_err(|error| error.to_string())?
        .send(DictationCommand::ToggleFromUi)
        .map_err(|error| error.to_string())
}

fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn build_tray(app: &mut tauri::App, sender: mpsc::Sender<DictationCommand>) -> tauri::Result<()> {
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
    let show = MenuItem::with_id(app, "show", "Open Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Orally", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&toggle, &pause, &show, &quit])?;

    let mut tray = TrayIconBuilder::with_id("orally-main")
        .tooltip("Orally")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "toggle-dictation" => {
                let _ = sender.send(DictationCommand::ToggleFromUi);
            }
            "toggle-pause" => {
                let _ = sender.send(DictationCommand::TogglePause);
            }
            "show" => show_settings(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_settings(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }

    let _tray = tray.build(app)?;
    Ok(())
}

fn start_dictation_service(app: AppHandle) -> DictationController {
    let (sender, receiver) = mpsc::channel::<DictationCommand>();
    let hotkey_sender = sender.clone();
    let hotkey_app = app.clone();
    let hotkey = load_hotkey();
    let hotkey_label = hotkey.label();

    thread::spawn(move || {
        if let Err(error) = run_hotkey_loop(hotkey, |_| {
            hotkey_sender
                .send(DictationCommand::ToggleFromHotkey)
                .map_err(|error| OrallyError::Insertion(error.to_string()))
        }) {
            show_transient_overlay(
                &hotkey_app,
                "快捷键注册失败",
                &format!("{error}"),
                Duration::from_secs(6),
            );
        }
    });

    thread::spawn(move || run_dictation_processor(app, receiver, hotkey_label));

    DictationController {
        sender: Mutex::new(sender),
    }
}

fn load_hotkey() -> Hotkey {
    orally_config::load_or_default()
        .ok()
        .and_then(|config| Hotkey::from_preset(&config.hotkey.preset))
        .unwrap_or_else(Hotkey::ctrl_alt_space)
}

fn run_dictation_processor(
    app: AppHandle,
    receiver: mpsc::Receiver<DictationCommand>,
    hotkey_label: String,
) {
    let mut recording: Option<ActiveRecording> = None;
    let mut paused = false;

    while let Ok(command) = receiver.recv() {
        match command {
            DictationCommand::TogglePause => {
                paused = !paused;
                if paused {
                    show_transient_overlay(
                        &app,
                        "热键已暂停",
                        "托盘菜单仍可手动开始或停止语音输入",
                        Duration::from_secs(3),
                    );
                } else {
                    show_transient_overlay(
                        &app,
                        "热键已恢复",
                        &format!("当前热键：{hotkey_label}"),
                        Duration::from_secs(3),
                    );
                }
            }
            DictationCommand::ToggleFromHotkey if paused => {}
            DictationCommand::ToggleFromHotkey | DictationCommand::ToggleFromUi => {
                if let Some(active) = recording.take() {
                    hide_overlay(&app);
                    if let Err(error) = finish_dictation(&app, active) {
                        show_transient_overlay(
                            &app,
                            "语音输入失败",
                            &format!("{error}"),
                            Duration::from_secs(5),
                        );
                    }
                    while receiver.try_recv().is_ok() {}
                } else {
                    match CpalRecordingSession::start() {
                        Ok(session) => {
                            recording = Some(ActiveRecording {
                                session,
                                target: capture_foreground_window(),
                            });
                            show_recording_overlay(&app, &hotkey_label);
                        }
                        Err(error) => show_transient_overlay(
                            &app,
                            "录音启动失败",
                            &format!("{error}"),
                            Duration::from_secs(5),
                        ),
                    }
                }
            }
        }
    }
}

fn finish_dictation(app: &AppHandle, active: ActiveRecording) -> Result<(), OrallyError> {
    show_processing_overlay(app);

    let recorded = active.session.stop()?;
    let config = orally_config::load_or_default()
        .map_err(|error| OrallyError::InvalidInput(error.to_string()))?;
    if !config.privacy.allow_external_requests {
        return Err(OrallyError::InvalidInput(
            "external requests are disabled in privacy settings".to_string(),
        ));
    }

    let speech = build_speech_processor(&config)?;
    let outcome = speech.process(recorded.audio)?;
    let raw_text = outcome.raw_transcript.text;
    let final_text = outcome.final_text;

    restore_foreground_window(active.target);
    let inserter = WindowsClipboardPasteInserter::new(WindowsPasteConfig {
        paste_delay: Duration::from_millis(config.output.paste_delay_ms),
        restore_clipboard: config.output.restore_clipboard,
        restore_clipboard_delay: Duration::from_millis(config.output.restore_clipboard_delay_ms),
    });
    inserter.insert(&final_text, InsertMode::ClipboardFallback)?;

    if let Err(error) = save_history(&config, raw_text, final_text) {
        show_transient_overlay(
            app,
            "已插入文本",
            &format!("历史保存失败：{error}"),
            Duration::from_secs(4),
        );
    } else {
        show_transient_overlay(app, "已插入文本", "语音输入完成", Duration::from_secs(2));
    }
    Ok(())
}

fn build_speech_processor(config: &AppConfig) -> Result<SpeechProcessor, OrallyError> {
    let refinement = if config.output.raw {
        RefinementPlan::Raw
    } else if matches!(config.postprocess.mode.as_str(), "llm" | "ai") {
        RefinementPlan::AiPostprocessing(AiPostprocessPlan {
            base_url: config.postprocess.base_url.clone(),
            model: config.postprocess.model.clone(),
            credential: CredentialSource::new(
                config.postprocess.api_key.clone(),
                config.postprocess.api_key_env.clone(),
            ),
            system_prompt: config.postprocess.system_prompt.clone(),
            user_template: config.postprocess.user_template.clone(),
            fallback_to_local_basic_cleanup: config.postprocess.fallback_to_builtin,
        })
    } else {
        RefinementPlan::LocalBasicCleanup
    };

    SpeechProcessor::new(SpeechPlan {
        recognition: RecognitionPlan {
            base_url: config.asr.base_url.clone(),
            model: config.asr.model.clone(),
            credential: CredentialSource::new(
                config.asr.api_key.clone(),
                config.asr.api_key_env.clone(),
            ),
            protocol: config.asr.protocol.parse().unwrap_or(AsrProtocol::Auto),
            language: config.asr.language.clone(),
            prompt: config.asr.prompt.clone(),
        },
        refinement,
        locale: config.output.locale.clone(),
    })
    .map_err(OrallyError::from)
}

fn save_history(
    config: &AppConfig,
    raw_text: String,
    final_text: String,
) -> Result<(), OrallyError> {
    if !config.privacy.history_enabled {
        return Ok(());
    }

    let path = if let Some(path) = config
        .privacy
        .history_path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        path.into()
    } else {
        let config_path = orally_config::config_path()
            .map_err(|error| OrallyError::Processing(error.to_string()))?;
        default_history_path(&config_path)
    };
    let store = HistoryStore::new(path);
    store.append(&HistoryEntry::new(raw_text, final_text, "desktop"))
}

fn show_recording_overlay(app: &AppHandle, hotkey_label: &str) {
    show_overlay(
        app,
        OverlayStatus {
            title: "正在语音输入".to_string(),
            detail: format!("再次按 {hotkey_label} 或点击停止"),
            can_stop: true,
        },
    );
}

fn show_processing_overlay(app: &AppHandle) {
    show_overlay(
        app,
        OverlayStatus {
            title: "正在整理文字".to_string(),
            detail: "正在识别并后处理语音内容".to_string(),
            can_stop: false,
        },
    );
}

fn show_transient_overlay(app: &AppHandle, title: &str, detail: &str, duration: Duration) {
    show_overlay(
        app,
        OverlayStatus {
            title: title.to_string(),
            detail: detail.to_string(),
            can_stop: false,
        },
    );

    let app = app.clone();
    thread::spawn(move || {
        thread::sleep(duration);
        hide_overlay(&app);
    });
}

fn show_overlay(app: &AppHandle, status: OverlayStatus) {
    if let Some(window) = app.get_webview_window("dictation") {
        position_overlay(&window);
        let _ = window.show();
        let _ = window.emit("dictation-status", status);
    }
}

fn hide_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("dictation") {
        let _ = window.hide();
    }
}

fn position_overlay(window: &WebviewWindow) {
    let width = 380_u32;
    let height = 96_u32;
    let bottom_margin = 72_i32;

    let _ = window.set_size(PhysicalSize::new(width, height));

    if let Ok(Some(monitor)) = window.current_monitor() {
        let monitor_size = monitor.size();
        let monitor_position = monitor.position();
        let x = monitor_position.x + ((monitor_size.width.saturating_sub(width)) / 2) as i32;
        let y = monitor_position.y + monitor_size.height as i32 - height as i32 - bottom_margin;
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
}

#[cfg(windows)]
fn capture_foreground_window() -> Option<ForegroundWindow> {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_null() {
        None
    } else {
        Some(ForegroundWindow(hwnd as isize))
    }
}

#[cfg(not(windows))]
fn capture_foreground_window() -> Option<ForegroundWindow> {
    None
}

#[cfg(windows)]
fn restore_foreground_window(target: Option<ForegroundWindow>) {
    use windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

    if let Some(target) = target {
        unsafe {
            SetForegroundWindow(target.0 as _);
        }
        thread::sleep(Duration::from_millis(120));
    }
}

#[cfg(not(windows))]
fn restore_foreground_window(_target: Option<ForegroundWindow>) {}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let controller = start_dictation_service(app.handle().clone());
            let tray_sender = controller
                .sender
                .lock()
                .expect("dictation sender mutex should not be poisoned")
                .clone();
            build_tray(app, tray_sender)?;
            app.manage(controller);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_config_path,
            get_config,
            save_config,
            get_portable_config_path,
            enable_portable_config,
            stop_recording
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Orally desktop app");
}
