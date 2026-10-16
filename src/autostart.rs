//! "Start with Windows": the per-user Run key value the installer also writes, so the tray
//! toggle and the installer agree on one switch.

use windows::core::{w, PCWSTR};
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE: PCWSTR = w!("AgentPals");

pub fn enabled() -> bool {
    unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE, RRF_RT_REG_SZ, None, None, None).is_ok() }
}

/// On: start this exe at login. Off: remove the entry.
pub fn set(on: bool) -> Result<(), String> {
    let err = if on {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let cmd: Vec<u16> = format!("\"{}\"", exe.display()).encode_utf16().chain([0]).collect();
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                VALUE,
                REG_SZ.0,
                Some(cmd.as_ptr().cast()),
                (cmd.len() * 2) as u32,
            )
        }
    } else {
        let e = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE) };
        if e == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND {
            return Ok(()); // already off
        }
        e
    };
    if err.is_ok() {
        Ok(())
    } else {
        Err(windows::core::Error::from(err.to_hresult()).message().to_string())
    }
}
