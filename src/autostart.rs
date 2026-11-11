//! "Start with Windows" via the per-user Run registry key (no admin needed).

use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegQueryValueExW, RegSetValueExW,
};
use windows::core::{PCWSTR, w};

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const VALUE: PCWSTR = w!("AudioLock");

fn open(access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Option<HKEY> {
    let mut key = HKEY::default();
    let err = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            access,
            None,
            &mut key,
            None,
        )
    };
    err.is_ok().then_some(key)
}

pub fn is_enabled() -> bool {
    let Some(key) = open(KEY_QUERY_VALUE) else {
        return false;
    };
    let found = unsafe { RegQueryValueExW(key, VALUE, None, None, None, None) }.is_ok();
    unsafe {
        let _ = RegCloseKey(key);
    }
    found
}

pub fn set_enabled(enabled: bool) {
    let Some(key) = open(KEY_SET_VALUE) else {
        return;
    };
    if enabled {
        if let Ok(exe) = std::env::current_exe() {
            let cmd = crate::logic::autostart_command(&exe);
            let wide: Vec<u16> = cmd.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes: Vec<u8> = wide.iter().flat_map(|c| c.to_le_bytes()).collect();
            unsafe {
                let _ = RegSetValueExW(key, VALUE, None, REG_SZ, Some(&bytes));
            }
        }
    } else {
        unsafe {
            let _ = RegDeleteValueW(key, VALUE);
        }
    }
    unsafe {
        let _ = RegCloseKey(key);
    }
}
