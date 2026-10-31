//! CPU temperature through PawnIO, the signed driver used by LibreHardwareMonitor and
//! FanControl. Blackbox ships two of its official modules (LGPL-2.1, see `pawnio/COPYING`):
//!
//! - AMD family 17h–1Ah (Zen 1–5): SMN register THM_TCON_CUR_TMP → Tctl.
//! - Intel: IA32_PACKAGE_THERM_STATUS against TjMax (IA32_TEMPERATURE_TARGET).
//!
//! Needs the PawnIO driver installed (https://pawnio.eu) and an elevated Blackbox. Without
//! them, there's simply no CPU temperature. One register read per second.

use crate::util::{reg_string, wide, Hive, OwnedHandle};
use windows_sys::Win32::Foundation::{FreeLibrary, HANDLE, HMODULE, WAIT_ABANDONED, WAIT_OBJECT_0};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_WITH_ALTERED_SEARCH_PATH};
use windows_sys::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};

const AMD_MODULE: &[u8] = include_bytes!("../pawnio/AMDFamily17.bin");
const INTEL_MODULE: &[u8] = include_bytes!("../pawnio/IntelMSR.bin");

/// Zen thermal control register (THM_TCON_CUR_TMP).
const SMN_THM_TCON_CUR_TMP: u64 = 0x0005_9800;
const ZEN_RANGE_SEL: u64 = 1 << 19;
const MSR_TEMPERATURE_TARGET: u64 = 0x1A2;
const MSR_PACKAGE_THERM_STATUS: u64 = 0x1B1;

type Open = unsafe extern "system" fn(*mut HANDLE) -> i32;
type Load = unsafe extern "system" fn(HANDLE, *const u8, usize) -> i32;
type Execute = unsafe extern "system" fn(HANDLE, *const u8, *const u64, usize, *mut u64, usize, *mut usize) -> i32;
type Close = unsafe extern "system" fn(HANDLE) -> i32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Vendor {
    Amd,
    Intel,
}

/// Why there is no CPU temperature (shown on the System page).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    NotInstalled,
    NotElevated,
    UnsupportedCpu,
    Failed,
}

pub struct CpuTemp {
    lib: HMODULE,
    handle: HANDLE,
    execute: Execute,
    close: Close,
    vendor: Vendor,
    tj_max: f32,
    /// Serializes SMN/PCI access with other tools (HWiNFO, LibreHardwareMonitor, FanControl).
    pci_mutex: OwnedHandle,
}

// The PawnIO handle is used from the engine thread only.
unsafe impl Send for CpuTemp {}

fn library_path() -> Option<Vec<u16>> {
    let dir = reg_string(
        Hive::LocalMachine,
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO",
        "InstallLocation",
    )
    .unwrap_or_else(|| r"C:\Program Files\PawnIO".to_owned());
    let path = std::path::Path::new(dir.trim_matches('"')).join("PawnIOLib.dll");
    path.exists().then(|| wide(&path.to_string_lossy()))
}

/// # Safety
/// `T` must be the exact function pointer type of `name`.
unsafe fn sym<T: Copy>(lib: HMODULE, name: &str) -> Option<T> {
    let c = format!("{name}\0");
    unsafe {
        let p = GetProcAddress(lib, c.as_ptr())?;
        Some(std::mem::transmute_copy::<_, T>(&p))
    }
}

impl CpuTemp {
    pub fn open(cpu_name: &str, elevated: bool) -> Result<CpuTemp, Unavailable> {
        let vendor = if cpu_name.contains("AMD") {
            Vendor::Amd
        } else if cpu_name.contains("Intel") {
            Vendor::Intel
        } else {
            return Err(Unavailable::UnsupportedCpu);
        };
        let path = library_path().ok_or(Unavailable::NotInstalled)?;
        if !elevated {
            return Err(Unavailable::NotElevated);
        }
        unsafe {
            let lib = LoadLibraryExW(path.as_ptr(), std::ptr::null_mut(), LOAD_WITH_ALTERED_SEARCH_PATH);
            if lib.is_null() {
                return Err(Unavailable::NotInstalled);
            }
            let fns = (
                sym::<Open>(lib, "pawnio_open"),
                sym::<Load>(lib, "pawnio_load"),
                sym::<Execute>(lib, "pawnio_execute"),
                sym::<Close>(lib, "pawnio_close"),
            );
            let (Some(open), Some(load), Some(execute), Some(close)) = fns else {
                FreeLibrary(lib);
                return Err(Unavailable::NotInstalled);
            };
            let mut handle: HANDLE = std::ptr::null_mut();
            if open(&mut handle) < 0 || handle.is_null() {
                FreeLibrary(lib);
                return Err(Unavailable::Failed);
            }
            let module = if vendor == Vendor::Amd { AMD_MODULE } else { INTEL_MODULE };
            if load(handle, module.as_ptr(), module.len()) < 0 {
                close(handle);
                FreeLibrary(lib);
                // The module refuses CPUs it doesn't know (e.g. AMD before Zen).
                return Err(Unavailable::UnsupportedCpu);
            }
            let pci_mutex = OwnedHandle(CreateMutexW(std::ptr::null(), 0, wide("Global\\Access_PCI").as_ptr()));
            let mut t = CpuTemp { lib, handle, execute, close, vendor, tj_max: 100.0, pci_mutex };
            if vendor == Vendor::Intel {
                if let Some(v) = t.call("ioctl_read_msr", MSR_TEMPERATURE_TARGET) {
                    let tj = ((v >> 16) & 0xFF) as f32;
                    if tj > 0.0 {
                        t.tj_max = tj;
                    }
                }
            }
            if t.read().is_none() {
                return Err(Unavailable::Failed);
            }
            Ok(t)
        }
    }

    fn call(&self, name: &str, arg: u64) -> Option<u64> {
        let c = format!("{name}\0");
        let input = [arg];
        let mut out = [0u64; 1];
        let mut written = 0usize;
        let hr =
            unsafe { (self.execute)(self.handle, c.as_ptr(), input.as_ptr(), 1, out.as_mut_ptr(), 1, &mut written) };
        (hr >= 0 && written >= 1).then_some(out[0])
    }

    /// Current CPU temperature in °C (Tctl on AMD, package on Intel).
    pub fn read(&mut self) -> Option<f32> {
        match self.vendor {
            Vendor::Amd => {
                // SMN goes through a PCI index/data pair shared by every tool: hold the mutex.
                let locked = self.pci_mutex.is_valid() && {
                    let w = unsafe { WaitForSingleObject(self.pci_mutex.0, 20) };
                    w == WAIT_OBJECT_0 || w == WAIT_ABANDONED
                };
                let raw = self.call("ioctl_read_smn", SMN_THM_TCON_CUR_TMP);
                if locked {
                    unsafe { ReleaseMutex(self.pci_mutex.0) };
                }
                let raw = raw?;
                let mut c = ((raw >> 21) & 0x7FF) as f32 * 0.125;
                if raw & ZEN_RANGE_SEL != 0 {
                    c -= 49.0;
                }
                (0.0..130.0).contains(&c).then_some(c)
            }
            Vendor::Intel => {
                let v = self.call("ioctl_read_msr", MSR_PACKAGE_THERM_STATUS)?;
                if v & (1 << 31) == 0 {
                    return None; // reading not valid
                }
                let c = self.tj_max - ((v >> 16) & 0x7F) as f32;
                (0.0..130.0).contains(&c).then_some(c)
            }
        }
    }
}

impl Drop for CpuTemp {
    fn drop(&mut self) {
        unsafe {
            (self.close)(self.handle);
            FreeLibrary(self.lib);
        }
    }
}

impl std::fmt::Debug for CpuTemp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CpuTemp").field("vendor", &self.vendor).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_or_explains_why_not() {
        let cpu = crate::sysinfo::machine().cpu;
        match CpuTemp::open(&cpu, crate::util::is_elevated()) {
            Ok(mut t) => {
                let c = t.read().expect("a reading");
                assert!(c > 10.0 && c < 110.0, "{c}");
            }
            Err(e) => {
                assert!(matches!(e, Unavailable::NotElevated | Unavailable::NotInstalled | Unavailable::UnsupportedCpu))
            }
        }
    }
}
