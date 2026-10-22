//! Static machine description and configuration checks (read once, refreshed rarely).

use crate::nt;
use crate::util::{reg_dword, reg_string, Hive};
use windows_sys::Win32::System::SystemInformation::{
    GetLogicalProcessorInformationEx, GetTickCount64, RelationProcessorCore, SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
};
use windows_sys::Win32::System::Threading::GetActiveProcessorCount;

const ALL_PROCESSOR_GROUPS: u16 = 0xffff;

#[derive(Debug, Clone, Default)]
pub struct MachineInfo {
    pub os: String,
    pub build: String,
    pub cpu: String,
    pub cores: u32,
    pub threads: u32,
    pub gpu_registry_name: Option<String>,
    pub uptime_ms: u64,
}

pub fn machine() -> MachineInfo {
    let nt_key = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    let v = nt::os_version();
    let mut product = reg_string(Hive::LocalMachine, nt_key, "ProductName").unwrap_or_else(|| "Windows".into());
    if v.build >= 22000 {
        // The registry still says "Windows 10" on Windows 11.
        product = product.replace("Windows 10", "Windows 11");
    }
    let display = reg_string(Hive::LocalMachine, nt_key, "DisplayVersion").unwrap_or_default();
    let ubr = reg_dword(Hive::LocalMachine, nt_key, "UBR").unwrap_or(0);
    let cpu = reg_string(Hive::LocalMachine, r"HARDWARE\DESCRIPTION\System\CentralProcessor\0", "ProcessorNameString")
        .unwrap_or_else(|| "CPU".into());
    let gpu = reg_string(
        Hive::LocalMachine,
        r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0000",
        "DriverDesc",
    );
    MachineInfo {
        os: format!("{product} {display}").trim().to_owned(),
        build: format!("{}.{}", v.build, ubr),
        cpu,
        cores: physical_cores(),
        threads: unsafe { GetActiveProcessorCount(ALL_PROCESSOR_GROUPS) },
        gpu_registry_name: gpu,
        uptime_ms: unsafe { GetTickCount64() },
    }
}

fn physical_cores() -> u32 {
    unsafe {
        let mut len = 0u32;
        GetLogicalProcessorInformationEx(RelationProcessorCore, std::ptr::null_mut(), &mut len);
        if len == 0 {
            return 0;
        }
        let mut buf = vec![0u64; (len as usize).div_ceil(8)];
        if GetLogicalProcessorInformationEx(
            RelationProcessorCore,
            buf.as_mut_ptr() as *mut SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
            &mut len,
        ) == 0
        {
            return 0;
        }
        let base = buf.as_ptr() as *const u8;
        let (mut off, mut n) = (0usize, 0u32);
        while off < len as usize {
            let e = &*(base.add(off) as *const SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX);
            if e.Size == 0 {
                break;
            }
            n += 1;
            off += e.Size as usize;
        }
        n
    }
}

/// Game Mode: missing value means enabled (the Windows default).
pub fn game_mode_enabled() -> bool {
    reg_dword(Hive::CurrentUser, r"Software\Microsoft\GameBar", "AutoGameModeEnabled").unwrap_or(1) != 0
}

/// Hardware-accelerated GPU scheduling: Some(true/false) when explicitly configured.
pub fn hags_enabled() -> Option<bool> {
    reg_dword(Hive::LocalMachine, r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers", "HwSchMode").map(|v| v == 2)
}

/// Memory integrity (HVCI) setting.
pub fn hvci_enabled() -> bool {
    reg_dword(
        Hive::LocalMachine,
        r"SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity",
        "Enabled",
    )
    .unwrap_or(0)
        == 1
}

/// Timer resolution in milliseconds: (current, finest, coarsest).
pub fn timer_resolution_ms() -> Option<(f32, f32, f32)> {
    nt::timer_resolution().map(|(coarse, fine, cur)| (cur as f32 / 10_000.0, fine as f32 / 10_000.0, coarse as f32 / 10_000.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_info_is_filled() {
        let m = machine();
        assert!(m.os.starts_with("Windows"));
        assert!(m.threads >= 1 && m.cores >= 1 && m.cores <= m.threads);
        let (cur, fine, coarse) = timer_resolution_ms().unwrap();
        assert!(fine <= cur && cur <= coarse, "{fine} {cur} {coarse}");
    }
}
