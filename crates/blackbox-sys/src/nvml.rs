//! NVIDIA GPU telemetry through NVML, loaded dynamically from the driver install.
//! Nothing is loaded on machines without an NVIDIA driver.

use crate::util::wide;
use std::ffi::{c_char, c_void, CStr};
use windows_sys::Win32::Foundation::{FreeLibrary, HMODULE};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};

type Device = *mut c_void;
type Ret = i32;

#[repr(C)]
#[derive(Default)]
struct Utilization {
    gpu: u32,
    memory: u32,
}

#[repr(C)]
#[derive(Default)]
struct Memory {
    total: u64,
    free: u64,
    used: u64,
}

struct Api {
    shutdown: unsafe extern "C" fn() -> Ret,
    get_name: unsafe extern "C" fn(Device, *mut c_char, u32) -> Ret,
    utilization: unsafe extern "C" fn(Device, *mut Utilization) -> Ret,
    temperature: unsafe extern "C" fn(Device, u32, *mut u32) -> Ret,
    power_usage: unsafe extern "C" fn(Device, *mut u32) -> Ret,
    power_limit: unsafe extern "C" fn(Device, *mut u32) -> Ret,
    clock: unsafe extern "C" fn(Device, u32, *mut u32) -> Ret,
    memory: unsafe extern "C" fn(Device, *mut Memory) -> Ret,
    fan: Option<unsafe extern "C" fn(Device, *mut u32) -> Ret>,
    throttle: Option<unsafe extern "C" fn(Device, *mut u64) -> Ret>,
    proc_util: Option<unsafe extern "C" fn(Device, *mut ProcUtil, *mut u32, u64) -> Ret>,
}

/// nvmlProcessUtilizationSample_t
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct ProcUtil {
    pid: u32,
    timestamp: u64,
    sm_util: u32,
    mem_util: u32,
    enc_util: u32,
    dec_util: u32,
}

const NVML_ERROR_INSUFFICIENT_SIZE: Ret = 7;

pub struct Nvml {
    lib: HMODULE,
    api: Api,
    device: Device,
    pub name: String,
}

// The NVML handle is thread-safe; we only use it from the engine thread anyway.
unsafe impl Send for Nvml {}

#[derive(Debug, Clone, Default)]
pub struct GpuReading {
    pub util_pct: f32,
    pub mem_util_pct: f32,
    pub temp_c: Option<f32>,
    pub power_w: Option<f32>,
    pub power_limit_w: Option<f32>,
    pub core_mhz: Option<u32>,
    pub mem_mhz: Option<u32>,
    pub fan_pct: Option<f32>,
    pub vram_used_mb: u32,
    pub vram_total_mb: u32,
    /// nvmlClocksEventReasons bitmask (0x1 idle, 0x4 SW power cap, 0x8 HW slowdown, ...).
    pub throttle_mask: u64,
}

/// # Safety
/// `T` must be the exact function pointer type of the export `name`.
unsafe fn sym<T: Copy>(lib: HMODULE, name: &str) -> Option<T> {
    let c = format!("{name}\0");
    unsafe {
        let p = GetProcAddress(lib, c.as_ptr())?;
        debug_assert_eq!(std::mem::size_of_val(&p), std::mem::size_of::<T>());
        Some(std::mem::transmute_copy::<_, T>(&p))
    }
}

impl Nvml {
    pub fn load() -> Option<Nvml> {
        unsafe {
            let mut lib = LoadLibraryExW(wide("nvml.dll").as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
            if lib.is_null() {
                let alt = wide(r"C:\Program Files\NVIDIA Corporation\NVSMI\nvml.dll");
                lib = LoadLibraryExW(alt.as_ptr(), std::ptr::null_mut(), 0);
            }
            if lib.is_null() {
                return None;
            }
            match Self::init(lib) {
                Some(n) => Some(n),
                None => {
                    FreeLibrary(lib);
                    None
                }
            }
        }
    }

    /// # Safety
    /// `lib` must be a loaded nvml.dll.
    unsafe fn init(lib: HMODULE) -> Option<Nvml> {
        unsafe { Self::init_inner(lib) }
    }

    unsafe fn init_inner(lib: HMODULE) -> Option<Nvml> {
        #![allow(unsafe_op_in_unsafe_fn)]
        let init: unsafe extern "C" fn() -> Ret = sym(lib, "nvmlInit_v2")?;
        if init() != 0 {
            return None;
        }
        let by_index: unsafe extern "C" fn(u32, *mut Device) -> Ret = sym(lib, "nvmlDeviceGetHandleByIndex_v2")?;
        let api = Api {
            shutdown: sym(lib, "nvmlShutdown")?,
            get_name: sym(lib, "nvmlDeviceGetName")?,
            utilization: sym(lib, "nvmlDeviceGetUtilizationRates")?,
            temperature: sym(lib, "nvmlDeviceGetTemperature")?,
            power_usage: sym(lib, "nvmlDeviceGetPowerUsage")?,
            power_limit: sym(lib, "nvmlDeviceGetEnforcedPowerLimit")?,
            clock: sym(lib, "nvmlDeviceGetClockInfo")?,
            memory: sym(lib, "nvmlDeviceGetMemoryInfo")?,
            fan: sym(lib, "nvmlDeviceGetFanSpeed"),
            throttle: sym(lib, "nvmlDeviceGetCurrentClocksEventReasons")
                .or_else(|| sym(lib, "nvmlDeviceGetCurrentClocksThrottleReasons")),
            proc_util: sym(lib, "nvmlDeviceGetProcessUtilization"),
        };
        let mut device: Device = std::ptr::null_mut();
        if by_index(0, &mut device) != 0 || device.is_null() {
            (api.shutdown)();
            return None;
        }
        let mut name = [0 as c_char; 96];
        let name = if (api.get_name)(device, name.as_mut_ptr(), name.len() as u32) == 0 {
            CStr::from_ptr(name.as_ptr()).to_string_lossy().into_owned()
        } else {
            "NVIDIA GPU".to_owned()
        };
        Some(Nvml { lib, api, device, name })
    }

    pub fn read(&self) -> GpuReading {
        let mut r = GpuReading::default();
        unsafe {
            let d = self.device;
            let mut u = Utilization::default();
            if (self.api.utilization)(d, &mut u) == 0 {
                r.util_pct = u.gpu as f32;
                r.mem_util_pct = u.memory as f32;
            }
            let mut v = 0u32;
            if (self.api.temperature)(d, 0, &mut v) == 0 {
                r.temp_c = Some(v as f32);
            }
            if (self.api.power_usage)(d, &mut v) == 0 {
                r.power_w = Some(v as f32 / 1000.0);
            }
            if (self.api.power_limit)(d, &mut v) == 0 {
                r.power_limit_w = Some(v as f32 / 1000.0);
            }
            if (self.api.clock)(d, 0, &mut v) == 0 {
                r.core_mhz = Some(v);
            }
            if (self.api.clock)(d, 2, &mut v) == 0 {
                r.mem_mhz = Some(v);
            }
            if let Some(fan) = self.api.fan {
                if fan(d, &mut v) == 0 {
                    r.fan_pct = Some(v as f32);
                }
            }
            let mut m = Memory::default();
            if (self.api.memory)(d, &mut m) == 0 {
                r.vram_used_mb = (m.used >> 20) as u32;
                r.vram_total_mb = (m.total >> 20) as u32;
            }
            if let Some(t) = self.api.throttle {
                let mut mask = 0u64;
                if t(d, &mut mask) == 0 {
                    r.throttle_mask = mask;
                }
            }
        }
        r
    }
}

impl Nvml {
    /// GPU usage per process (%, 3D/compute engine) over the last couple of seconds, highest
    /// first. Empty when the driver doesn't report it.
    pub fn process_utilization(&self) -> Vec<(u32, f32)> {
        let Some(f) = self.api.proc_util else { return Vec::new() };
        let since = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_micros() as u64)
            .saturating_sub(3_000_000);
        let mut count = 0u32;
        let rc = unsafe { f(self.device, std::ptr::null_mut(), &mut count, since) };
        if rc != NVML_ERROR_INSUFFICIENT_SIZE || count == 0 {
            return Vec::new();
        }
        let mut buf = vec![ProcUtil::default(); count as usize + 8];
        count = buf.len() as u32;
        if unsafe { f(self.device, buf.as_mut_ptr(), &mut count, since) } != 0 {
            return Vec::new();
        }
        // Several samples per process: keep the highest.
        let mut out: Vec<(u32, f32)> = Vec::new();
        for s in &buf[..(count as usize).min(buf.len())] {
            match out.iter_mut().find(|(p, _)| *p == s.pid) {
                Some((_, u)) => *u = u.max(s.sm_util as f32),
                None => out.push((s.pid, s.sm_util as f32)),
            }
        }
        out.sort_by(|a, b| b.1.total_cmp(&a.1));
        out
    }
}

impl Drop for Nvml {
    fn drop(&mut self) {
        unsafe {
            (self.api.shutdown)();
            FreeLibrary(self.lib);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_when_an_nvidia_gpu_is_present() {
        if let Some(n) = Nvml::load() {
            let r = n.read();
            assert!(!n.name.is_empty());
            assert!(r.vram_total_mb > 0);
            let per_process = n.process_utilization();
            println!("per-process GPU: {per_process:?}");
            assert!(per_process.iter().all(|(_, u)| (0.0..=100.0).contains(u)));
        }
    }
}
