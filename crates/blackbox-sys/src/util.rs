//! Small Win32 helpers shared by the collectors.

use std::ffi::c_void;
use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, LUID};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, GetTokenInformation, LookupPrivilegeValueW, TokenElevation, LUID_AND_ATTRIBUTES,
    SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES, TOKEN_ELEVATION, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows_sys::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows_sys::Win32::System::Registry::{
    RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
};
use windows_sys::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentThread, OpenProcessToken, SetThreadInformation, SetThreadPriority, ThreadPowerThrottling,
    THREAD_MODE_BACKGROUND_BEGIN, THREAD_POWER_THROTTLING_CURRENT_VERSION, THREAD_POWER_THROTTLING_EXECUTION_SPEED,
    THREAD_POWER_THROTTLING_STATE, THREAD_PRIORITY_BELOW_NORMAL,
};

pub type Micros = i64;

/// Null-terminated UTF-16.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// UTF-16 up to the first NUL (or the whole slice).
pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

pub const fn guid(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> GUID {
    GUID { data1: d1, data2: d2, data3: d3, data4: d4 }
}

pub fn guid_eq(a: &GUID, b: &GUID) -> bool {
    a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}

pub fn filetime_to_i64(ft: &FILETIME) -> i64 {
    ((ft.dwHighDateTime as i64) << 32) | ft.dwLowDateTime as i64
}

/// Monotonic clock based on QPC, the same clock ETW uses for raw timestamps
/// (sessions are created with `ClientContext = 1`).
#[derive(Debug, Clone, Copy)]
pub struct Clock {
    freq: i64,
    /// QPC ticks per µs when the frequency is a whole number of MHz (10 MHz on Windows 10+):
    /// conversions on ETW hot paths are then a single i64 division.
    ticks_per_us: i64,
    base_qpc: i64,
    base_unix_ms: i64,
}

fn wall_unix_ms() -> i64 {
    let mut ft = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    unsafe { GetSystemTimePreciseAsFileTime(&mut ft) };
    // FILETIME: 100 ns since 1601-01-01.
    (filetime_to_i64(&ft) - 116_444_736_000_000_000) / 10_000
}

impl Clock {
    pub fn new() -> Clock {
        let mut freq = 0i64;
        unsafe { QueryPerformanceFrequency(&mut freq) };
        let freq = freq.max(1);
        let ticks_per_us = if freq % 1_000_000 == 0 { freq / 1_000_000 } else { 0 };
        let base_qpc = Self::now_qpc();
        Clock { freq, ticks_per_us, base_qpc, base_unix_ms: wall_unix_ms() }
    }

    /// Re-anchors wall-clock conversions on the current system time (NTP steps, drift).
    /// Monotonic microseconds are unaffected.
    pub fn rebase_unix(&mut self) {
        let now_us = self.now_us();
        self.base_unix_ms = wall_unix_ms() - now_us.div_euclid(1000);
    }

    pub fn now_qpc() -> i64 {
        let mut v = 0i64;
        unsafe { QueryPerformanceCounter(&mut v) };
        v
    }

    pub fn freq(&self) -> i64 {
        self.freq
    }

    #[inline]
    pub fn qpc_to_us(&self, qpc: i64) -> Micros {
        self.ticks_to_us(qpc - self.base_qpc)
    }

    #[inline]
    pub fn ticks_to_us(&self, ticks: i64) -> i64 {
        if self.ticks_per_us > 0 {
            ticks / self.ticks_per_us
        } else {
            (ticks as i128 * 1_000_000 / self.freq as i128) as i64
        }
    }

    pub fn now_us(&self) -> Micros {
        self.qpc_to_us(Self::now_qpc())
    }

    pub fn us_to_unix_ms(&self, us: Micros) -> i64 {
        self.base_unix_ms + us.div_euclid(1000)
    }

    pub fn now_unix_ms(&self) -> i64 {
        self.us_to_unix_ms(self.now_us())
    }
}

impl Default for Clock {
    fn default() -> Self {
        Clock::new()
    }
}

/// RAII wrapper for kernel handles.
pub struct OwnedHandle(pub HANDLE);

impl OwnedHandle {
    pub fn is_valid(&self) -> bool {
        !self.0.is_null() && self.0 as isize != -1
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if self.is_valid() {
            unsafe { CloseHandle(self.0) };
        }
    }
}

unsafe impl Send for OwnedHandle {}
unsafe impl Sync for OwnedHandle {}

pub fn is_elevated() -> bool {
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let token = OwnedHandle(token);
        let mut elev = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token.0,
            TokenElevation,
            &mut elev as *mut _ as *mut c_void,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        );
        ok != 0 && elev.TokenIsElevated != 0
    }
}

/// Enables a privilege the token already holds (e.g. "SeDebugPrivilege" for an admin).
pub fn enable_privilege(name: &str) -> bool {
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let token = OwnedHandle(token);
        let mut luid = LUID { LowPart: 0, HighPart: 0 };
        let w = wide(name);
        if LookupPrivilegeValueW(std::ptr::null(), w.as_ptr(), &mut luid) == 0 {
            return false;
        }
        let tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }],
        };
        let ok = AdjustTokenPrivileges(token.0, 0, &tp, 0, std::ptr::null_mut(), std::ptr::null_mut());
        // AdjustTokenPrivileges succeeds even when the privilege is not held; check the last error.
        ok != 0 && windows_sys::Win32::Foundation::GetLastError() == 0
    }
}

/// Lowers CPU, I/O and memory priority of the calling thread (analysis, disk writes).
pub fn background_current_thread() {
    unsafe {
        SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN);
    }
    eco_qos_current_thread();
}

/// Below-normal CPU priority: still keeps up with ETW buffers under load.
pub fn below_normal_current_thread() {
    unsafe {
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
    eco_qos_current_thread();
}

/// Windows 11 EcoQoS: the scheduler prefers efficient cores and low clocks for this thread.
pub fn eco_qos_current_thread() {
    let state = THREAD_POWER_THROTTLING_STATE {
        Version: THREAD_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: THREAD_POWER_THROTTLING_EXECUTION_SPEED,
        StateMask: THREAD_POWER_THROTTLING_EXECUTION_SPEED,
    };
    unsafe {
        SetThreadInformation(
            GetCurrentThread(),
            ThreadPowerThrottling,
            &state as *const _ as *const c_void,
            std::mem::size_of::<THREAD_POWER_THROTTLING_STATE>() as u32,
        );
    }
}

/// Registry roots we read from (keeps raw HKEY values out of the public API).
#[derive(Debug, Clone, Copy)]
pub enum Hive {
    LocalMachine,
    CurrentUser,
}

impl Hive {
    fn hkey(self) -> HKEY {
        match self {
            Hive::LocalMachine => HKEY_LOCAL_MACHINE,
            Hive::CurrentUser => HKEY_CURRENT_USER,
        }
    }
}

pub fn reg_dword(hive: Hive, subkey: &str, value: &str) -> Option<u32> {
    let root = hive.hkey();
    let (k, v) = (wide(subkey), wide(value));
    let mut data = 0u32;
    let mut len = 4u32;
    let rc = unsafe {
        RegGetValueW(
            root,
            k.as_ptr(),
            v.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            &mut data as *mut _ as *mut c_void,
            &mut len,
        )
    };
    (rc == 0).then_some(data)
}

pub fn reg_string(hive: Hive, subkey: &str, value: &str) -> Option<String> {
    let root = hive.hkey();
    let (k, v) = (wide(subkey), wide(value));
    let mut buf = vec![0u16; 512];
    let mut len = (buf.len() * 2) as u32;
    let rc = unsafe {
        RegGetValueW(
            root,
            k.as_ptr(),
            v.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr() as *mut c_void,
            &mut len,
        )
    };
    (rc == 0).then(|| from_wide(&buf).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_is_monotonic_and_converts() {
        let c = Clock::new();
        let a = c.now_us();
        let b = c.now_us();
        assert!(b >= a);
        assert_eq!(c.ticks_to_us(c.freq()), 1_000_000);
        let unix = c.now_unix_ms();
        assert!(unix > 1_700_000_000_000);
    }

    #[test]
    fn wide_roundtrip() {
        assert_eq!(from_wide(&wide("Blackbox é")), "Blackbox é");
    }
}
