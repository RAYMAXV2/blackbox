//! Native API (ntdll) entry points that are stable but not exposed by windows-sys.
//! Bound with `raw-dylib`, so no import library is needed.

use std::ffi::c_void;

pub const SYSTEM_PROCESS_INFORMATION: u32 = 5;
pub const SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION: u32 = 8;
pub const SYSTEM_MODULE_INFORMATION: u32 = 11;
pub const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC000_0004_u32 as i32;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct OsVersionInfoW {
    pub size: u32,
    pub major: u32,
    pub minor: u32,
    pub build: u32,
    pub platform_id: u32,
    pub csd_version: [u16; 128],
}

#[link(name = "ntdll", kind = "raw-dylib")]
extern "system" {
    fn NtQuerySystemInformation(class: u32, info: *mut c_void, len: u32, ret_len: *mut u32) -> i32;
    fn NtQueryTimerResolution(maximum: *mut u32, minimum: *mut u32, current: *mut u32) -> i32;
    fn RtlGetVersion(info: *mut OsVersionInfoW) -> i32;
}

/// 8-byte aligned growable buffer for variable-size native structures.
#[derive(Debug, Default)]
pub struct AlignedBuf {
    words: Vec<u64>,
}

impl AlignedBuf {
    pub fn with_bytes(bytes: usize) -> AlignedBuf {
        AlignedBuf { words: vec![0; bytes.div_ceil(8)] }
    }
    pub fn bytes(&self) -> usize {
        self.words.len() * 8
    }
    pub fn as_ptr(&self) -> *const u8 {
        self.words.as_ptr() as *const u8
    }
    fn as_mut_ptr(&mut self) -> *mut c_void {
        self.words.as_mut_ptr() as *mut c_void
    }
    fn grow_to(&mut self, bytes: usize) {
        if bytes > self.bytes() {
            self.words.resize(bytes.div_ceil(8), 0);
        }
    }
}

/// `NtQuerySystemInformation` into `buf`, growing it when the data doesn't fit.
/// Returns the number of bytes written.
pub fn query_system_information(class: u32, buf: &mut AlignedBuf) -> Result<usize, i32> {
    for _ in 0..8 {
        let mut needed = 0u32;
        let status = unsafe { NtQuerySystemInformation(class, buf.as_mut_ptr(), buf.bytes() as u32, &mut needed) };
        if status == STATUS_INFO_LENGTH_MISMATCH {
            // Processes and threads come and go between calls: leave headroom.
            let target = (needed as usize).max(buf.bytes() * 2) + 64 * 1024;
            buf.grow_to(target);
            continue;
        }
        if status < 0 {
            return Err(status);
        }
        return Ok(needed as usize);
    }
    Err(STATUS_INFO_LENGTH_MISMATCH)
}

/// Timer resolution in 100 ns units: (coarsest, finest, current).
pub fn timer_resolution() -> Option<(u32, u32, u32)> {
    let (mut max, mut min, mut cur) = (0u32, 0u32, 0u32);
    let status = unsafe { NtQueryTimerResolution(&mut max, &mut min, &mut cur) };
    (status >= 0).then_some((max, min, cur))
}

pub fn os_version() -> OsVersionInfoW {
    let mut v = OsVersionInfoW { size: 0, major: 0, minor: 0, build: 0, platform_id: 0, csd_version: [0; 128] };
    v.size = std::mem::size_of::<OsVersionInfoW>() as u32;
    unsafe { RtlGetVersion(&mut v) };
    v
}
