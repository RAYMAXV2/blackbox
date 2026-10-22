//! Foreground window tracking, event-driven (no polling): a WinEvent hook on a tiny thread
//! that sleeps in GetMessage between focus changes.

use crate::util::Clock;
use std::sync::atomic::{AtomicI64, AtomicIsize, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetClassNameW, GetForegroundWindow, GetMessageW, GetShellWindow, GetWindowRect,
    GetWindowThreadProcessId, IsHungAppWindow, TranslateMessage, EVENT_SYSTEM_FOREGROUND, MSG, WINEVENT_OUTOFCONTEXT,
};

#[derive(Debug, Default)]
pub struct Foreground {
    pid: AtomicU32,
    hwnd: AtomicIsize,
    changed_qpc: AtomicI64,
}

static GLOBAL: OnceLock<Arc<Foreground>> = OnceLock::new();

impl Foreground {
    /// Starts the hook thread once per process and returns the shared state.
    pub fn start() -> Arc<Foreground> {
        GLOBAL
            .get_or_init(|| {
                let fg = Arc::new(Foreground::default());
                fg.update(unsafe { GetForegroundWindow() });
                let _ = std::thread::Builder::new().name("bb-foreground".into()).spawn(hook_thread);
                fg
            })
            .clone()
    }

    fn update(&self, hwnd: HWND) {
        let mut pid = 0u32;
        if !hwnd.is_null() {
            unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        }
        self.hwnd.store(hwnd as isize, Ordering::Relaxed);
        self.pid.store(pid, Ordering::Relaxed);
        self.changed_qpc.store(Clock::now_qpc(), Ordering::Relaxed);
    }

    pub fn pid(&self) -> u32 {
        self.pid.load(Ordering::Relaxed)
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd.load(Ordering::Relaxed) as HWND
    }

    /// QPC time of the last focus change.
    pub fn changed_qpc(&self) -> i64 {
        self.changed_qpc.load(Ordering::Relaxed)
    }

    /// The foreground window covers its whole monitor (borderless or exclusive fullscreen).
    pub fn is_fullscreen(&self) -> bool {
        let hwnd = self.hwnd();
        if hwnd.is_null() || hwnd == unsafe { GetShellWindow() } {
            return false;
        }
        unsafe {
            let mut wr: RECT = std::mem::zeroed();
            if GetWindowRect(hwnd, &mut wr) == 0 {
                return false;
            }
            let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            let mut mi: MONITORINFO = std::mem::zeroed();
            mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(mon, &mut mi) == 0 {
                return false;
            }
            let m = mi.rcMonitor;
            wr.left <= m.left && wr.top <= m.top && wr.right >= m.right && wr.bottom >= m.bottom
        }
    }

    /// Windows considers the foreground window hung (no message processed for 5 s).
    pub fn is_hung(&self) -> bool {
        is_window_hung(self.hwnd() as isize)
    }

    /// The foreground window is the "ghost" Windows puts over a hung window: the real app is
    /// still the one that was there before.
    pub fn is_ghost(&self) -> bool {
        let hwnd = self.hwnd();
        if hwnd.is_null() {
            return false;
        }
        let mut buf = [0u16; 16];
        let n = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
        n > 0 && String::from_utf16_lossy(&buf[..n as usize]) == "Ghost"
    }
}

/// `hwnd` (as returned by [`Foreground::hwnd`]) is hung. False for closed windows.
pub fn is_window_hung(hwnd: isize) -> bool {
    hwnd != 0 && unsafe { IsHungAppWindow(hwnd as HWND) } != 0
}

unsafe extern "system" fn on_foreground(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if let Some(fg) = GLOBAL.get() {
        fg.update(hwnd);
    }
}

fn hook_thread() {
    unsafe {
        let hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            std::ptr::null_mut(),
            Some(on_foreground),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        if hook.is_null() {
            log::warn!("foreground hook unavailable");
            return;
        }
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
