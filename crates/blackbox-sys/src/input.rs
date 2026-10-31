//! What the player is doing, read from outside the game: system cursor visibility and
//! confinement, time since the last keyboard/mouse input, controller activity.
//! No hooks, no injection: a handful of cheap calls, once per second, only during a game.

use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::System::SystemInformation::GetTickCount;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows_sys::Win32::UI::Input::XboxController::{XInputGetState, XINPUT_STATE};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetClipCursor, GetCursorInfo, GetSystemMetrics, GetWindowRect, CURSORINFO, CURSOR_SHOWING, SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

const ERROR_DEVICE_NOT_CONNECTED: u32 = 1167;
/// Disconnected controller slots are slow to query: look for new controllers this often.
const PAD_RESCAN: Duration = Duration::from_secs(15);
const LEFT_DEADZONE: i32 = 7849;
const RIGHT_DEADZONE: i32 = 8689;
const TRIGGER_THRESHOLD: u8 = 30;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct InputSample {
    /// The system cursor is drawn (games usually hide it during play, show it in menus).
    pub cursor_visible: bool,
    /// The cursor is clipped to the game window (mouse-look).
    pub cursor_confined: bool,
    /// Milliseconds since the last keyboard or mouse input, system-wide.
    pub idle_ms: u32,
    /// A controller was used during the last second (buttons, sticks or triggers).
    pub pad_active: bool,
}

#[derive(Debug)]
pub struct InputProbe {
    pads: [Option<u32>; 4],
    last_scan: Instant,
}

impl Default for InputProbe {
    fn default() -> Self {
        InputProbe::new()
    }
}

impl InputProbe {
    pub fn new() -> InputProbe {
        let mut p = InputProbe { pads: [None; 4], last_scan: Instant::now() };
        p.scan();
        p
    }

    fn scan(&mut self) {
        for (i, slot) in self.pads.iter_mut().enumerate() {
            if slot.is_none() {
                let mut st: XINPUT_STATE = unsafe { std::mem::zeroed() };
                if unsafe { XInputGetState(i as u32, &mut st) } == 0 {
                    *slot = Some(st.dwPacketNumber);
                }
            }
        }
        self.last_scan = Instant::now();
    }

    fn pads_active(&mut self) -> bool {
        if self.last_scan.elapsed() >= PAD_RESCAN {
            self.scan();
        }
        let mut active = false;
        for (i, slot) in self.pads.iter_mut().enumerate() {
            let Some(last_packet) = *slot else { continue };
            let mut st: XINPUT_STATE = unsafe { std::mem::zeroed() };
            match unsafe { XInputGetState(i as u32, &mut st) } {
                0 => {
                    let g = st.Gamepad;
                    let left = (g.sThumbLX as i32).abs().max((g.sThumbLY as i32).abs()) > LEFT_DEADZONE;
                    let right = (g.sThumbRX as i32).abs().max((g.sThumbRY as i32).abs()) > RIGHT_DEADZONE;
                    let triggers = g.bLeftTrigger > TRIGGER_THRESHOLD || g.bRightTrigger > TRIGGER_THRESHOLD;
                    active |= st.dwPacketNumber != last_packet || left || right || triggers || g.wButtons != 0;
                    *slot = Some(st.dwPacketNumber);
                }
                ERROR_DEVICE_NOT_CONNECTED => *slot = None,
                _ => {}
            }
        }
        active
    }

    /// One reading. `game_hwnd` is the game's window (0 if unknown).
    pub fn sample(&mut self, game_hwnd: isize) -> InputSample {
        let mut out = InputSample { pad_active: self.pads_active(), ..Default::default() };
        unsafe {
            let mut ci: CURSORINFO = std::mem::zeroed();
            ci.cbSize = std::mem::size_of::<CURSORINFO>() as u32;
            if GetCursorInfo(&mut ci) != 0 {
                out.cursor_visible = ci.flags & CURSOR_SHOWING != 0 && !ci.hCursor.is_null();
            }
            let mut clip: RECT = std::mem::zeroed();
            if game_hwnd != 0 && GetClipCursor(&mut clip) != 0 {
                let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
                let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
                let whole = clip.left <= vx
                    && clip.top <= vy
                    && clip.right >= vx + GetSystemMetrics(SM_CXVIRTUALSCREEN)
                    && clip.bottom >= vy + GetSystemMetrics(SM_CYVIRTUALSCREEN);
                let mut wr: RECT = std::mem::zeroed();
                if !whole && GetWindowRect(game_hwnd as HWND, &mut wr) != 0 {
                    out.cursor_confined = clip.left >= wr.left
                        && clip.top >= wr.top
                        && clip.right <= wr.right
                        && clip.bottom <= wr.bottom;
                }
            }
            let mut li = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
            if GetLastInputInfo(&mut li) != 0 {
                out.idle_ms = GetTickCount().wrapping_sub(li.dwTime);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampling_works_without_a_game() {
        let mut p = InputProbe::new();
        let s = p.sample(0);
        assert!(!s.cursor_confined);
        assert!(s.idle_ms < 30 * 24 * 3600 * 1000);
    }
}
