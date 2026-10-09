//! Thread-owned global registration with capture suspension and save rollback.

use orally_config::AppConfig;
use orally_windows::{is_hotkey_pressed, take_hotkey_event, Hotkey, HotkeyRegistration};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{Manager, State, WebviewWindow};

trait Registration {
    fn id(&self) -> i32;
}

impl Registration for HotkeyRegistration {
    fn id(&self) -> i32 {
        self.id()
    }
}

trait Registrar {
    type Handle: Registration;
    fn register(&mut self, hotkey: Hotkey) -> Result<Self::Handle, String>;
}

#[derive(Default)]
struct NativeRegistrar {
    next_id: i32,
}

impl Registrar for NativeRegistrar {
    type Handle = HotkeyRegistration;

    fn register(&mut self, hotkey: Hotkey) -> Result<Self::Handle, String> {
        self.next_id = self.next_id % 0xbfff + 1;
        HotkeyRegistration::new(self.next_id, hotkey).map_err(|_| {
            format!(
                "快捷键 {} 已被其他程序占用或由 Windows 保留，请换一个组合。",
                hotkey.label()
            )
        })
    }
}

struct Binding<R: Registrar> {
    registrar: R,
    current: Hotkey,
    registered: Option<R::Handle>,
    capturing: bool,
}

impl<R: Registrar> Binding<R> {
    fn new(registrar: R, current: Hotkey) -> Self {
        Self {
            registrar,
            current,
            registered: None,
            capturing: false,
        }
    }

    fn set_capture(&mut self, active: bool) -> Result<(), String> {
        if active {
            self.registered = None;
            self.capturing = true;
        } else {
            // A restore attempt ends UI capture even if another application has
            // claimed the old key. A subsequent save must be able to bind a new key.
            self.capturing = false;
            if self.registered.is_none() {
                self.registered = Some(self.registrar.register(self.current)?);
            }
        }
        Ok(())
    }

    fn save(
        &mut self,
        hotkey: Hotkey,
        persist: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        // Register the replacement while retaining the previous handle. A failed
        // registration or write cannot remove the user's working global trigger.
        let candidate = if hotkey != self.current || self.registered.is_none() {
            Some(self.registrar.register(hotkey)?)
        } else {
            None
        };
        persist()?;
        if !self.capturing {
            if let Some(candidate) = candidate {
                self.registered = Some(candidate);
            }
        }
        self.current = hotkey;
        Ok(())
    }

    fn accepts_event(&self, id: i32) -> bool {
        self.registered
            .as_ref()
            .is_some_and(|registration| registration.id() == id)
    }
}

#[derive(Default)]
struct TriggerFilter {
    waiting_for_release: bool,
}

impl TriggerFilter {
    fn rebind(&mut self, combination_pressed: bool) {
        self.waiting_for_release = combination_pressed;
    }

    fn allows_trigger(&self) -> bool {
        !self.waiting_for_release
    }

    fn after_poll(&mut self, combination_pressed: bool) {
        if !combination_pressed {
            self.waiting_for_release = false;
        }
    }
}

enum Request {
    Save {
        config: Box<AppConfig>,
        hotkey: Hotkey,
        response: mpsc::Sender<Result<(), String>>,
    },
    Capture {
        active: bool,
        epoch: u64,
        response: Option<mpsc::Sender<Result<(), String>>>,
    },
    Shutdown,
}

pub(super) struct HotkeyController {
    sender: mpsc::Sender<Request>,
    epoch: Arc<AtomicU64>,
    current: Arc<Mutex<Hotkey>>,
}

impl HotkeyController {
    pub(super) fn new(
        current: Hotkey,
        mut on_trigger: impl FnMut() + Send + 'static,
        mut on_update: impl FnMut() + Send + 'static,
        mut on_error: impl FnMut(String) + Send + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let epoch = Arc::new(AtomicU64::new(0));
        let shared_current = Arc::new(Mutex::new(current));
        let worker_epoch = epoch.clone();
        let worker_current = shared_current.clone();
        thread::spawn(move || {
            let mut binding = Binding::new(NativeRegistrar::default(), current);
            if let Err(error) = binding.set_capture(false) {
                on_error(error);
            }
            let mut trigger_filter = TriggerFilter::default();
            trigger_filter.rebind(is_hotkey_pressed(current));
            loop {
                match receiver.recv_timeout(Duration::from_millis(20)) {
                    Ok(Request::Save {
                        config,
                        hotkey,
                        response,
                    }) => {
                        let result = binding.save(hotkey, || {
                            super::tray::with_config_lock(|| {
                                let path = orally_config::config_path()
                                    .map_err(|error| error.to_string())?;
                                orally_config::save_to_path(path, &config)
                                    .map_err(|error| error.to_string())
                            })
                        });
                        if result.is_ok() {
                            *worker_current
                                .lock()
                                .unwrap_or_else(|error| error.into_inner()) = hotkey;
                            on_update();
                            trigger_filter.rebind(is_hotkey_pressed(hotkey));
                        }
                        let _ = response.send(result);
                    }
                    Ok(Request::Capture {
                        active,
                        epoch,
                        response,
                    }) => {
                        let result = if active && epoch != worker_epoch.load(Ordering::SeqCst) {
                            Err("快捷键录入已取消，请重新点击录入。".to_string())
                        } else {
                            binding.set_capture(active)
                        };
                        if !active && result.is_ok() {
                            trigger_filter.rebind(is_hotkey_pressed(binding.current));
                        }
                        match response {
                            Some(response) => {
                                let _ = response.send(result);
                            }
                            None => {
                                if let Err(error) = result {
                                    on_error(error);
                                }
                            }
                        }
                    }
                    Ok(Request::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                while let Some(id) = take_hotkey_event() {
                    if binding.accepts_event(id) && trigger_filter.allows_trigger() {
                        on_trigger();
                    }
                }
                // Drain messages generated by the held capture key before ending
                // suppression, including messages delivered just after key release.
                trigger_filter.after_poll(is_hotkey_pressed(binding.current));
            }
        });
        Self {
            sender,
            epoch,
            current: shared_current,
        }
    }

    pub(super) fn current(&self) -> Arc<Mutex<Hotkey>> {
        self.current.clone()
    }

    pub(super) fn save(&self, mut config: AppConfig) -> Result<(), String> {
        let hotkey = Hotkey::from_preset(&config.hotkey.preset).ok_or_else(|| {
            "快捷键无效：请选择修饰键加字母、数字或常用按键，或 F1–F24。".to_string()
        })?;
        config.hotkey.preset = hotkey.preset();
        let (response, receiver) = mpsc::channel();
        self.sender
            .send(Request::Save {
                config: Box::new(config),
                hotkey,
                response,
            })
            .map_err(|_| "快捷键服务暂时不可用，请重启 Orally。".to_string())?;
        receiver
            .recv()
            .map_err(|_| "快捷键服务未能完成保存，请重试。".to_string())?
    }

    fn set_capture(&self, active: bool, observed_epoch: u64) -> Result<(), String> {
        let epoch = if active {
            observed_epoch
        } else {
            self.epoch.fetch_add(1, Ordering::SeqCst) + 1
        };
        let (response, receiver) = mpsc::channel();
        self.sender
            .send(Request::Capture {
                active,
                epoch,
                response: Some(response),
            })
            .map_err(|_| "快捷键服务暂时不可用，请重启 Orally。".to_string())?;
        receiver
            .recv()
            .map_err(|_| "快捷键服务未响应，请重新打开设置。".to_string())?
    }

    fn restore(&self) {
        let epoch = self.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = self.sender.send(Request::Capture {
            active: false,
            epoch,
            response: None,
        });
    }
}

impl Drop for HotkeyController {
    fn drop(&mut self) {
        let _ = self.sender.send(Request::Shutdown);
    }
}

#[tauri::command]
pub(super) fn set_hotkey_capture(
    window: WebviewWindow,
    state: State<'_, HotkeyController>,
    active: bool,
) -> Result<(), String> {
    // Capture before checking focus: lifecycle cancellation must invalidate an
    // already pending invocation even when it reaches the worker afterwards.
    let epoch = state.epoch.load(Ordering::SeqCst);
    if window.label() != "main" {
        return Err("此命令仅用于设置窗口。".to_string());
    }
    if active && !window.is_focused().unwrap_or(false) {
        return Err("设置窗口已失去焦点，请重新点击录入快捷键。".to_string());
    }
    state.set_capture(active, epoch)
}

pub(super) fn restore_window(app: &tauri::AppHandle, label: &str) {
    if label == "main" {
        if let Some(controller) = app.try_state::<HotkeyController>() {
            controller.restore();
        }
    }
}

#[cfg(test)]
mod tests;
