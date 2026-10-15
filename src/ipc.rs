use crate::config::log;
use serde::Deserialize;
use serde_json::Value;
use std::sync::mpsc::Sender;
use std::time::Duration;
use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::Storage::FileSystem::{ReadFile, PIPE_ACCESS_INBOUND};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub const WM_APP_EVENT: u32 = WM_APP + 1;
const MAX_MSG: usize = 256 * 1024;

#[derive(Deserialize)]
pub struct Envelope {
    pub agent: String,
    #[serde(default)]
    pub payload: Value,
}

/// Blocking pipe server on its own thread; wakes the UI thread with WM_APP_EVENT.
pub fn spawn_server(tx: Sender<Envelope>, hwnd_raw: usize) {
    std::thread::Builder::new()
        .name("pipe-server".into())
        .spawn(move || loop {
            let pipe = unsafe {
                CreateNamedPipeW(
                    w!("\\\\.\\pipe\\agent-pals"),
                    PIPE_ACCESS_INBOUND,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                    PIPE_UNLIMITED_INSTANCES,
                    0,
                    64 * 1024,
                    0,
                    None,
                )
            };
            if pipe.is_invalid() {
                log("pipe: create failed, retrying");
                std::thread::sleep(Duration::from_secs(2));
                continue;
            }
            // Err(ERROR_PIPE_CONNECTED) just means the client beat us to it; reading still works.
            let _ = unsafe { ConnectNamedPipe(pipe, None) };
            let data = read_all(pipe);
            unsafe {
                let _ = DisconnectNamedPipe(pipe);
                let _ = CloseHandle(pipe);
            }
            if data.is_empty() {
                continue;
            }
            match serde_json::from_slice::<Envelope>(&data) {
                Ok(env) => {
                    if tx.send(env).is_err() {
                        return;
                    }
                    unsafe {
                        let _ = PostMessageW(HWND(hwnd_raw as _), WM_APP_EVENT, WPARAM(0), LPARAM(0));
                    }
                }
                Err(e) => log(format!("pipe: bad message: {e}")),
            }
        })
        .expect("spawn pipe thread");
}

fn read_all(pipe: HANDLE) -> Vec<u8> {
    let mut data = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let mut n = 0u32;
        let ok = unsafe { ReadFile(pipe, Some(&mut buf), Some(&mut n), None) };
        if ok.is_err() || n == 0 {
            break;
        }
        if data.len() + n as usize > MAX_MSG {
            log("pipe: message too large, dropped");
            return Vec::new();
        }
        data.extend_from_slice(&buf[..n as usize]);
    }
    data
}
