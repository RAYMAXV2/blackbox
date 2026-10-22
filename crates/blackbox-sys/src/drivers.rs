//! Kernel module map: resolves a code address (DPC/ISR routine) to the driver that owns it.
//!
//! Uses one `SystemModuleInformation` query (names, bases and sizes in a single call) instead
//! of hundreds of `GetDeviceDriverBaseName` calls. Since Windows 11 24H2 kernel addresses are
//! only returned to processes holding SeDebugPrivilege, which the engine enables when elevated.

use crate::nt::{query_system_information, AlignedBuf, SYSTEM_MODULE_INFORMATION};
use std::sync::{Arc, Mutex};

pub use blackbox_core::series::NO_DRIVER;
/// Hard cap on distinct module names (ids are u16 and NO_DRIVER is reserved).
const MAX_NAMES: usize = 4096;

#[repr(C)]
struct RtlProcessModuleInformation {
    section: usize,
    mapped_base: usize,
    image_base: usize,
    image_size: u32,
    flags: u32,
    load_order_index: u16,
    init_order_index: u16,
    load_count: u16,
    offset_to_file_name: u16,
    full_path_name: [u8; 256],
}

const _: () = assert!(std::mem::size_of::<RtlProcessModuleInformation>() == 296);

/// Append-only id <-> name table shared between the ETW thread and the engine.
#[derive(Debug, Default)]
pub struct DriverRegistry {
    inner: Mutex<RegistryInner>,
}

#[derive(Debug, Default)]
struct RegistryInner {
    names: Vec<String>,
    loaded: Vec<bool>,
}

impl DriverRegistry {
    pub fn new() -> Arc<DriverRegistry> {
        Arc::new(DriverRegistry::default())
    }

    fn intern(inner: &mut RegistryInner, name: &str) -> u16 {
        if let Some(i) = inner.names.iter().position(|n| n.eq_ignore_ascii_case(name)) {
            return i as u16;
        }
        if inner.names.len() >= MAX_NAMES {
            return NO_DRIVER;
        }
        inner.names.push(name.to_owned());
        inner.loaded.push(false);
        (inner.names.len() - 1) as u16
    }

    pub fn name(&self, id: u16) -> Option<String> {
        self.inner.lock().ok()?.names.get(id as usize).cloned()
    }

    /// (name, currently loaded) for every id.
    pub fn snapshot(&self) -> Vec<(String, bool)> {
        match self.inner.lock() {
            Ok(g) => g.names.iter().cloned().zip(g.loaded.iter().copied()).collect(),
            Err(_) => Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Range {
    base: u64,
    end: u64,
    id: u16,
}

#[derive(Debug)]
pub struct ModuleMap {
    ranges: Vec<Range>,
    registry: Arc<DriverRegistry>,
    buf: AlignedBuf,
    available: bool,
}

impl ModuleMap {
    pub fn new(registry: Arc<DriverRegistry>) -> ModuleMap {
        let mut m = ModuleMap { ranges: Vec::new(), registry, buf: AlignedBuf::with_bytes(128 * 1024), available: false };
        m.refresh();
        m
    }

    /// True when the kernel returned real module addresses.
    pub fn available(&self) -> bool {
        self.available
    }

    pub fn refresh(&mut self) {
        let Ok(len) = query_system_information(SYSTEM_MODULE_INFORMATION, &mut self.buf) else {
            return;
        };
        if len < 8 {
            return;
        }
        let base = self.buf.as_ptr();
        let count = unsafe { (base as *const u32).read() } as usize;
        let entry = std::mem::size_of::<RtlProcessModuleInformation>();
        let count = count.min((len.saturating_sub(8)) / entry);
        let mut ranges = Vec::with_capacity(count);
        let Ok(mut reg) = self.registry.inner.lock() else {
            return;
        };
        reg.loaded.iter_mut().for_each(|l| *l = false);
        for i in 0..count {
            // SAFETY: bounds checked against `len` above; the buffer is 8-byte aligned.
            let m = unsafe { &*(base.add(8 + i * entry) as *const RtlProcessModuleInformation) };
            let off = (m.offset_to_file_name as usize).min(255);
            let raw = &m.full_path_name[off..];
            let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
            let name = String::from_utf8_lossy(&raw[..end]).into_owned();
            if name.is_empty() {
                continue;
            }
            let id = DriverRegistry::intern(&mut reg, &name);
            if let Some(l) = reg.loaded.get_mut(id as usize) {
                *l = true;
            }
            if m.image_base != 0 && m.image_size != 0 {
                ranges.push(Range { base: m.image_base as u64, end: m.image_base as u64 + m.image_size as u64, id });
            }
        }
        drop(reg);
        ranges.sort_unstable_by_key(|r| r.base);
        self.available = !ranges.is_empty();
        self.ranges = ranges;
    }

    pub fn lookup(&self, addr: u64) -> u16 {
        let i = self.ranges.partition_point(|r| r.base <= addr);
        if i == 0 {
            return NO_DRIVER;
        }
        let r = self.ranges[i - 1];
        if addr < r.end {
            r.id
        } else {
            NO_DRIVER
        }
    }

    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_kernel_modules_names() {
        let reg = DriverRegistry::new();
        let map = ModuleMap::new(reg.clone());
        let names = reg.snapshot();
        assert!(names.iter().any(|(n, _)| n.eq_ignore_ascii_case("ntoskrnl.exe")), "{names:?}");
        if map.available() {
            assert!(map.len() > 10);
        }
    }
}
