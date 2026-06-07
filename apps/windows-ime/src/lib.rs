use windows_sys::Win32::Foundation::{CLASS_E_CLASSNOTAVAILABLE, E_NOTIMPL, S_OK};

#[no_mangle]
pub extern "system" fn DllCanUnloadNow() -> i32 {
    S_OK
}

#[no_mangle]
pub extern "system" fn DllGetClassObject(
    _rclsid: *const windows_sys::core::GUID,
    _riid: *const windows_sys::core::GUID,
    _ppv: *mut *mut core::ffi::c_void,
) -> i32 {
    CLASS_E_CLASSNOTAVAILABLE
}

#[no_mangle]
pub extern "system" fn DllRegisterServer() -> i32 {
    // TODO: Register COM server and TSF language profile for per-user development.
    E_NOTIMPL
}

#[no_mangle]
pub extern "system" fn DllUnregisterServer() -> i32 {
    // TODO: Remove COM server and TSF language profile registration.
    E_NOTIMPL
}

#[no_mangle]
pub extern "system" fn DllInstall(_install: i32, _cmd_line: *const u16) -> i32 {
    // TODO: Support regsvr32 /i:user style install hooks if needed.
    E_NOTIMPL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dll_can_unload_now_is_successful_stub() {
        assert_eq!(DllCanUnloadNow(), S_OK);
    }

    #[test]
    fn class_factory_is_not_available_until_com_is_implemented() {
        let mut object = core::ptr::null_mut();
        let result = DllGetClassObject(core::ptr::null(), core::ptr::null(), &mut object);

        assert_eq!(result, CLASS_E_CLASSNOTAVAILABLE);
        assert!(object.is_null());
    }
}
