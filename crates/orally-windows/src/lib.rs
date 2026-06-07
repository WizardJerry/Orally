use orally_core::{InsertMode, OrallyError, TextInserter};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowsPasteConfig {
    pub paste_delay: Duration,
    pub restore_clipboard: bool,
    pub restore_clipboard_delay: Duration,
}

impl Default for WindowsPasteConfig {
    fn default() -> Self {
        Self {
            paste_delay: Duration::from_millis(750),
            restore_clipboard: true,
            restore_clipboard_delay: Duration::from_millis(250),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsClipboardPasteInserter {
    config: WindowsPasteConfig,
}

impl WindowsClipboardPasteInserter {
    pub fn new(config: WindowsPasteConfig) -> Self {
        Self { config }
    }
}

impl TextInserter for WindowsClipboardPasteInserter {
    fn insert(&self, text: &str, _mode: InsertMode) -> Result<(), OrallyError> {
        thread::sleep(self.config.paste_delay);
        platform::paste_text(
            text,
            self.config.restore_clipboard,
            self.config.restore_clipboard_delay,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    pub modifiers: HotkeyModifiers,
    pub key: VirtualKey,
}

impl Hotkey {
    pub fn ctrl_alt_space() -> Self {
        Self {
            modifiers: HotkeyModifiers {
                ctrl: true,
                alt: true,
                shift: false,
                win: false,
            },
            key: VirtualKey::Space,
        }
    }

    pub fn from_preset(value: &str) -> Option<Self> {
        match value {
            "ctrl-alt-space" => Some(Self::ctrl_alt_space()),
            "ctrl-shift-space" => Some(Self {
                modifiers: HotkeyModifiers {
                    ctrl: true,
                    shift: true,
                    ..HotkeyModifiers::default()
                },
                key: VirtualKey::Space,
            }),
            "alt-space" => Some(Self {
                modifiers: HotkeyModifiers {
                    alt: true,
                    ..HotkeyModifiers::default()
                },
                key: VirtualKey::Space,
            }),
            "f9" => Some(Self::function_key(VirtualKey::F9)),
            "f10" => Some(Self::function_key(VirtualKey::F10)),
            "f11" => Some(Self::function_key(VirtualKey::F11)),
            "f12" => Some(Self::function_key(VirtualKey::F12)),
            "ctrl-alt-f9" => Some(Self::ctrl_alt_function_key(VirtualKey::F9)),
            "ctrl-alt-f10" => Some(Self::ctrl_alt_function_key(VirtualKey::F10)),
            "ctrl-alt-f11" => Some(Self::ctrl_alt_function_key(VirtualKey::F11)),
            "ctrl-alt-f12" => Some(Self::ctrl_alt_function_key(VirtualKey::F12)),
            _ => None,
        }
    }

    pub fn label(&self) -> String {
        let mut parts = Vec::new();
        if self.modifiers.ctrl {
            parts.push("Ctrl");
        }
        if self.modifiers.alt {
            parts.push("Alt");
        }
        if self.modifiers.shift {
            parts.push("Shift");
        }
        if self.modifiers.win {
            parts.push("Win");
        }
        parts.push(self.key.label());
        parts.join(" + ")
    }

    fn function_key(key: VirtualKey) -> Self {
        Self {
            modifiers: HotkeyModifiers::default(),
            key,
        }
    }

    fn ctrl_alt_function_key(key: VirtualKey) -> Self {
        Self {
            modifiers: HotkeyModifiers {
                ctrl: true,
                alt: true,
                ..HotkeyModifiers::default()
            },
            key,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HotkeyModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtualKey {
    Space,
    F9,
    F10,
    F11,
    F12,
}

impl VirtualKey {
    fn label(&self) -> &'static str {
        match self {
            Self::Space => "Space",
            Self::F9 => "F9",
            Self::F10 => "F10",
            Self::F11 => "F11",
            Self::F12 => "F12",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Triggered,
}

pub fn run_hotkey_loop(
    hotkey: Hotkey,
    mut on_event: impl FnMut(HotkeyEvent) -> Result<(), OrallyError>,
) -> Result<(), OrallyError> {
    platform::run_hotkey_loop(hotkey, &mut on_event)
}

#[cfg(windows)]
mod platform {
    use super::{Hotkey, HotkeyEvent, VirtualKey};
    use orally_core::OrallyError;
    use std::mem::size_of;
    use std::ptr::{copy_nonoverlapping, null_mut};
    use std::slice;
    use std::thread;
    use std::time::Duration;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
        OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
    };
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, SendInput, UnregisterHotKey, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
        KEYEVENTF_KEYUP, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, VK_CONTROL, VK_V,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, TranslateMessage, MSG, WM_HOTKEY,
    };

    const ORALLY_HOTKEY_ID: i32 = 1;

    pub fn paste_text(
        text: &str,
        restore_clipboard: bool,
        restore_delay: Duration,
    ) -> Result<(), OrallyError> {
        let previous = if restore_clipboard {
            clipboard_text().ok().flatten()
        } else {
            None
        };

        set_clipboard_text(text)?;
        send_ctrl_v()?;

        if restore_clipboard {
            thread::sleep(restore_delay);
            restore_clipboard_text(previous)?;
        }

        Ok(())
    }

    pub fn run_hotkey_loop(
        hotkey: Hotkey,
        on_event: &mut dyn FnMut(HotkeyEvent) -> Result<(), OrallyError>,
    ) -> Result<(), OrallyError> {
        let modifiers = hotkey_modifiers(hotkey) | MOD_NOREPEAT;
        let key = virtual_key_code(hotkey.key);

        unsafe {
            if RegisterHotKey(null_mut(), ORALLY_HOTKEY_ID, modifiers, key) == 0 {
                return Err(OrallyError::Insertion(
                    "RegisterHotKey failed; the hotkey may already be in use".to_string(),
                ));
            }

            let _guard = HotkeyGuard;
            let mut message = std::mem::zeroed::<MSG>();
            loop {
                let result = GetMessageW(&mut message, null_mut(), 0, 0);
                if result == -1 {
                    return Err(OrallyError::Insertion("GetMessageW failed".to_string()));
                }

                if result == 0 {
                    break;
                }

                if message.message == WM_HOTKEY && message.wParam == ORALLY_HOTKEY_ID as usize {
                    on_event(HotkeyEvent::Triggered)?;
                } else {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        }

        Ok(())
    }

    fn set_clipboard_text(text: &str) -> Result<(), OrallyError> {
        let mut utf16 = text.encode_utf16().collect::<Vec<_>>();
        utf16.push(0);
        let byte_len = utf16.len() * size_of::<u16>();

        unsafe {
            let allocation = GlobalAlloc(GMEM_MOVEABLE, byte_len);
            if allocation.is_null() {
                return Err(OrallyError::Insertion(
                    "GlobalAlloc failed while preparing clipboard text".to_string(),
                ));
            }

            let locked = GlobalLock(allocation);
            if locked.is_null() {
                return Err(OrallyError::Insertion(
                    "GlobalLock failed while preparing clipboard text".to_string(),
                ));
            }

            copy_nonoverlapping(utf16.as_ptr(), locked.cast::<u16>(), utf16.len());
            GlobalUnlock(allocation);

            if OpenClipboard(null_mut()) == 0 {
                return Err(OrallyError::Insertion(
                    "OpenClipboard failed; another app may be using the clipboard".to_string(),
                ));
            }

            let _guard = ClipboardGuard;
            if EmptyClipboard() == 0 {
                return Err(OrallyError::Insertion("EmptyClipboard failed".to_string()));
            }

            let handle = SetClipboardData(u32::from(CF_UNICODETEXT), allocation as HANDLE);
            if handle.is_null() {
                return Err(OrallyError::Insertion(
                    "SetClipboardData failed".to_string(),
                ));
            }
        }

        Ok(())
    }

    fn restore_clipboard_text(text: Option<String>) -> Result<(), OrallyError> {
        match text {
            Some(text) => set_clipboard_text(&text),
            None => clear_clipboard(),
        }
    }

    fn clear_clipboard() -> Result<(), OrallyError> {
        unsafe {
            if OpenClipboard(null_mut()) == 0 {
                return Err(OrallyError::Insertion(
                    "OpenClipboard failed while clearing clipboard".to_string(),
                ));
            }

            let _guard = ClipboardGuard;
            if EmptyClipboard() == 0 {
                return Err(OrallyError::Insertion("EmptyClipboard failed".to_string()));
            }
        }

        Ok(())
    }

    fn clipboard_text() -> Result<Option<String>, OrallyError> {
        unsafe {
            if IsClipboardFormatAvailable(u32::from(CF_UNICODETEXT)) == 0 {
                return Ok(None);
            }

            if OpenClipboard(null_mut()) == 0 {
                return Err(OrallyError::Insertion(
                    "OpenClipboard failed while reading clipboard".to_string(),
                ));
            }

            let _guard = ClipboardGuard;
            let handle = GetClipboardData(u32::from(CF_UNICODETEXT));
            if handle.is_null() {
                return Ok(None);
            }

            let locked = GlobalLock(handle);
            if locked.is_null() {
                return Err(OrallyError::Insertion(
                    "GlobalLock failed while reading clipboard".to_string(),
                ));
            }

            let byte_len = GlobalSize(handle);
            let u16_len = byte_len / size_of::<u16>();
            let data = slice::from_raw_parts(locked.cast::<u16>(), u16_len);
            let nul = data
                .iter()
                .position(|value| *value == 0)
                .unwrap_or(data.len());
            let text = String::from_utf16_lossy(&data[..nul]);
            GlobalUnlock(handle);

            Ok(Some(text))
        }
    }

    fn send_ctrl_v() -> Result<(), OrallyError> {
        let inputs = [
            keyboard_input(VK_CONTROL, 0),
            keyboard_input(VK_V, 0),
            keyboard_input(VK_V, KEYEVENTF_KEYUP),
            keyboard_input(VK_CONTROL, KEYEVENTF_KEYUP),
        ];

        let sent = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                size_of::<INPUT>() as i32,
            )
        };

        if sent != inputs.len() as u32 {
            return Err(OrallyError::Insertion(format!(
                "SendInput sent {sent} of {} keyboard events",
                inputs.len()
            )));
        }

        Ok(())
    }

    fn keyboard_input(vk: u16, flags: u32) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    struct ClipboardGuard;

    impl Drop for ClipboardGuard {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard();
            }
        }
    }

    struct HotkeyGuard;

    impl Drop for HotkeyGuard {
        fn drop(&mut self) {
            unsafe {
                UnregisterHotKey(null_mut(), ORALLY_HOTKEY_ID);
            }
        }
    }

    fn hotkey_modifiers(hotkey: Hotkey) -> u32 {
        let mut modifiers = 0_u32;
        if hotkey.modifiers.ctrl {
            modifiers |= MOD_CONTROL;
        }
        if hotkey.modifiers.alt {
            modifiers |= MOD_ALT;
        }
        if hotkey.modifiers.shift {
            modifiers |= MOD_SHIFT;
        }
        if hotkey.modifiers.win {
            modifiers |= MOD_WIN;
        }

        modifiers
    }

    fn virtual_key_code(key: VirtualKey) -> u32 {
        match key {
            VirtualKey::Space => 0x20,
            VirtualKey::F9 => 0x78,
            VirtualKey::F10 => 0x79,
            VirtualKey::F11 => 0x7A,
            VirtualKey::F12 => 0x7B,
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{Hotkey, HotkeyEvent};
    use orally_core::OrallyError;
    use std::time::Duration;

    pub fn paste_text(
        _text: &str,
        _restore_clipboard: bool,
        _restore_delay: Duration,
    ) -> Result<(), OrallyError> {
        Err(OrallyError::Insertion(
            "Windows clipboard paste insertion is only available on Windows".to_string(),
        ))
    }

    pub fn run_hotkey_loop(
        _hotkey: Hotkey,
        _on_event: &mut dyn FnMut(HotkeyEvent) -> Result<(), OrallyError>,
    ) -> Result<(), OrallyError> {
        Err(OrallyError::Insertion(
            "Windows hotkey listening is only available on Windows".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hotkey_presets() {
        assert_eq!(
            Hotkey::from_preset("ctrl-alt-space").map(|hotkey| hotkey.label()),
            Some("Ctrl + Alt + Space".to_string())
        );
        assert_eq!(
            Hotkey::from_preset("ctrl-alt-f9").map(|hotkey| hotkey.label()),
            Some("Ctrl + Alt + F9".to_string())
        );
        assert_eq!(
            Hotkey::from_preset("f12").map(|hotkey| hotkey.label()),
            Some("F12".to_string())
        );
        assert!(Hotkey::from_preset("ctrl-f13").is_none());
    }
}
