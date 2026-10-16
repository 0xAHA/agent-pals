//! Minimal HTTPS GET over WinHTTP (built into Windows), for the update check. Follows
//! redirects (GitHub serves release assets from another host) and uses the system proxy.

use std::ffi::c_void;
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Networking::WinHttp::*;

/// Largest response accepted; the MSI is under 1 MB.
const LIMIT: usize = 64 * 1024 * 1024;

struct Handle(*mut c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn check(h: *mut c_void, what: &str) -> Result<Handle, String> {
    if h.is_null() {
        Err(format!("{what}: {}", windows::core::Error::from_win32().message()))
    } else {
        Ok(Handle(h))
    }
}

pub fn get(url: &str, accept: &str) -> Result<Vec<u8>, String> {
    let rest = url.strip_prefix("https://").ok_or_else(|| format!("not an https URL: {url}"))?;
    let (host, path) = match rest.split_once('/') {
        Some((h, p)) => (h, format!("/{p}")),
        None => (rest, "/".to_string()),
    };
    let agent = HSTRING::from(format!("agent-pals/{}", env!("CARGO_PKG_VERSION")));
    let headers = HSTRING::from(format!("Accept: {accept}\r\n"));
    let err = |what: &str, e: windows::core::Error| format!("{what}: {}", e.message());
    unsafe {
        let session = check(
            WinHttpOpen(&agent, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0),
            "open",
        )?;
        let _ = WinHttpSetTimeouts(session.0, 10_000, 10_000, 30_000, 60_000);
        let conn = check(
            WinHttpConnect(session.0, &HSTRING::from(host), INTERNET_DEFAULT_HTTPS_PORT, 0),
            "connect",
        )?;
        let req = check(
            WinHttpOpenRequest(
                conn.0,
                w!("GET"),
                &HSTRING::from(path),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                WINHTTP_FLAG_SECURE,
            ),
            "request",
        )?;
        WinHttpSendRequest(req.0, Some(headers.as_wide()), None, 0, 0, 0).map_err(|e| err("send", e))?;
        WinHttpReceiveResponse(req.0, std::ptr::null_mut()).map_err(|e| err("receive", e))?;

        let mut status = 0u32;
        let mut len = std::mem::size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            req.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut status as *mut u32 as *mut c_void),
            &mut len,
            std::ptr::null_mut(),
        )
        .map_err(|e| err("status", e))?;
        if status != 200 {
            return Err(format!("HTTP {status} from {host}"));
        }

        let mut out = Vec::new();
        loop {
            let mut avail = 0u32;
            WinHttpQueryDataAvailable(req.0, &mut avail).map_err(|e| err("read", e))?;
            if avail == 0 {
                break;
            }
            if out.len() + avail as usize > LIMIT {
                return Err("response too large".into());
            }
            let mut buf = vec![0u8; avail as usize];
            let mut read = 0u32;
            WinHttpReadData(req.0, buf.as_mut_ptr().cast(), avail, &mut read).map_err(|e| err("read", e))?;
            out.extend_from_slice(&buf[..read as usize]);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Network tests: cargo test -- --ignored

    #[test]
    #[ignore]
    fn github_api_json() {
        let body = get("https://api.github.com/repos/rust-lang/rust/releases/latest", "application/vnd.github+json").unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(v["tag_name"].as_str().is_some());
    }

    #[test]
    #[ignore]
    fn follows_cross_host_redirect() {
        // /releases/latest redirects to the tag page, as asset downloads redirect to a CDN host.
        let body = get("https://github.com/rust-lang/rust/releases/latest", "text/html").unwrap();
        assert!(String::from_utf8_lossy(&body).contains("releases/tag/"));
    }

    #[test]
    fn rejects_plain_http() {
        assert!(get("http://example.com/", "text/html").is_err());
    }
}
