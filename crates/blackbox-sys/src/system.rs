//! Cheap whole-machine readings: memory, processor frequency limits, power plan, battery,
//! network throughput and Blackbox's own footprint.

use crate::util::{filetime_to_i64, from_wide};
use std::ffi::c_void;
use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{LocalFree, FILETIME};
use windows_sys::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_ROW2, MIB_IF_TABLE2};
use windows_sys::Win32::System::Power::{
    CallNtPowerInformation, GetSystemPowerStatus, PowerGetActiveScheme, PowerReadFriendlyName, ProcessorInformation,
    PROCESSOR_POWER_INFORMATION, SYSTEM_POWER_STATUS,
};
use windows_sys::Win32::System::ProcessStatus::{
    K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

#[derive(Debug, Clone, Copy, Default)]
pub struct MemoryInfo {
    pub total_mb: u32,
    pub avail_mb: u32,
    pub load_pct: f32,
    pub commit_mb: u32,
    pub commit_limit_mb: u32,
}

/// One cheap call. (GetPerformanceInfo would also walk every process, which the snapshot
/// already does.)
pub fn memory() -> MemoryInfo {
    let mut out = MemoryInfo::default();
    unsafe {
        let mut ms: MEMORYSTATUSEX = std::mem::zeroed();
        ms.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut ms) != 0 {
            out.total_mb = (ms.ullTotalPhys >> 20) as u32;
            out.avail_mb = (ms.ullAvailPhys >> 20) as u32;
            out.load_pct = ms.dwMemoryLoad as f32;
            // "Page file" here is the commit limit (RAM + page files) and what's left of it.
            out.commit_limit_mb = (ms.ullTotalPageFile >> 20) as u32;
            out.commit_mb = (ms.ullTotalPageFile.saturating_sub(ms.ullAvailPageFile) >> 20) as u32;
        }
    }
    out
}

/// Reusable reader for [`cpu_freq_limit_pct`].
#[derive(Default)]
pub struct CpuFreq {
    buf: Vec<PROCESSOR_POWER_INFORMATION>,
}

impl CpuFreq {
    pub fn limit_pct(&mut self, ncpu: usize) -> Option<f32> {
        cpu_freq_limit_pct(&mut self.buf, ncpu)
    }
}

/// Lowest "frequency limit / max frequency" over all logical processors, in percent.
/// Drops below 100 when Windows caps the CPU (thermal, power, battery saver).
pub fn cpu_freq_limit_pct(buf: &mut Vec<PROCESSOR_POWER_INFORMATION>, ncpu: usize) -> Option<f32> {
    if buf.len() != ncpu {
        buf.resize(ncpu, unsafe { std::mem::zeroed() });
    }
    let bytes = (ncpu * std::mem::size_of::<PROCESSOR_POWER_INFORMATION>()) as u32;
    let status = unsafe {
        CallNtPowerInformation(ProcessorInformation, std::ptr::null(), 0, buf.as_mut_ptr() as *mut c_void, bytes)
    };
    if status != 0 {
        return None;
    }
    buf.iter()
        .filter(|p| p.MaxMhz > 0)
        .map(|p| p.MhzLimit as f32 * 100.0 / p.MaxMhz as f32)
        .fold(None, |acc: Option<f32>, v| Some(acc.map_or(v, |a| a.min(v))))
        .map(|v| v.min(100.0))
}

#[derive(Debug, Clone, Default)]
pub struct PowerPlan {
    pub guid: String,
    pub name: String,
    pub is_power_saver: bool,
}

pub fn active_power_plan() -> Option<PowerPlan> {
    unsafe {
        let mut g: *mut GUID = std::ptr::null_mut();
        if PowerGetActiveScheme(std::ptr::null_mut(), &mut g) != 0 || g.is_null() {
            return None;
        }
        let guid = *g;
        let mut buf = [0u16; 128];
        let mut size = (buf.len() * 2) as u32;
        let rc = PowerReadFriendlyName(
            std::ptr::null_mut(),
            g,
            std::ptr::null(),
            std::ptr::null(),
            buf.as_mut_ptr() as *mut u8,
            &mut size,
        );
        LocalFree(g as *mut c_void);
        let s = format!(
            "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            guid.data1,
            guid.data2,
            guid.data3,
            guid.data4[0],
            guid.data4[1],
            guid.data4[2],
            guid.data4[3],
            guid.data4[4],
            guid.data4[5],
            guid.data4[6],
            guid.data4[7]
        );
        Some(PowerPlan {
            is_power_saver: s == "a1841308-3541-4fab-bc81-f71556f20b4a",
            name: if rc == 0 { from_wide(&buf) } else { String::new() },
            guid: s,
        })
    }
}

/// `Some(percent)` when running on battery, `None` on AC or without a battery.
pub fn on_battery() -> Option<u8> {
    unsafe {
        let mut s: SYSTEM_POWER_STATUS = std::mem::zeroed();
        if GetSystemPowerStatus(&mut s) == 0 {
            return None;
        }
        let no_battery = s.BatteryFlag & 128 != 0 || s.BatteryFlag == 255;
        (s.ACLineStatus == 0 && !no_battery).then_some(s.BatteryLifePercent)
    }
}

/// Total received / sent bytes over physical, non-filter interfaces.
pub fn net_octets() -> Option<(u64, u64)> {
    unsafe {
        let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        if GetIfTable2(&mut table) != 0 || table.is_null() {
            return None;
        }
        let n = (*table).NumEntries as usize;
        // The struct declares a 1-element array; the rows really follow it in the allocation.
        let first = std::ptr::addr_of!((*table).Table) as *const MIB_IF_ROW2;
        let rows = std::slice::from_raw_parts(first, n);
        let (mut rx, mut tx) = (0u64, 0u64);
        for r in rows {
            let flags = r.InterfaceAndOperStatusFlags._bitfield;
            let hardware = flags & 0x1 != 0;
            let filter = flags & 0x2 != 0;
            if hardware && !filter && r.OperStatus == 1 {
                rx += r.InOctets;
                tx += r.OutOctets;
            }
        }
        FreeMibTable(table as *const c_void);
        Some((rx, tx))
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SelfUsage {
    pub cpu_100ns: i64,
    pub working_set: u64,
    pub private_bytes: u64,
}

pub fn self_usage() -> SelfUsage {
    let mut out = SelfUsage::default();
    unsafe {
        let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let (mut c, mut e, mut k, mut u) = (zero, zero, zero, zero);
        if GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) != 0 {
            out.cpu_100ns = filetime_to_i64(&k) + filetime_to_i64(&u);
        }
        let mut pmc: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
        pmc.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        if K32GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc as *mut _ as *mut PROCESS_MEMORY_COUNTERS, pmc.cb) != 0
        {
            out.working_set = pmc.WorkingSetSize as u64;
            out.private_bytes = pmc.PrivateUsage as u64;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readings_are_plausible() {
        let m = memory();
        assert!(m.total_mb > 1024 && m.avail_mb <= m.total_mb);
        assert!(m.commit_limit_mb >= m.commit_mb);
        let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
        let mut buf = Vec::new();
        if let Some(p) = cpu_freq_limit_pct(&mut buf, n) {
            assert!(p > 0.0 && p <= 100.0);
        }
        assert!(active_power_plan().is_some());
        assert!(net_octets().is_some());
        let s = self_usage();
        assert!(s.working_set > 0);
    }
}
