use crate::config::Placement;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use windows::core::{w, Result};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const WM_APP_NUDGE: u32 = WM_APP + 2;
static REPOSITION: AtomicBool = AtomicBool::new(false);

pub fn take_reposition() -> bool {
    REPOSITION.swap(false, Ordering::Relaxed)
}

/// Borderless, click-through, per-pixel-alpha window parked above the tray.
pub struct Overlay {
    pub hwnd: HWND,
    mem_dc: HDC,
    bmp: HBITMAP,
    stock: HGDIOBJ,
    bits: *mut u8,
    w: i32,
    h: i32,
    x: i32,
    y: i32,
    shown: bool,
    last_topmost: Instant,
    /// Set before calling `reposition` (directly or via `resize`).
    pub placement: Placement,
}

impl Overlay {
    pub fn create(w: i32, h: i32, margin: i32, placement: Placement) -> Result<Self> {
        unsafe {
            let hinst: HINSTANCE = GetModuleHandleW(None)?.into();
            let class = w!("AgentPalsOverlay");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: hinst,
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassW(&wc);
            let ex = WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE;
            let hwnd = CreateWindowExW(ex, class, w!("Agent Pals"), WS_POPUP, 0, 0, w, h, None, None, hinst, None)?;
            let mut o = Overlay {
                hwnd,
                mem_dc: CreateCompatibleDC(None),
                bmp: HBITMAP::default(),
                stock: HGDIOBJ::default(),
                bits: std::ptr::null_mut(),
                w: 0,
                h: 0,
                x: 0,
                y: 0,
                shown: false,
                last_topmost: Instant::now(),
                placement,
            };
            o.resize(w, h, margin)?;
            Ok(o)
        }
    }

    pub fn hwnd_raw(&self) -> usize {
        self.hwnd.0 as usize
    }

    pub fn resize(&mut self, w: i32, h: i32, margin: i32) -> Result<()> {
        unsafe {
            if !self.bmp.is_invalid() {
                SelectObject(self.mem_dc, self.stock);
                let _ = DeleteObject(self.bmp);
            }
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h, // top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut c_void = std::ptr::null_mut();
            let bmp = CreateDIBSection(self.mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0)?;
            self.stock = SelectObject(self.mem_dc, bmp);
            self.bmp = bmp;
            self.bits = bits as *mut u8;
            self.w = w;
            self.h = h;
        }
        self.reposition(margin);
        Ok(())
    }

    /// Bottom of the primary work area, i.e. just above a bottom taskbar: at its right end
    /// (by the tray), its left end, or centred.
    pub fn reposition(&mut self, margin: i32) {
        let mut wa = RECT::default();
        unsafe {
            let _ = SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some(&mut wa as *mut RECT as *mut c_void),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            );
        }
        self.x = match self.placement {
            Placement::Right => wa.right - self.w - margin,
            Placement::Left => wa.left + margin,
            Placement::Center => wa.left + (wa.right - wa.left - self.w) / 2,
        };
        self.y = wa.bottom - self.h;
    }

    /// `px` must be BGRA premultiplied, w*h*4 bytes.
    pub fn present(&mut self, px: &[u8]) {
        let len = (self.w * self.h * 4) as usize;
        if px.len() < len || self.bits.is_null() {
            return;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(px.as_ptr(), self.bits, len);
            let screen = GetDC(None);
            let dst = POINT { x: self.x, y: self.y };
            let size = SIZE { cx: self.w, cy: self.h };
            let src = POINT::default();
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let _ = UpdateLayeredWindow(
                self.hwnd,
                screen,
                Some(&dst),
                Some(&size),
                self.mem_dc,
                Some(&src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            ReleaseDC(None, screen);
            if !self.shown {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                self.shown = true;
            }
            // Re-assert topmost occasionally; other topmost windows like to shove in front.
            if self.last_topmost.elapsed() > Duration::from_secs(2) {
                let _ = SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                self.last_topmost = Instant::now();
            }
        }
    }

    pub fn hide(&mut self) {
        if self.shown {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            self.shown = false;
        }
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if matches!(msg, WM_DISPLAYCHANGE | WM_SETTINGCHANGE | WM_DPICHANGED) {
            // Taskbar moved / resolution changed: let the main loop re-park us.
            REPOSITION.store(true, Ordering::Relaxed);
            let _ = PostMessageW(hwnd, WM_APP_NUDGE, WPARAM(0), LPARAM(0));
        }
        DefWindowProcW(hwnd, msg, wp, lp)
    }
}
