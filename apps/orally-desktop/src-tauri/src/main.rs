#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use orally_asr::{
    ChatAudioAsrConfig, ChatAudioAsrProvider, OpenAiCompatibleAsrConfig,
    OpenAiCompatibleAsrProvider,
};
use orally_audio::CpalRecordingSession;
use orally_config::AppConfig;
use orally_core::{
    AppContext, AsrProvider, BuiltInTextProcessor, DictionaryTerm, InsertMode, OrallyError,
    PostprocessPrompt, ProcessInput, TextInserter, TextProcessor, Transcript,
};
use orally_windows::{run_hotkey_loop, Hotkey, WindowsClipboardPasteInserter, WindowsPasteConfig};
use serde::Serialize;
use std::env;
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
    Toggle,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AsrProtocol {
    Auto,
    OpenAiTranscriptions,
    ChatAudio,
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
fn stop_recording(state: State<'_, DictationController>) -> Result<(), String> {
    state
        .sender
        .lock()
        .map_err(|error| error.to_string())?
        .send(DictationCommand::Toggle)
        .map_err(|error| error.to_string())
}

fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn build_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Open Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Orally", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let mut tray = TrayIconBuilder::with_id("orally-main")
        .tooltip("Orally")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
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

    thread::spawn(move || {
        if let Err(error) = run_hotkey_loop(Hotkey::ctrl_alt_space(), |_| {
            hotkey_sender
                .send(DictationCommand::Toggle)
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

    thread::spawn(move || run_dictation_processor(app, receiver));

    DictationController {
        sender: Mutex::new(sender),
    }
}

fn run_dictation_processor(app: AppHandle, receiver: mpsc::Receiver<DictationCommand>) {
    let mut recording: Option<ActiveRecording> = None;

    while let Ok(command) = receiver.recv() {
        match command {
            DictationCommand::Toggle => {
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
                            show_recording_overlay(&app);
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
    let asr_options = AsrOptions::from_config(&config);
    let asr = build_asr_provider(&asr_options)?;
    let transcript = asr.transcribe(recorded.audio)?;
    let text = final_text(transcript, &config)?;

    restore_foreground_window(active.target);
    let inserter = WindowsClipboardPasteInserter::new(WindowsPasteConfig {
        paste_delay: Duration::from_millis(config.output.paste_delay_ms),
    });
    inserter.insert(&text, InsertMode::ClipboardFallback)?;

    hide_overlay(app);
    Ok(())
}

fn final_text(transcript: Transcript, config: &AppConfig) -> Result<String, OrallyError> {
    if config.output.raw {
        return Ok(transcript.text);
    }

    let processor = BuiltInTextProcessor;
    processor
        .process(ProcessInput {
            transcript,
            context: AppContext {
                locale: config.output.locale.clone(),
                ..AppContext::default()
            },
            prompt: PostprocessPrompt::default(),
            dictionary_terms: default_dictionary(),
        })
        .map(|processed| processed.text)
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

fn show_recording_overlay(app: &AppHandle) {
    show_overlay(
        app,
        OverlayStatus {
            title: "正在语音输入".to_string(),
            detail: "按 Ctrl + Alt + Space 或点击停止".to_string(),
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
            build_tray(app)?;
            let controller = start_dictation_service(app.handle().clone());
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
            stop_recording
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Orally desktop app");
}
