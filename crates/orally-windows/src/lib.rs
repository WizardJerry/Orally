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
        let normalized = value.trim().to_ascii_lowercase();
        let parts = normalized
            .split(['-', '+'])
            .map(str::trim)
            .collect::<Vec<_>>();
        let (key, modifiers) = parts.split_last()?;
        let key = VirtualKey::parse(key)?;
        let mut parsed = HotkeyModifiers::default();
        for modifier in modifiers {
            let flag = match *modifier {
                "ctrl" | "control" => &mut parsed.ctrl,
                "alt" => &mut parsed.alt,
                "shift" => &mut parsed.shift,
                "win" | "meta" | "windows" => &mut parsed.win,
                _ => return None,
            };
            if *flag {
                return None;
            }
            *flag = true;
        }
        if parsed == HotkeyModifiers::default() && !key.is_function_key() {
            return None;
        }
        Some(Self {
            modifiers: parsed,
            key,
        })
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
        let key = self.key.label();
        parts.push(&key);
        parts.join(" + ")
    }

    pub fn preset(&self) -> String {
        self.label().replace(" + ", "-").to_ascii_lowercase()
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
    Character(char),
    Function(u8),
    Space,
    Enter,
    Tab,
    Escape,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    F9,
    F10,
    F11,
    F12,
}

impl VirtualKey {
    fn parse(value: &str) -> Option<Self> {
        if value.len() == 1 {
            let character = value.chars().next()?;
            if character.is_ascii_alphanumeric() {
                return Some(Self::Character(character.to_ascii_uppercase()));
            }
        }
        if let Some(number) = value
            .strip_prefix('f')
            .and_then(|number| number.parse::<u8>().ok())
        {
            return match number {
                9 => Some(Self::F9),
                10 => Some(Self::F10),
                11 => Some(Self::F11),
                12 => Some(Self::F12),
                1..=24 => Some(Self::Function(number)),
                _ => None,
            };
        }
        match value {
            "space" => Some(Self::Space),
            "enter" | "return" => Some(Self::Enter),
            "tab" => Some(Self::Tab),
            "escape" | "esc" => Some(Self::Escape),
            "backspace" => Some(Self::Backspace),
            "delete" | "del" => Some(Self::Delete),
            "insert" => Some(Self::Insert),
            "home" => Some(Self::Home),
            "end" => Some(Self::End),
            "pageup" => Some(Self::PageUp),
            "pagedown" => Some(Self::PageDown),
            "arrowup" | "up" => Some(Self::ArrowUp),
            "arrowdown" | "down" => Some(Self::ArrowDown),
            "arrowleft" | "left" => Some(Self::ArrowLeft),
            "arrowright" | "right" => Some(Self::ArrowRight),
            _ => None,
        }
    }

    fn is_function_key(self) -> bool {
        matches!(
            self,
            Self::Function(1..=24) | Self::F9 | Self::F10 | Self::F11 | Self::F12
        )
    }

    fn code(self) -> Option<u32> {
        Some(match self {
            Self::Character(character) if character.is_ascii_alphanumeric() => {
                character.to_ascii_uppercase() as u32
            }
            Self::Character(_) | Self::Function(0 | 25..=255) => return None,
            Self::Function(number) => 0x6f + u32::from(number),
            Self::Space => 0x20,
            Self::Enter => 0x0d,
            Self::Tab => 0x09,
            Self::Escape => 0x1b,
            Self::Backspace => 0x08,
            Self::Delete => 0x2e,
            Self::Insert => 0x2d,
            Self::Home => 0x24,
            Self::End => 0x23,
            Self::PageUp => 0x21,
            Self::PageDown => 0x22,
            Self::ArrowUp => 0x26,
            Self::ArrowDown => 0x28,
            Self::ArrowLeft => 0x25,
            Self::ArrowRight => 0x27,
            Self::F9 => 0x78,
            Self::F10 => 0x79,
            Self::F11 => 0x7a,
            Self::F12 => 0x7b,
        })
    }

    fn label(&self) -> String {
        match self {
            Self::Character(character) => character.to_ascii_uppercase().to_string(),
            Self::Function(number) => format!("F{number}"),
            Self::Space => "Space".into(),
            Self::Enter => "Enter".into(),
            Self::Tab => "Tab".into(),
            Self::Escape => "Escape".into(),
            Self::Backspace => "Backspace".into(),
            Self::Delete => "Delete".into(),
            Self::Insert => "Insert".into(),
            Self::Home => "Home".into(),
            Self::End => "End".into(),
            Self::PageUp => "PageUp".into(),
            Self::PageDown => "PageDown".into(),
            Self::ArrowUp => "ArrowUp".into(),
            Self::ArrowDown => "ArrowDown".into(),
            Self::ArrowLeft => "ArrowLeft".into(),
            Self::ArrowRight => "ArrowRight".into(),
            Self::F9 => "F9".into(),
            Self::F10 => "F10".into(),
            Self::F11 => "F11".into(),
            Self::F12 => "F12".into(),
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

pub fn is_hotkey_pressed(hotkey: Hotkey) -> bool {
    platform::is_hotkey_pressed(hotkey)
}

/// A registration belongs to the thread that creates it and unregisters on drop.
/// Keep it on a dedicated message-pump thread, rather than in cross-thread app state.
#[derive(Debug)]
pub struct HotkeyRegistration {
    id: i32,
    _thread_bound: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl HotkeyRegistration {
    pub fn new(id: i32, hotkey: Hotkey) -> Result<Self, OrallyError> {
        if !(1..=0xbfff).contains(&id) || hotkey.key.code().is_none() {
            return Err(OrallyError::InvalidInput(
                "invalid hotkey registration".to_string(),
            ));
        }
        platform::register_hotkey(id, hotkey)?;
        Ok(Self {
            id,
            _thread_bound: std::marker::PhantomData,
        })
    }

    pub fn id(&self) -> i32 {
        self.id
    }
}

impl Drop for HotkeyRegistration {
    fn drop(&mut self) {
        platform::unregister_hotkey(self.id);
    }
}

/// Removes one queued WM_HOTKEY message for the calling registration thread.
pub fn take_hotkey_event() -> Option<i32> {
    platform::take_hotkey_event()
}

#[cfg(windows)]
mod platform {
    use super::{Hotkey, HotkeyEvent};
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
        GetAsyncKeyState, RegisterHotKey, SendInput, UnregisterHotKey, INPUT, INPUT_0,
        INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT,
        MOD_WIN, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT, VK_V,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_HOTKEY,
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
        register_hotkey(ORALLY_HOTKEY_ID, hotkey)?;
        unsafe {
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

    pub fn register_hotkey(id: i32, hotkey: Hotkey) -> Result<(), OrallyError> {
        let key = hotkey
            .key
            .code()
            .ok_or_else(|| OrallyError::InvalidInput("invalid hotkey key".to_string()))?;
        if unsafe { RegisterHotKey(null_mut(), id, hotkey_modifiers(hotkey) | MOD_NOREPEAT, key) }
            == 0
        {
            return Err(OrallyError::Insertion(
                "RegisterHotKey failed; the hotkey may already be in use or reserved by Windows"
                    .to_string(),
            ));
        }
        Ok(())
    }

    pub fn unregister_hotkey(id: i32) {
        unsafe {
            UnregisterHotKey(null_mut(), id);
        }
    }

    pub fn take_hotkey_event() -> Option<i32> {
        let mut message = unsafe { std::mem::zeroed::<MSG>() };
        if unsafe { PeekMessageW(&mut message, null_mut(), WM_HOTKEY, WM_HOTKEY, PM_REMOVE) } == 0 {
            None
        } else {
            Some(message.wParam as i32)
        }
    }

    pub fn is_hotkey_pressed(hotkey: Hotkey) -> bool {
        hotkey.key.code().is_some_and(|key| key_is_down(key as i32))
            && (!hotkey.modifiers.ctrl || key_is_down(i32::from(VK_CONTROL)))
            && (!hotkey.modifiers.alt || key_is_down(i32::from(VK_MENU)))
            && (!hotkey.modifiers.shift || key_is_down(i32::from(VK_SHIFT)))
            && (!hotkey.modifiers.win
                || key_is_down(i32::from(VK_LWIN))
                || key_is_down(i32::from(VK_RWIN)))
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

    fn key_is_down(vk: i32) -> bool {
        unsafe { GetAsyncKeyState(vk) < 0 }
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

    pub fn is_hotkey_pressed(_hotkey: Hotkey) -> bool {
        false
    }

    pub fn register_hotkey(_id: i32, _hotkey: Hotkey) -> Result<(), OrallyError> {
        Err(OrallyError::Insertion(
            "Global hotkeys are only available on Windows".to_string(),
        ))
    }

    pub fn unregister_hotkey(_id: i32) {}

    pub fn take_hotkey_event() -> Option<i32> {
        None
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
        assert_eq!(
            Hotkey::from_preset("ctrl-f13").unwrap().label(),
            "Ctrl + F13"
        );
    }

    #[test]
    fn parses_captured_combinations_and_round_trips_canonical_tokens() {
        for (input, canonical) in [
            (" Win + Shift + Ctrl + a ", "ctrl-shift-win-a"),
            ("alt-0", "alt-0"),
            ("ctrl-arrowleft", "ctrl-arrowleft"),
            ("ctrl-pageup", "ctrl-pageup"),
            ("shift-enter", "shift-enter"),
            ("f1", "f1"),
            ("F24", "f24"),
        ] {
            let hotkey = Hotkey::from_preset(input).unwrap();
            assert_eq!(hotkey.preset(), canonical);
            assert_eq!(Hotkey::from_preset(&hotkey.preset()), Some(hotkey));
        }
        assert_eq!(
            Hotkey::from_preset("win-f24").unwrap().key.code(),
            Some(0x87)
        );
        assert_eq!(
            Hotkey::from_preset("ctrl-z").unwrap().key.code(),
            Some(0x5a)
        );
    }

    #[test]
    fn rejects_bare_typing_keys_modifiers_and_ambiguous_combinations() {
        for invalid in [
            "",
            "a",
            "0",
            "space",
            "enter",
            "ctrl",
            "ctrl-shift",
            "ctrl-ctrl-a",
            "ctrl-a-b",
            "ctrl--a",
            "ctrl-f0",
            "alt-f25",
            "ctrl-中文",
            "ctrl-unknown",
        ] {
            assert!(Hotkey::from_preset(invalid).is_none(), "accepted {invalid}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_registration_detects_conflicts_and_releases_without_sending_keys() {
        let hotkey = Hotkey::from_preset("ctrl-alt-shift-f23").unwrap();
        let first = HotkeyRegistration::new(0x7ffe, hotkey).unwrap();
        let second_thread =
            std::thread::spawn(move || HotkeyRegistration::new(0x7ffd, hotkey).is_err());
        assert!(second_thread.join().unwrap());
        drop(first);
        let replacement = HotkeyRegistration::new(0x7ffe, hotkey).unwrap();
        drop(replacement);
    }
}
