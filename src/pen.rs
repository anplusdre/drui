// Pen tilt and pressure on Windows: egui carries only a pen's force, so a small window subclass reads the pen's own report
// (`WM_POINTERUPDATE` / `WM_POINTERDOWN` and `GetPointerPenInfo`) into a shared state. The Calligraphy brush turns its nib with the
// pen's azimuth; a stylus without tilt (or another system) leaves everything as it was.
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub struct PenState {
    pub tilt_x: f32,    // degrees, -90..90
    pub tilt_y: f32,
    pub pressure: f32,  // 0..1
    pub at: Instant,
}

impl PenState {
    // the direction the pen leans, degrees (0 = to the right, 90 = toward the user), and how far it leans
    pub fn azimuth(&self) -> f32 {
        self.tilt_y.atan2(self.tilt_x).to_degrees()
    }
    pub fn lean(&self) -> f32 {
        (self.tilt_x * self.tilt_x + self.tilt_y * self.tilt_y).sqrt()
    }
}

static STATE: Mutex<Option<PenState>> = Mutex::new(None);

pub fn report(tilt_x: f32, tilt_y: f32, pressure: f32) {
    if let Ok(mut s) = STATE.lock() {
        *s = Some(PenState { tilt_x, tilt_y, pressure, at: Instant::now() });
    }
}

// the last pen report, if it is no older than `max_age_ms`
pub fn latest(max_age_ms: u64) -> Option<PenState> {
    STATE.lock().ok().and_then(|s| *s).filter(|p| p.at.elapsed() <= Duration::from_millis(max_age_ms))
}

#[cfg(windows)]
mod win {
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::UI::Input::Pointer::{GetPointerPenInfo, GetPointerType, POINTER_PEN_INFO};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{PT_PEN, WM_POINTERDOWN, WM_POINTERUPDATE};

    const PEN_MASK_PRESSURE: u32 = 0x1;
    const PEN_MASK_TILT_X: u32 = 0x4;
    const PEN_MASK_TILT_Y: u32 = 0x8;

    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
        if msg == WM_POINTERUPDATE || msg == WM_POINTERDOWN {
            let id = (wparam & 0xFFFF) as u32;
            let mut kind = 0;
            // SAFETY: plain Win32 calls with pointers to locals
            unsafe {
                if GetPointerType(id, &mut kind) != 0 && kind == PT_PEN {
                    let mut info: POINTER_PEN_INFO = std::mem::zeroed();
                    if GetPointerPenInfo(id, &mut info) != 0 {
                        let tilt = |mask: u32, v: i32| if info.penMask & mask != 0 { v as f32 } else { 0.0 };
                        let pressure = if info.penMask & PEN_MASK_PRESSURE != 0 { info.pressure as f32 / 1024.0 } else { 0.0 };
                        super::report(tilt(PEN_MASK_TILT_X, info.tiltX), tilt(PEN_MASK_TILT_Y, info.tiltY), pressure);
                    }
                }
            }
        }
        // SAFETY: passes the message on unchanged
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }

    pub fn install(hwnd: isize) -> bool {
        // SAFETY: subclasses the app's own window; the procedure lives for the whole run
        unsafe { SetWindowSubclass(hwnd as HWND, Some(proc), 0x6472, 0) != 0 }
    }
}

// starts listening to the pen on this window (Windows only); true when it was installed
pub fn install(hwnd: isize) -> bool {
    #[cfg(windows)]
    {
        win::install(hwnd)
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        false
    }
}
