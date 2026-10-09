#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod hotkey;
mod service_test;
mod test_recording;
mod tray;

use orally_audio::{AudioMetrics, CpalRecordingSession, VoiceActivityConfig};
use orally_config::AppConfig;
use orally_core::{InsertMode, OrallyError, TextInserter};
use orally_speech::{
    AiPostprocessPlan, AsrProtocol, CredentialSource, RecognitionPlan, RefinementPlan, SpeechPlan,
    SpeechProcessor,
};
use orally_storage::{default_history_path, HistoryEntry, HistoryStore};
use orally_windows::{Hotkey, WindowsClipboardPasteInserter, WindowsPasteConfig};
use serde::Serialize;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow,
    WindowEvent,
};

const DICTATION_LEVEL_INTERVAL: Duration = Duration::from_millis(70);
const DICTATION_LEVEL_WINDOW: Duration = Duration::from_millis(120);
const OVERLAY_WIDTH: f64 = 300.0;
const OVERLAY_HEIGHT: f64 = 88.0;
const OVERLAY_BOTTOM_MARGIN: f64 = 72.0;
static OVERLAY_STATE: Mutex<OverlayState> = Mutex::new(OverlayState::new());

#[derive(Debug)]
struct DictationController {
    sender: Mutex<mpsc::Sender<DictationCommand>>,
}

#[derive(Debug, Clone, Copy)]
enum DictationCommand {
    ToggleFromHotkey,
    ToggleFromUi,
    StopIfRecording,
    TogglePause,
    RefreshHotkey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DictationAction {
    Start,
    Stop,
    Ignore,
}

fn dictation_action(command: DictationCommand, recording: bool, paused: bool) -> DictationAction {
    match command {
        DictationCommand::TogglePause | DictationCommand::RefreshHotkey => DictationAction::Ignore,
        DictationCommand::ToggleFromHotkey if paused => DictationAction::Ignore,
        DictationCommand::StopIfRecording if !recording => DictationAction::Ignore,
        _ if recording => DictationAction::Stop,
        _ => DictationAction::Start,
    }
}

struct ActiveRecording {
    session: CpalRecordingSession,
    target: Option<ForegroundWindow>,
    _lease: test_recording::RecordingLease,
}

#[derive(Debug, Clone, Copy)]
struct ForegroundWindow(isize);

#[derive(Debug, Clone, Serialize)]
struct OverlayStatus {
    title: String,
    detail: String,
    can_stop: bool,
}

struct OverlayState {
    generation: u64,
    status: Option<OverlayStatus>,
}

impl OverlayState {
    const fn new() -> Self {
        Self {
            generation: 0,
            status: None,
        }
    }

    fn reserve(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }

    fn apply(&mut self, generation: u64, status: Option<OverlayStatus>) -> bool {
        if self.generation != generation {
            return false;
        }
        self.status = status;
        true
    }

    fn dismiss(&mut self, generation: u64) -> bool {
        if self.generation != generation {
            return false;
        }
        self.reserve();
        self.status = None;
        true
    }
}

fn with_overlay_state<T>(action: impl FnOnce(&mut OverlayState) -> T) -> T {
    action(
        &mut OVERLAY_STATE
            .lock()
            .unwrap_or_else(|error| error.into_inner()),
    )
}

#[derive(Debug, Default, Clone, Copy, Serialize, PartialEq)]
struct DictationLevel {
    rms: f32,
    peak: f32,
}

impl DictationLevel {
    fn from_metrics(metrics: AudioMetrics) -> Self {
        fn amplitude(value: f32) -> f32 {
            if value.is_finite() {
                value.clamp(0.0, 1.0)
            } else {
                0.0
            }
        }

        Self {
            rms: amplitude(metrics.rms_amplitude),
            peak: amplitude(metrics.peak_amplitude),
        }
    }
}

#[tauri::command]
fn get_config() -> Result<AppConfig, String> {
    tray::with_config_lock(|| orally_config::load_or_default().map_err(|error| error.to_string()))
}

#[tauri::command]
fn parse_config_document(content: String) -> Result<AppConfig, String> {
    orally_config::from_toml(&content).map_err(|error| error.to_string())
}

#[tauri::command]
fn save_config(
    app: AppHandle,
    state: State<'_, hotkey::HotkeyController>,
    config: AppConfig,
) -> Result<(), String> {
    state.save(config)?;
    tray::refresh_menu_or_notify(&app);
    Ok(())
}

#[tauri::command]
fn stop_recording(state: State<'_, DictationController>) -> Result<(), String> {
    state
        .sender
        .lock()
        .map_err(|error| error.to_string())?
        .send(DictationCommand::StopIfRecording)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn replay_dictation_status(window: WebviewWindow) -> Result<(), String> {
    if window.label() != "dictation" {
        return Err("此命令仅用于语音输入状态浮层。".to_string());
    }
    let app = window.app_handle().clone();
    app.run_on_main_thread(move || {
        let status = with_overlay_state(|state| state.status.clone());
        let _ = window.emit("dictation-level", DictationLevel::default());
        if let Some(status) = status {
            let _ = window.emit("dictation-status", status);
        }
    })
    .map_err(|error| error.to_string())
}

fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn start_dictation_service(
    app: AppHandle,
    recording_gate: Arc<test_recording::RecordingGate>,
) -> DictationController {
    let (sender, receiver) = mpsc::channel::<DictationCommand>();
    let hotkey_sender = sender.clone();
    let hotkey_app = app.clone();
    let update_sender = sender.clone();
    let hotkey_controller = hotkey::HotkeyController::new(
        load_hotkey(),
        move || {
            let _ = hotkey_sender.send(DictationCommand::ToggleFromHotkey);
        },
        move || {
            let _ = update_sender.send(DictationCommand::RefreshHotkey);
        },
        move |error| {
            show_transient_overlay(
                &hotkey_app,
                "快捷键注册失败",
                &error,
                Duration::from_secs(6),
            );
        },
    );
    let current_hotkey = hotkey_controller.current();
    app.manage(hotkey_controller);
    thread::spawn(move || run_dictation_processor(app, receiver, current_hotkey, recording_gate));

    DictationController {
        sender: Mutex::new(sender),
    }
}

fn load_hotkey() -> Hotkey {
    tray::with_config_lock(|| orally_config::load_or_default().map_err(|error| error.to_string()))
        .ok()
        .and_then(|config| Hotkey::from_preset(&config.hotkey.preset))
        .unwrap_or_else(Hotkey::ctrl_alt_space)
}

fn run_dictation_processor(
    app: AppHandle,
    receiver: mpsc::Receiver<DictationCommand>,
    current_hotkey: Arc<Mutex<Hotkey>>,
    recording_gate: Arc<test_recording::RecordingGate>,
) {
    let mut recording: Option<ActiveRecording> = None;
    let mut paused = false;

    loop {
        let command = if let Some(active) = recording.as_ref() {
            match receiver.recv_timeout(DICTATION_LEVEL_INTERVAL) {
                Ok(command) => command,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if let Ok(metrics) = active
                        .session
                        .recent_metrics(DICTATION_LEVEL_WINDOW, VoiceActivityConfig::default())
                    {
                        emit_dictation_level(&app, DictationLevel::from_metrics(metrics));
                    }
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match receiver.recv() {
                Ok(command) => command,
                Err(_) => break,
            }
        };
        let hotkey_label = current_hotkey
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .label();
        if matches!(command, DictationCommand::RefreshHotkey) {
            if recording.is_some() {
                show_recording_overlay(&app, &hotkey_label, paused);
            }
            continue;
        }
        if matches!(command, DictationCommand::TogglePause) {
            paused = !paused;
            if recording.is_some() {
                show_recording_overlay(&app, &hotkey_label, paused);
            } else if paused {
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
            continue;
        }
        match dictation_action(command, recording.is_some(), paused) {
            DictationAction::Ignore => {}
            DictationAction::Stop => {
                let active = recording
                    .take()
                    .expect("stop action requires active recording");
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
            }
            DictationAction::Start => {
                let lease = match recording_gate.acquire() {
                    Ok(lease) => lease,
                    Err(error) => {
                        show_transient_overlay(
                            &app,
                            "麦克风正在使用",
                            &error,
                            Duration::from_secs(3),
                        );
                        continue;
                    }
                };
                match CpalRecordingSession::start() {
                    Ok(session) => {
                        recording = Some(ActiveRecording {
                            session,
                            target: capture_foreground_window(),
                            _lease: lease,
                        });
                        show_recording_overlay(&app, &hotkey_label, paused);
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
    hide_overlay(&app);
}

fn finish_dictation(app: &AppHandle, active: ActiveRecording) -> Result<(), OrallyError> {
    show_processing_overlay(app);

    let recorded = active.session.stop()?;
    let config = tray::with_config_lock(|| {
        orally_config::load_or_default().map_err(|error| error.to_string())
    })
    .map_err(OrallyError::InvalidInput)?;
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
    SpeechProcessor::new(build_speech_plan(config)).map_err(OrallyError::from)
}

fn build_speech_plan(config: &AppConfig) -> SpeechPlan {
    SpeechPlan {
        recognition: RecognitionPlan {
            base_url: config.asr.base_url.clone(),
            model: config.asr.model.clone(),
            credential: CredentialSource::new(
                config.asr.api_key.clone(),
                config.asr.api_key_env.clone(),
            ),
            protocol: AsrProtocol::Auto,
            language: config.asr.language.clone(),
        },
        refinement: build_refinement_plan(config),
        locale: config.output.locale.clone(),
    }
}

fn build_refinement_plan(config: &AppConfig) -> RefinementPlan {
    if config.output.raw {
        RefinementPlan::Raw
    } else if matches!(config.postprocess.mode.as_str(), "llm" | "ai") {
        if config.postprocess.models.is_empty() {
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
            RefinementPlan::AiPipeline(
                config
                    .postprocess
                    .models
                    .iter()
                    .filter(|model| model.enabled)
                    .map(|model| AiPostprocessPlan {
                        base_url: model.base_url.clone(),
                        model: model.model.clone(),
                        credential: CredentialSource::new(
                            model.api_key.clone(),
                            model.api_key_env.clone(),
                        ),
                        system_prompt: model.composed_system_prompt(),
                        user_template: model.user_template.clone(),
                        fallback_to_local_basic_cleanup: model.fallback_to_builtin,
                    })
                    .collect(),
            )
        }
    } else {
        RefinementPlan::LocalBasicCleanup
    }
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

fn recording_overlay_status(hotkey_label: &str, paused: bool) -> OverlayStatus {
    OverlayStatus {
        title: "正在录音…".to_string(),
        detail: if paused {
            "快捷键已暂停，请点击停止结束本次语音输入".to_string()
        } else {
            format!("再次按 {hotkey_label} 或点击停止")
        },
        can_stop: true,
    }
}

fn show_recording_overlay(app: &AppHandle, hotkey_label: &str, paused: bool) {
    show_overlay(app, recording_overlay_status(hotkey_label, paused));
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
    let generation = show_overlay(
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
        hide_overlay_if_current(&app, generation);
    });
}

fn show_overlay(app: &AppHandle, status: OverlayStatus) -> u64 {
    let generation = with_overlay_state(OverlayState::reserve);
    let ui_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        if !with_overlay_state(|state| state.apply(generation, Some(status.clone()))) {
            return;
        }
        if let Some(window) = ui_app.get_webview_window("dictation") {
            let _ = window.emit("dictation-level", DictationLevel::default());
            position_overlay(&window);
            let _ = window.show();
            let _ = window.emit("dictation-status", status);
        }
    });
    generation
}

fn hide_overlay(app: &AppHandle) {
    let generation = with_overlay_state(OverlayState::reserve);
    let ui_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        if with_overlay_state(|state| state.apply(generation, None)) {
            hide_overlay_window(&ui_app);
        }
    });
}

fn hide_overlay_if_current(app: &AppHandle, generation: u64) {
    let ui_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        if with_overlay_state(|state| state.dismiss(generation)) {
            hide_overlay_window(&ui_app);
        }
    });
}

fn hide_overlay_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("dictation") {
        let _ = window.emit("dictation-level", DictationLevel::default());
        let _ = window.hide();
    }
}

fn emit_dictation_level(app: &AppHandle, level: DictationLevel) {
    if let Some(window) = app.get_webview_window("dictation") {
        let _ = window.emit("dictation-level", level);
    }
}

fn overlay_geometry(
    work_position: PhysicalPosition<i32>,
    work_size: PhysicalSize<u32>,
    scale_factor: f64,
) -> (PhysicalSize<u32>, PhysicalPosition<i32>) {
    let size = LogicalSize::new(OVERLAY_WIDTH, OVERLAY_HEIGHT).to_physical::<u32>(scale_factor);
    let bottom_margin = (OVERLAY_BOTTOM_MARGIN * scale_factor).round() as u32;
    let x = work_position.x + (work_size.width.saturating_sub(size.width) / 2) as i32;
    let y = work_position.y
        + work_size
            .height
            .saturating_sub(size.height)
            .saturating_sub(bottom_margin) as i32;
    (size, PhysicalPosition::new(x, y))
}

fn position_overlay(window: &WebviewWindow) {
    if let Ok(Some(monitor)) = window.current_monitor() {
        let work_area = monitor.work_area();
        let (size, position) =
            overlay_geometry(work_area.position, work_area.size, monitor.scale_factor());
        let _ = window.set_size(size);
        let _ = window.set_position(position);
    } else {
        let _ = window.set_size(LogicalSize::new(OVERLAY_WIDTH, OVERLAY_HEIGHT));
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
            let recording_gate = Arc::new(test_recording::RecordingGate::default());
            app.manage(test_recording::TestRecordingController::new(
                recording_gate.clone(),
                app.handle().clone(),
            )?);
            let controller = start_dictation_service(app.handle().clone(), recording_gate);
            let tray_sender = controller
                .sender
                .lock()
                .expect("dictation sender mutex should not be poisoned")
                .clone();
            tray::build(app, tray_sender)?;
            app.manage(controller);
            Ok(())
        })
        .on_page_load(|webview, payload| {
            if payload.event() == tauri::webview::PageLoadEvent::Started {
                test_recording::cancel_window(webview.app_handle(), webview.window().label());
                hotkey::restore_window(webview.app_handle(), webview.window().label());
            }
        })
        .on_window_event(|window, event| {
            if matches!(
                event,
                WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed
            ) {
                test_recording::cancel_window(window.app_handle(), window.label());
                hotkey::restore_window(window.app_handle(), window.label());
            }
            if matches!(event, WindowEvent::Focused(false)) {
                hotkey::restore_window(window.app_handle(), window.label());
            }
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            parse_config_document,
            service_test::test_service_connection,
            service_test::test_config_input,
            test_recording::start_test_recording,
            test_recording::stop_test_recording,
            test_recording::cancel_test_recording,
            save_config,
            hotkey::set_hotkey_capture,
            stop_recording,
            replay_dictation_status
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Orally desktop app");
}

#[cfg(test)]
mod tests {
    use super::*;
    use orally_config::{PostprocessModelConfig, PromptNodeConfig};

    #[test]
    fn stop_button_never_starts_capture_and_still_stops_while_hotkey_is_paused() {
        for paused in [false, true] {
            assert_eq!(
                dictation_action(DictationCommand::StopIfRecording, true, paused),
                DictationAction::Stop
            );
            assert_eq!(
                dictation_action(DictationCommand::StopIfRecording, false, paused),
                DictationAction::Ignore
            );
        }
        assert_eq!(
            dictation_action(DictationCommand::ToggleFromHotkey, false, false),
            DictationAction::Start
        );
        assert_eq!(
            dictation_action(DictationCommand::ToggleFromHotkey, true, false),
            DictationAction::Stop
        );
        assert_eq!(
            dictation_action(DictationCommand::ToggleFromHotkey, true, true),
            DictationAction::Ignore
        );
        assert_eq!(
            dictation_action(DictationCommand::ToggleFromUi, false, true),
            DictationAction::Start
        );
    }

    #[test]
    fn recording_status_keeps_stop_available_when_pausing_and_resuming_hotkey() {
        let paused = recording_overlay_status("Ctrl+Alt+Space", true);
        assert!(paused.can_stop);
        assert!(paused.detail.contains("快捷键已暂停"));
        let resumed = recording_overlay_status("Ctrl+Alt+Space", false);
        assert!(resumed.can_stop);
        assert!(resumed.detail.contains("Ctrl+Alt+Space"));
        assert_eq!(paused.title, resumed.title);
    }

    #[test]
    fn an_old_transient_timer_cannot_hide_new_recording_or_processing_status() {
        let mut state = OverlayState::new();
        let transient = state.reserve();
        assert!(state.apply(
            transient,
            Some(OverlayStatus {
                title: "已插入文本".to_string(),
                detail: String::new(),
                can_stop: false,
            })
        ));
        let recording = state.reserve();
        assert!(state.apply(
            recording,
            Some(recording_overlay_status("Ctrl+Alt+Space", false))
        ));
        assert!(!state.dismiss(transient));
        assert!(state.status.as_ref().unwrap().can_stop);
        let processing = state.reserve();
        assert!(state.apply(
            processing,
            Some(OverlayStatus {
                title: "正在整理文字".to_string(),
                detail: String::new(),
                can_stop: false,
            })
        ));
        assert!(!state.dismiss(transient));
        assert!(!state.dismiss(recording));
        assert_eq!(state.status.as_ref().unwrap().title, "正在整理文字");
        assert!(state.dismiss(processing));
        assert!(state.status.is_none());
        assert!(!state.dismiss(processing));
    }

    #[test]
    fn out_of_order_show_or_hide_cannot_replace_a_more_recent_overlay_request() {
        let mut state = OverlayState::new();
        let old_show = state.reserve();
        let hide = state.reserve();
        let new_show = state.reserve();
        assert!(state.apply(
            new_show,
            Some(recording_overlay_status("Ctrl+Alt+Space", false))
        ));
        assert!(!state.apply(
            old_show,
            Some(OverlayStatus {
                title: "过期提示".to_string(),
                detail: String::new(),
                can_stop: false,
            })
        ));
        assert!(!state.apply(hide, None));
        assert!(state.status.as_ref().unwrap().can_stop);
    }

    fn metrics(rms: f32, peak: f32) -> AudioMetrics {
        AudioMetrics {
            duration_ms: 120,
            peak_amplitude: peak,
            rms_amplitude: rms,
            voice_activity_ratio: 0.0,
        }
    }

    #[test]
    fn dictation_levels_preserve_real_amplitude_and_silence() {
        assert_eq!(
            DictationLevel::from_metrics(metrics(0.125, 0.75)),
            DictationLevel {
                rms: 0.125,
                peak: 0.75
            }
        );
        assert_eq!(
            DictationLevel::from_metrics(metrics(0.0, 0.0)),
            DictationLevel::default()
        );
    }

    #[test]
    fn dictation_levels_clamp_pcm_overflow_and_reject_nonfinite_values() {
        assert_eq!(
            DictationLevel::from_metrics(metrics(-0.25, 1.000_031)),
            DictationLevel {
                rms: 0.0,
                peak: 1.0
            }
        );
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                DictationLevel::from_metrics(metrics(value, value)),
                DictationLevel::default()
            );
        }
    }

    #[test]
    fn overlay_geometry_scales_content_and_margin_inside_monitor_work_area() {
        assert_eq!(
            overlay_geometry(
                PhysicalPosition::new(-1920, 32),
                PhysicalSize::new(1920, 1008),
                1.5
            ),
            (
                PhysicalSize::new(450, 132),
                PhysicalPosition::new(-1185, 800)
            )
        );
        assert_eq!(
            overlay_geometry(PhysicalPosition::new(0, 0), PhysicalSize::new(280, 80), 1.0),
            (PhysicalSize::new(300, 88), PhysicalPosition::new(0, 0))
        );
    }

    #[test]
    fn desktop_speech_uses_auto_negotiation_regardless_of_the_stored_protocol() {
        for protocol in ["auto", "chat-audio", "openai-transcriptions", "unsupported"] {
            let mut config = AppConfig::default();
            config.asr.protocol = protocol.to_string();

            assert_eq!(
                build_speech_plan(&config).recognition.protocol,
                AsrProtocol::Auto
            );
            assert_eq!(config.asr.protocol, protocol);
        }
    }

    #[test]
    fn legacy_config_keeps_single_model_refinement() {
        let mut config = AppConfig::default();
        config.postprocess.mode = "llm".to_string();
        config.postprocess.model = "legacy-model".to_string();
        config.postprocess.system_prompt = "legacy instructions".to_string();
        config.postprocess.fallback_to_builtin = false;

        let RefinementPlan::AiPostprocessing(plan) = build_refinement_plan(&config) else {
            panic!("an empty model array should retain the legacy model");
        };

        assert_eq!(plan.model, "legacy-model");
        assert_eq!(plan.system_prompt, "legacy instructions");
        assert!(!plan.fallback_to_local_basic_cleanup);
    }

    #[test]
    fn model_pipeline_preserves_enabled_order_and_composes_each_models_prompts() {
        let mut config = AppConfig::default();
        config.postprocess.mode = "ai".to_string();
        config.postprocess.models = vec![
            PostprocessModelConfig {
                model: "first-model".to_string(),
                base_url: "https://first.example/v1".to_string(),
                api_key: Some("first-key".to_string()),
                api_key_env: "FIRST_KEY".to_string(),
                system_prompt: "base instructions".to_string(),
                user_template: "first: {{transcript}}".to_string(),
                fallback_to_builtin: false,
                prompts: vec![
                    PromptNodeConfig {
                        content: "first prompt".to_string(),
                        ..PromptNodeConfig::default()
                    },
                    PromptNodeConfig {
                        content: "disabled prompt".to_string(),
                        enabled: false,
                        ..PromptNodeConfig::default()
                    },
                    PromptNodeConfig {
                        content: "second prompt".to_string(),
                        ..PromptNodeConfig::default()
                    },
                ],
                ..PostprocessModelConfig::default()
            },
            PostprocessModelConfig {
                model: "disabled-model".to_string(),
                enabled: false,
                ..PostprocessModelConfig::default()
            },
            PostprocessModelConfig {
                model: "last-model".to_string(),
                system_prompt: "last instructions".to_string(),
                ..PostprocessModelConfig::default()
            },
        ];

        let RefinementPlan::AiPipeline(plans) = build_refinement_plan(&config) else {
            panic!("models should select the serial pipeline");
        };

        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].model, "first-model");
        assert_eq!(plans[0].base_url, "https://first.example/v1");
        assert_eq!(
            plans[0].credential,
            CredentialSource::new(Some("first-key".to_string()), "FIRST_KEY")
        );
        assert_eq!(
            plans[0].system_prompt,
            "base instructions\n\nfirst prompt\n\nsecond prompt"
        );
        assert_eq!(plans[0].user_template, "first: {{transcript}}");
        assert!(!plans[0].fallback_to_local_basic_cleanup);
        assert_eq!(plans[1].model, "last-model");
        assert_eq!(plans[1].system_prompt, "last instructions");
    }

    #[test]
    fn disabling_all_models_keeps_an_empty_pipeline() {
        let mut config = AppConfig::default();
        config.postprocess.mode = "llm".to_string();
        config.postprocess.models = vec![PostprocessModelConfig {
            enabled: false,
            ..PostprocessModelConfig::default()
        }];

        assert_eq!(
            build_refinement_plan(&config),
            RefinementPlan::AiPipeline(Vec::new())
        );
    }

    #[test]
    fn raw_output_takes_precedence_and_local_mode_does_not_run_models() {
        let mut config = AppConfig::default();
        config.postprocess.mode = "llm".to_string();
        config.postprocess.models = vec![PostprocessModelConfig::default()];
        config.output.raw = true;

        assert_eq!(build_refinement_plan(&config), RefinementPlan::Raw);

        config.output.raw = false;
        config.postprocess.mode = "builtin".to_string();

        assert_eq!(
            build_refinement_plan(&config),
            RefinementPlan::LocalBasicCleanup
        );
    }

    #[test]
    fn config_import_command_parses_toml_without_saving() {
        assert_eq!(
            parse_config_document(String::new()).unwrap(),
            AppConfig::default()
        );
        assert!(parse_config_document("[invalid".to_string()).is_err());
    }
}
