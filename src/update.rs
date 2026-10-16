//! Updates from GitHub Releases. A release carries AgentPals-<ver>.msi and a
//! AgentPals-<ver>.msi.sha256 beside it (the release workflow publishes both).
//!
//! Installing runs the per-user MSI quietly; it stops this app, upgrades in place (settings and
//! hooks kept) and starts the new version, so nothing here needs to restart anything.

use crate::http;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Security::Cryptography::{BCryptHash, BCRYPT_SHA256_ALG_HANDLE};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub const REPO: &str = "0xAHA/agent-pals";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
/// Wakes the UI thread when an UpdateMsg is queued.
pub const WM_APP_UPDATE: u32 = WM_APP + 2;
const FIRST_CHECK: Duration = Duration::from_secs(30);
const EVERY: Duration = Duration::from_secs(6 * 3600);

pub enum UpdateMsg {
    /// `manual`: the user asked, so "up to date" and errors are worth showing.
    Checked { result: Result<Option<Release>, String>, manual: bool },
    InstallStarted(String),
    InstallFailed(String),
}

fn post(tx: &Sender<UpdateMsg>, hwnd: usize, m: UpdateMsg) {
    if tx.send(m).is_ok() {
        unsafe {
            let _ = PostMessageW(HWND(hwnd as _), WM_APP_UPDATE, WPARAM(0), LPARAM(0));
        }
    }
}

/// Background checks: shortly after start, then every few hours, while `enabled`.
pub fn spawn_schedule(tx: Sender<UpdateMsg>, hwnd: usize, enabled: Arc<AtomicBool>) {
    let _ = std::thread::Builder::new().name("update-schedule".into()).spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        loop {
            if enabled.load(Ordering::Relaxed) {
                post(&tx, hwnd, UpdateMsg::Checked { result: check(), manual: false });
            }
            std::thread::sleep(EVERY);
        }
    });
}

pub fn spawn_check(tx: Sender<UpdateMsg>, hwnd: usize) {
    let _ = std::thread::Builder::new().name("update-check".into()).spawn(move || {
        post(&tx, hwnd, UpdateMsg::Checked { result: check(), manual: true });
    });
}

pub fn spawn_install(rel: Release, tx: Sender<UpdateMsg>, hwnd: usize) {
    let _ = std::thread::Builder::new().name("update-install".into()).spawn(move || {
        let m = match install(&rel) {
            Ok(()) => UpdateMsg::InstallStarted(rel.version),
            Err(e) => UpdateMsg::InstallFailed(e),
        };
        post(&tx, hwnd, m);
    });
}

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    msi_url: String,
    sha_url: String,
}

/// "v1.2.3", "1.2.3" or "1.2.3-beta" -> (1, 2, 3).
fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let core = s.trim().trim_start_matches('v').split(['-', '+']).next()?;
    let mut it = core.split('.').map(|n| n.parse::<u32>().ok());
    Some((it.next()??, it.next().unwrap_or(Some(0))?, it.next().unwrap_or(Some(0))?))
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    matches!((parse_version(candidate), parse_version(current)), (Some(a), Some(b)) if a > b)
}

/// The installable release in a GitHub "latest release" response, if it has both assets.
fn parse_release(v: &Value) -> Option<Release> {
    if v["draft"].as_bool() == Some(true) || v["prerelease"].as_bool() == Some(true) {
        return None;
    }
    let tag = v["tag_name"].as_str()?;
    let assets = v["assets"].as_array()?;
    let url_of = |suffix: &str| {
        assets
            .iter()
            .find(|a| a["name"].as_str().is_some_and(|n| n.to_ascii_lowercase().ends_with(suffix)))
            .and_then(|a| a["browser_download_url"].as_str())
            .map(String::from)
    };
    Some(Release {
        version: tag.trim_start_matches('v').to_string(),
        msi_url: url_of(".msi")?,
        sha_url: url_of(".msi.sha256")?,
    })
}

/// Ok(Some) when a newer release is published.
pub fn check() -> Result<Option<Release>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let body = http::get(&url, "application/vnd.github+json")?;
    let v: Value = serde_json::from_slice(&body).map_err(|e| format!("release info: {e}"))?;
    let rel = parse_release(&v).ok_or("latest release has no installer")?;
    Ok(is_newer(&rel.version, CURRENT).then_some(rel))
}

fn sha256_hex(data: &[u8]) -> Result<String, String> {
    let mut out = [0u8; 32];
    unsafe { BCryptHash(BCRYPT_SHA256_ALG_HANDLE, None, data, &mut out) }
        .ok()
        .map_err(|e| format!("sha256: {}", e.message()))?;
    Ok(out.iter().map(|b| format!("{b:02x}")).collect())
}

/// First token of a `sha256sum`-style line, lower-cased.
fn expected_hash(text: &str) -> Option<String> {
    let h = text.split_whitespace().next()?.to_ascii_lowercase();
    (h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit())).then_some(h)
}

/// Downloads, verifies and launches the installer. Returns once msiexec has started; the
/// installer then stops this process.
pub fn install(rel: &Release) -> Result<(), String> {
    let msi = http::get(&rel.msi_url, "application/octet-stream")?;
    let sha = http::get(&rel.sha_url, "application/octet-stream")?;
    let want = expected_hash(&String::from_utf8_lossy(&sha)).ok_or("checksum file is malformed")?;
    let got = sha256_hex(&msi)?;
    if got != want {
        return Err(format!("checksum mismatch for {} (got {got}, expected {want})", rel.version));
    }
    let dir = std::env::temp_dir();
    let path = dir.join(format!("AgentPals-{}.msi", rel.version));
    std::fs::write(&path, &msi).map_err(|e| format!("saving installer: {e}"))?;
    std::process::Command::new("msiexec.exe")
        .arg("/i")
        .arg(&path)
        .arg("/qn")
        .arg("/l*v")
        .arg(dir.join("AgentPals-update.log"))
        .spawn()
        .map_err(|e| format!("starting installer: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn version_order() {
        assert!(is_newer("v0.3.0", "0.2.2"));
        assert!(is_newer("0.10.0", "0.9.9"), "numeric, not string, comparison");
        assert!(is_newer("1.0", "0.9.9"));
        assert!(!is_newer("0.2.2", "0.2.2"));
        assert!(!is_newer("0.2.1", "0.2.2"));
        assert!(!is_newer("nonsense", "0.2.2"));
        assert!(!is_newer("0.3.0-beta", "0.3.0"), "a suffix compares as its release");
    }

    fn release(tag: &str, prerelease: bool, names: &[&str]) -> Value {
        json!({
            "tag_name": tag, "draft": false, "prerelease": prerelease,
            "assets": names.iter().map(|n| json!({
                "name": n, "browser_download_url": format!("https://github.com/{REPO}/releases/download/{tag}/{n}")
            })).collect::<Vec<_>>()
        })
    }

    #[test]
    fn picks_msi_and_checksum() {
        let r = parse_release(&release("v0.3.0", false, &["AgentPals-0.3.0.msi.sha256", "AgentPals-0.3.0.msi"])).unwrap();
        assert_eq!(r.version, "0.3.0");
        assert!(r.msi_url.ends_with("/AgentPals-0.3.0.msi"));
        assert!(r.sha_url.ends_with("/AgentPals-0.3.0.msi.sha256"));
    }

    #[test]
    fn refuses_unverifiable_or_prerelease() {
        assert!(parse_release(&release("v0.3.0", false, &["AgentPals-0.3.0.msi"])).is_none(), "no checksum, no install");
        assert!(parse_release(&release("v0.3.0", true, &["a.msi", "a.msi.sha256"])).is_none());
    }

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(sha256_hex(b"abc").unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn checksum_file_formats() {
        let h = "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD";
        assert_eq!(expected_hash(&format!("{h}  AgentPals-0.3.0.msi\n")).unwrap(), h.to_ascii_lowercase());
        assert!(expected_hash("not-a-hash file.msi").is_none());
        assert!(expected_hash("").is_none());
    }
}
