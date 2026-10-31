//! Native notification "pill": a tiny GDI window, not a Windows toast and not a WebView.
//!
//! One thread, one window created once and kept hidden. While nothing is shown the thread
//! sleeps in GetMessage: zero CPU. It never takes focus (WS_EX_NOACTIVATE, MA_NOACTIVATE).

use blackbox_core::model::Severity;
use blackbox_engine::OverlayText;
use std::collections::VecDeque;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_ROUND,
};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const WM_SHOW_NEXT: u32 = WM_APP + 1;
const TIMER_HIDE: usize = 1;
const SHOW_MS: u32 = 6000;
const MAX_QUEUE: usize = 3;

const BG: COLORREF = rgb(11, 11, 11);
const BORDER: COLORREF = rgb(42, 42, 42);
const TITLE: COLORREF = rgb(242, 242, 240);
const BODY: COLORREF = rgb(160, 160, 155);
const LABEL: COLORREF = rgb(95, 95, 92);
const RED: COLORREF = rgb(215, 25, 33);
const AMBER: COLORREF = rgb(232, 163, 61);
const DIM: COLORREF = rgb(60, 60, 58);

const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}

type ClickFn = Arc<dyn Fn() + Send + Sync>;

struct State {
    queue: VecDeque<OverlayText>,
    current: Option<OverlayText>,
    on_click: ClickFn,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();

#[derive(Clone)]
pub struct Overlay {
    thread_id: u32,
}

impl Overlay {
    /// Starts the overlay thread. `on_click` runs on that thread when the pill is clicked.
    pub fn start(on_click: ClickFn) -> Overlay {
        let _ = STATE.set(Mutex::new(State { queue: VecDeque::new(), current: None, on_click }));
        let (tx, rx) = mpsc::channel();
        let _ = std::thread::Builder::new().name("bb-overlay".into()).spawn(move || run(tx));
        Overlay { thread_id: rx.recv().unwrap_or(0) }
    }

    pub fn show(&self, text: OverlayText) {
        if let Some(s) = STATE.get() {
            if let Ok(mut s) = s.lock() {
                if s.queue.len() >= MAX_QUEUE {
                    s.queue.pop_front();
                }
                s.queue.push_back(text);
            }
        }
        if self.thread_id != 0 {
            unsafe { PostThreadMessageW(self.thread_id, WM_SHOW_NEXT, 0, 0) };
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn run(ready: mpsc::Sender<u32>) {
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        // Create the thread's message queue before announcing the thread id.
        PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);
        let _ = ready.send(GetCurrentThreadId());

        let class = wide("BlackboxOverlay");
        let hinstance = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_HAND),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class.as_ptr(),
            wide("Blackbox").as_ptr(),
            WS_POPUP,
            0,
            0,
            360,
            84,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null(),
        );
        if hwnd.is_null() {
            log::warn!("overlay window not created");
            return;
        }
        let corner = DWMWCP_ROUND;
        DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE as u32, &corner as *const _ as *const _, 4);
        let dark: i32 = 1;
        DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE as u32, &dark as *const _ as *const _, 4);
        let border: COLORREF = BORDER;
        DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR as u32, &border as *const _ as *const _, 4);

        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            if msg.hwnd.is_null() && msg.message == WM_SHOW_NEXT {
                show_next(hwnd);
                continue;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// Shows the next queued notification if nothing is on screen.
unsafe fn show_next(hwnd: HWND) {
    let next = {
        let Some(s) = STATE.get() else { return };
        let Ok(mut s) = s.lock() else { return };
        if s.current.is_some() {
            return;
        }
        let Some(n) = s.queue.pop_front() else { return };
        s.current = Some(n.clone());
        n
    };
    let _ = next;
    unsafe {
        let dpi = GetDpiForWindow(hwnd).max(96) as i32;
        let (w, h) = (scale(372, dpi), scale(86, dpi));
        // Bottom-right of the work area of the monitor the user is looking at.
        let mon = MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTOPRIMARY);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(mon, &mut mi);
        let margin = scale(18, dpi);
        let x = mi.rcWork.right - w - margin;
        let y = mi.rcWork.bottom - h - margin;
        SetWindowPos(hwnd, HWND_TOPMOST, x, y, w, h, SWP_NOACTIVATE);
        InvalidateRect(hwnd, std::ptr::null(), 0);
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetTimer(hwnd, TIMER_HIDE, SHOW_MS, None);
    }
}

unsafe fn hide(hwnd: HWND) {
    unsafe {
        KillTimer(hwnd, TIMER_HIDE);
        ShowWindow(hwnd, SW_HIDE);
    }
    if let Some(s) = STATE.get() {
        if let Ok(mut s) = s.lock() {
            s.current = None;
        }
    }
    unsafe { show_next(hwnd) };
}

fn scale(v: i32, dpi: i32) -> i32 {
    v * dpi / 96
}

unsafe fn font(px: i32, weight: u32, face: &str) -> HFONT {
    let f = wide(face);
    unsafe {
        CreateFontW(
            -px,
            0,
            0,
            0,
            weight as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            (DEFAULT_PITCH | FF_DONTCARE) as u32,
            f.as_ptr(),
        )
    }
}

unsafe fn paint(hwnd: HWND) {
    let current = STATE.get().and_then(|s| s.lock().ok().and_then(|s| s.current.clone()));
    unsafe {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut rc: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut rc);
        let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
        // Double buffer: no flicker.
        let mem = CreateCompatibleDC(hdc);
        let bmp = CreateCompatibleBitmap(hdc, w, h);
        let old_bmp = SelectObject(mem, bmp as HGDIOBJ);
        let bg = CreateSolidBrush(BG);
        FillRect(mem, &rc, bg);
        DeleteObject(bg as HGDIOBJ);

        if let Some(n) = current {
            let dpi = GetDpiForWindow(hwnd).max(96) as i32;
            let s = |v: i32| scale(v, dpi);
            let accent = match n.severity {
                Severity::Minor => AMBER,
                _ => RED,
            };
            // 3x3 dot glyph, center dot = severity color.
            let pad = s(18);
            let step = s(8);
            let r = s(2);
            let gy0 = h / 2 - step;
            for gy in 0..3 {
                for gx in 0..3 {
                    let center = gx == 1 && gy == 1;
                    let color = if center { accent } else { DIM };
                    let brush = CreateSolidBrush(color);
                    let pen = CreatePen(PS_SOLID, 1, color);
                    let ob = SelectObject(mem, brush as HGDIOBJ);
                    let op = SelectObject(mem, pen as HGDIOBJ);
                    let (cx, cy) = (pad + r + gx * step, gy0 + gy * step);
                    let rr = if center { r + s(1) } else { r };
                    Ellipse(mem, cx - rr, cy - rr, cx + rr + 1, cy + rr + 1);
                    SelectObject(mem, ob);
                    SelectObject(mem, op);
                    DeleteObject(brush as HGDIOBJ);
                    DeleteObject(pen as HGDIOBJ);
                }
            }
            SetBkMode(mem, TRANSPARENT as i32);
            let text_x = pad + 2 * step + 2 * r + s(16);
            let f_label = font(s(10), FW_NORMAL, "Cascadia Mono");
            let f_title = font(s(14), FW_SEMIBOLD, "Segoe UI Variable Display");
            let f_body = font(s(12), FW_NORMAL, "Segoe UI Variable Text");

            let old_font = SelectObject(mem, f_label as HGDIOBJ);
            SetTextColor(mem, LABEL);
            let mut lr = RECT { left: text_x, top: s(10), right: w - s(14), bottom: s(24) };
            let label = wide("BLACKBOX");
            DrawTextW(mem, label.as_ptr(), -1, &mut lr, DT_RIGHT | DT_SINGLELINE | DT_NOPREFIX);

            SelectObject(mem, f_title as HGDIOBJ);
            SetTextColor(mem, TITLE);
            let mut tr = RECT { left: text_x, top: s(16), right: w - s(14), bottom: s(36) };
            let title = wide(&n.title);
            DrawTextW(mem, title.as_ptr(), -1, &mut tr, DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX);

            SelectObject(mem, f_body as HGDIOBJ);
            SetTextColor(mem, BODY);
            let mut br = RECT { left: text_x, top: s(40), right: w - s(14), bottom: h - s(10) };
            let body = wide(&n.body);
            DrawTextW(
                mem,
                body.as_ptr(),
                -1,
                &mut br,
                DT_LEFT | DT_WORDBREAK | DT_END_ELLIPSIS | DT_EDITCONTROL | DT_NOPREFIX,
            );

            SelectObject(mem, old_font);
            DeleteObject(f_label as HGDIOBJ);
            DeleteObject(f_title as HGDIOBJ);
            DeleteObject(f_body as HGDIOBJ);
        }

        BitBlt(hdc, 0, 0, w, h, mem, 0, 0, SRCCOPY);
        SelectObject(mem, old_bmp);
        DeleteObject(bmp as HGDIOBJ);
        DeleteDC(mem);
        EndPaint(hwnd, &ps);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            unsafe { paint(hwnd) };
            0
        }
        WM_ERASEBKGND => 1,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_TIMER if wparam == TIMER_HIDE => {
            unsafe { hide(hwnd) };
            0
        }
        WM_LBUTTONUP => {
            let cb = STATE.get().and_then(|s| s.lock().ok().map(|s| s.on_click.clone()));
            unsafe { hide(hwnd) };
            if let Some(cb) = cb {
                cb();
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
