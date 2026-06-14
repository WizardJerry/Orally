#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use orally_asr::{
    ChatAudioAsrConfig, ChatAudioAsrProvider, OpenAiCompatibleAsrConfig,
    OpenAiCompatibleAsrProvider,
};
use orally_audio::{CpalRecordingSession, VoiceActivityConfig};
use orally_config::AppConfig;
use orally_core::{
    AppContext, AsrProvider, BuiltInTextProcessor, DictionaryTerm, InsertMode, OrallyError,
    ProcessInput, ProcessedText, TextInserter, TextProcessor, Transcript,
};
use orally_llm::{OpenAiChatPostprocessor, OpenAiChatPostprocessorConfig};
use orally_storage::{default_history_path, HistoryEntry, HistoryStore};
use orally_windows::{
    is_hotkey_pressed, run_hotkey_loop, Hotkey, WindowsClipboardPasteInserter, WindowsPasteConfig,
};
use serde::Serialize;
use std::env;
use std::sync::{
    mpsc::{self, RecvTimeoutError},
    Mutex,
};
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
    options: DictationOptions,
}

#[derive(Debug, Clone, Copy)]
struct ForegroundWindow(isize);

#[derive(Debug, Clone, Serialize)]
struct OverlayStatus {
    title: String,
    detail: String,
    can_stop: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AsrProtocol {
    Auto,
    OpenAiTranscriptions,
    ChatAudio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputMode {
    Toggle,
    Hold,
    FixedWindow,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct DictationOptions {
    input_mode: InputMode,
    fixed_duration: Duration,
    auto_stop_enabled: bool,
    min_record_duration: Duration,
    max_record_duration: Duration,
    silence_timeout: Duration,
    silence_threshold: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StopReason {
    User,
    Released,
    Silence,
    FixedWindow,
    MaxDuration,
}

#[derive(Debug, Clone)]
struct AsrOptions {
    base_url: String,
    model: String,
    api_key: Option<String>,
    api_key_env: String,
    protocol: AsrProtocol,
    language: Option<String>,
    prompt: Option<String>,
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
    let processor_hotkey = hotkey;
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

    thread::spawn(move || run_dictation_processor(app, receiver, processor_hotkey, hotkey_label));

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

fn load_dictation_options() -> DictationOptions {
    orally_config::load_or_default()
        .map(|config| DictationOptions::from_config(&config))
        .unwrap_or_default()
}

fn run_dictation_processor(
    app: AppHandle,
    receiver: mpsc::Receiver<DictationCommand>,
    hotkey: Hotkey,
    hotkey_label: String,
) {
    let mut recording: Option<ActiveRecording> = None;
    let mut paused = false;

    loop {
        match receiver.recv_timeout(Duration::from_millis(120)) {
            Ok(command) => match command {
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
                        if let Err(error) = finish_dictation(&app, active, StopReason::User) {
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
                                let options = load_dictation_options();
                                recording = Some(ActiveRecording {
                                    session,
                                    target: capture_foreground_window(),
                                    options,
                                });
                                show_recording_overlay(&app, &hotkey_label, options);
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
            },
            Err(RecvTimeoutError::Timeout) => {
                let stop_reason = recording
                    .as_ref()
                    .and_then(|active| auto_stop_reason(active, hotkey));
                if let Some(reason) = stop_reason {
                    if let Some(active) = recording.take() {
                        hide_overlay(&app);
                        if let Err(error) = finish_dictation(&app, active, reason) {
                            show_transient_overlay(
                                &app,
                                "语音输入失败",
                                &format!("{error}"),
                                Duration::from_secs(5),
                            );
                        }
                        while receiver.try_recv().is_ok() {}
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn auto_stop_reason(active: &ActiveRecording, hotkey: Hotkey) -> Option<StopReason> {
    let elapsed = active.session.elapsed();
    let options = active.options;

    if elapsed >= options.max_record_duration {
        return Some(StopReason::MaxDuration);
    }

    match options.input_mode {
        InputMode::Hold if elapsed >= options.min_record_duration && !is_hotkey_pressed(hotkey) => {
            return Some(StopReason::Released);
        }
        InputMode::FixedWindow if elapsed >= options.fixed_duration => {
            return Some(StopReason::FixedWindow);
        }
        _ => {}
    }

    if !options.auto_stop_enabled || elapsed < options.min_record_duration {
        return None;
    }

    let metrics = active
        .session
        .recent_metrics(
            options.silence_timeout,
            VoiceActivityConfig {
                sample_threshold: options.silence_threshold,
            },
        )
        .ok()?;
    let enough_window = u128::from(metrics.duration_ms) * 4
        >= options.silence_timeout.as_millis().saturating_mul(3);
    if enough_window
        && metrics.voice_activity_ratio <= 0.01
        && metrics.rms_amplitude < options.silence_threshold
    {
        Some(StopReason::Silence)
    } else {
        None
    }
}

fn finish_dictation(
    app: &AppHandle,
    active: ActiveRecording,
    stop_reason: StopReason,
) -> Result<(), OrallyError> {
    show_processing_overlay(app, stop_reason);

    let recorded = active.session.stop()?;
    let config = orally_config::load_or_default()
        .map_err(|error| OrallyError::InvalidInput(error.to_string()))?;
    if !config.privacy.allow_external_requests {
        return Err(OrallyError::InvalidInput(
            "external requests are disabled in privacy settings".to_string(),
        ));
    }

    let asr_options = AsrOptions::from_config(&config);
    let asr = build_asr_provider(&asr_options)?;
    let transcript = asr.transcribe(recorded.audio)?;
    let raw_text = transcript.text.clone();
    let processed = process_text(transcript, &config)?;

    restore_foreground_window(active.target);
    let inserter = WindowsClipboardPasteInserter::new(WindowsPasteConfig {
        paste_delay: Duration::from_millis(config.output.paste_delay_ms),
        restore_clipboard: config.output.restore_clipboard,
        restore_clipboard_delay: Duration::from_millis(config.output.restore_clipboard_delay_ms),
    });
    inserter.insert(&processed.text, InsertMode::ClipboardFallback)?;

    if let Err(error) = save_history(&config, raw_text, processed.text) {
        show_transient_overlay(
            app,
            "已插入文本",
            &format!("历史保存失败：{error}"),
            Duration::from_secs(4),
        );
    } else {
        show_transient_overlay(
            app,
            "已插入文本",
            completion_detail(stop_reason),
            Duration::from_secs(2),
        );
    }
    Ok(())
}

impl Default for DictationOptions {
    fn default() -> Self {
        Self {
            input_mode: InputMode::Toggle,
            fixed_duration: Duration::from_secs(3),
            auto_stop_enabled: true,
            min_record_duration: Duration::from_millis(450),
            max_record_duration: Duration::from_secs(120),
            silence_timeout: Duration::from_millis(1_200),
            silence_threshold: 0.02,
        }
    }
}

impl DictationOptions {
    fn from_config(config: &AppConfig) -> Self {
        let mut options = Self::default();
        options.input_mode = InputMode::parse(&config.audio.input_mode);
        options.fixed_duration = Duration::from_secs(config.audio.dictate_seconds.max(1));
        options.auto_stop_enabled = config.audio.auto_stop_enabled;
        options.min_record_duration = Duration::from_millis(config.audio.min_record_ms);
        options.max_record_duration = Duration::from_millis(
            config
                .audio
                .max_record_ms
                .max(config.audio.min_record_ms.saturating_add(1)),
        );
        options.silence_timeout = Duration::from_millis(config.audio.silence_timeout_ms.max(250));
        options.silence_threshold = config.audio.silence_threshold.clamp(0.001, 1.0);
        options
    }
}

impl InputMode {
    fn parse(value: &str) -> Self {
        match value {
            "hold" | "push-to-talk" | "push_to_talk" => Self::Hold,
            "fixed" | "fixed-window" | "fixed_window" => Self::FixedWindow,
            _ => Self::Toggle,
        }
    }
}

fn process_text(transcript: Transcript, config: &AppConfig) -> Result<ProcessedText, OrallyError> {
    if config.output.raw {
        return Ok(ProcessedText {
            text: transcript.text,
            changes: Vec::new(),
        });
    }

    let input = ProcessInput {
        transcript,
        context: AppContext {
            locale: config.output.locale.clone(),
            ..AppContext::default()
        },
        prompt: Default::default(),
        dictionary_terms: default_dictionary(),
    };

    if matches!(config.postprocess.mode.as_str(), "llm" | "ai") {
        let api_key = config
            .postprocess
            .api_key
            .clone()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| env::var(&config.postprocess.api_key_env).ok())
            .ok_or_else(|| {
                OrallyError::InvalidInput(format!(
                    "missing postprocess API key: set postprocess.api_key or env var {}",
                    config.postprocess.api_key_env
                ))
            })?;
        let mut llm_config = OpenAiChatPostprocessorConfig::new(
            config.postprocess.base_url.clone(),
            api_key,
            config.postprocess.model.clone(),
        );
        llm_config.system_prompt = config.postprocess.system_prompt.clone();
        llm_config.user_template = config.postprocess.user_template.clone();

        match OpenAiChatPostprocessor::new(llm_config)?.process(input.clone()) {
            Ok(processed) => return Ok(processed),
            Err(error) if config.postprocess.fallback_to_builtin => {
                let mut processed = BuiltInTextProcessor.process(input)?;
                processed.changes.push(format!(
                    "AI postprocessor failed; used built-in cleanup: {error}"
                ));
                return Ok(processed);
            }
            Err(error) => return Err(error),
        }
    }

    let processor = BuiltInTextProcessor;
    processor.process(input)
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

fn build_asr_provider(options: &AsrOptions) -> Result<Box<dyn AsrProvider>, OrallyError> {
    let api_key = options
        .api_key
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| env::var(&options.api_key_env).ok())
        .ok_or_else(|| {
            OrallyError::InvalidInput(format!(
                "missing API key: set asr.api_key or env var {}",
                options.api_key_env
            ))
        })?;

    match options.resolved_protocol() {
        AsrProtocol::OpenAiTranscriptions => {
            let mut config = OpenAiCompatibleAsrConfig::new(
                options.base_url.clone(),
                api_key,
                options.model.clone(),
            );
            config.language = options.language.clone();
            config.prompt = options.prompt.clone();

            OpenAiCompatibleAsrProvider::new(config)
                .map(|provider| Box::new(provider) as Box<dyn AsrProvider>)
        }
        AsrProtocol::ChatAudio => {
            let mut config =
                ChatAudioAsrConfig::new(options.base_url.clone(), api_key, options.model.clone());
            config.language = options.language.clone();
            config.prompt = options.prompt.clone();

            ChatAudioAsrProvider::new(config)
                .map(|provider| Box::new(provider) as Box<dyn AsrProvider>)
        }
        AsrProtocol::Auto => unreachable!("auto protocol should resolve before provider build"),
    }
}

impl AsrOptions {
    fn from_config(config: &AppConfig) -> Self {
        Self {
            base_url: config.asr.base_url.clone(),
            model: config.asr.model.clone(),
            api_key: config.asr.api_key.clone(),
            api_key_env: config.asr.api_key_env.clone(),
            protocol: AsrProtocol::parse(&config.asr.protocol).unwrap_or(AsrProtocol::Auto),
            language: config.asr.language.clone(),
            prompt: config.asr.prompt.clone(),
        }
    }

    fn resolved_protocol(&self) -> AsrProtocol {
        if self.protocol != AsrProtocol::Auto {
            return self.protocol;
        }

        let base_url = self.base_url.to_ascii_lowercase();
        let model = self.model.to_ascii_lowercase();
        if base_url.contains("openrouter.ai")
            || base_url.contains("dashscope.aliyuncs.com/compatible-mode")
            || model.contains("qwen3-asr")
        {
            AsrProtocol::ChatAudio
        } else {
            AsrProtocol::OpenAiTranscriptions
        }
    }
}

impl AsrProtocol {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "auto" => Ok(Self::Auto),
            "openai-transcriptions" | "multipart" => Ok(Self::OpenAiTranscriptions),
            "chat-audio" | "chat-completions" => Ok(Self::ChatAudio),
            other => Err(format!("unknown ASR protocol: {other}")),
        }
    }
}

fn default_dictionary() -> Vec<DictionaryTerm> {
    vec![
        DictionaryTerm {
            spoken: "visual studio code".to_string(),
            written: "Visual Studio Code".to_string(),
        },
        DictionaryTerm {
            spoken: "orally".to_string(),
            written: "Orally".to_string(),
        },
    ]
}

fn show_recording_overlay(app: &AppHandle, hotkey_label: &str, options: DictationOptions) {
    let detail = match options.input_mode {
        InputMode::Toggle => format!("按 {hotkey_label} 或点击停止"),
        InputMode::Hold => format!("按住 {hotkey_label} 说话，松开后自动整理"),
        InputMode::FixedWindow => format!(
            "正在录音 {} 秒，可点击停止提前整理",
            options.fixed_duration.as_secs()
        ),
    };
    show_overlay(
        app,
        OverlayStatus {
            title: "正在语音输入".to_string(),
            detail,
            can_stop: true,
        },
    );
}

fn show_processing_overlay(app: &AppHandle, stop_reason: StopReason) {
    show_overlay(
        app,
        OverlayStatus {
            title: "正在整理文字".to_string(),
            detail: processing_detail(stop_reason).to_string(),
            can_stop: false,
        },
    );
}

fn processing_detail(stop_reason: StopReason) -> &'static str {
    match stop_reason {
        StopReason::User => "正在识别并后处理语音内容",
        StopReason::Released => "已松开热键，正在整理语音内容",
        StopReason::Silence => "检测到停顿，正在整理语音内容",
        StopReason::FixedWindow => "固定录音时长结束，正在整理语音内容",
        StopReason::MaxDuration => "已到最大录音时长，正在整理语音内容",
    }
}

fn completion_detail(stop_reason: StopReason) -> &'static str {
    match stop_reason {
        StopReason::User => "语音输入完成",
        StopReason::Released => "已按住说话并完成插入",
        StopReason::Silence => "静音判停后已完成插入",
        StopReason::FixedWindow => "固定时长录音已完成插入",
        StopReason::MaxDuration => "最大时长保护后已完成插入",
    }
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
