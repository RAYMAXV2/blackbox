//! Kernel session: DPC and ISR durations per driver, disk I/O latency, hard page faults and
//! process start/stop. All payloads are parsed at fixed offsets (x64 layouts).

use super::{read_u32, read_u64, user_data, EventSink, ProviderSpec, SessionSpec};
use crate::drivers::{DriverRegistry, ModuleMap, NO_DRIVER};
use crate::util::{guid, guid_eq, Clock};
use blackbox_core::series::{KernelBucket, BUCKET_US};
use std::sync::{Arc, Mutex};
use windows_sys::core::GUID;
use windows_sys::Win32::System::Diagnostics::Etw::{
    EVENT_RECORD, EVENT_TRACE_FLAG_DISK_IO, EVENT_TRACE_FLAG_DPC, EVENT_TRACE_FLAG_INTERRUPT,
    EVENT_TRACE_FLAG_MEMORY_HARD_FAULTS, EVENT_TRACE_FLAG_PROCESS,
};

const PERFINFO: GUID = guid(0xce1dbfb4, 0x137e, 0x4da6, [0x87, 0xb0, 0x3f, 0x59, 0xaa, 0x10, 0x2c, 0xbc]);
const DISKIO: GUID = guid(0x3d6fa8d4, 0xfe05, 0x11d0, [0x9d, 0xda, 0x00, 0xc0, 0x4f, 0xd7, 0xba, 0x7c]);
const PAGEFAULT: GUID = guid(0x3d6fa8d3, 0xfe05, 0x11d0, [0x9d, 0xda, 0x00, 0xc0, 0x4f, 0xd7, 0xba, 0x7c]);
const PROCESS: GUID = guid(0x3d6fa8d0, 0xfe05, 0x11d0, [0x9d, 0xda, 0x00, 0xc0, 0x4f, 0xd7, 0xba, 0x7c]);
const SESSION_GUID: GUID = guid(0xb1ac0b0c, 0x0d1e, 0x4b5f, [0x9a, 0x1d, 0x6b, 0x3c, 0x2e, 0x7f, 0x4a, 0x01]);

pub const SESSION_NAME: &str = "Blackbox-Kernel";

const OP_DPC_THREADED: u8 = 66;
const OP_ISR: u8 = 67;
const OP_DPC: u8 = 68;
const OP_DPC_TIMER: u8 = 69;
const OP_ISR_MSI: u8 = 50;
const OP_DISK_READ: u8 = 10;
const OP_DISK_WRITE: u8 = 11;
const OP_HARD_FAULT: u8 = 32;
const OP_PROCESS_START: u8 = 1;
const OP_PROCESS_END: u8 = 2;

/// Pending buckets are published early past this count (keeps the hot path allocation-free).
const MAX_PENDING: usize = 48;
/// Kernel modules are re-enumerated at most this often (new USB devices, etc.).
const MODULE_REFRESH_S: i64 = 30;

pub fn spec() -> SessionSpec {
    SessionSpec {
        name: SESSION_NAME.into(),
        session_guid: SESSION_GUID,
        kernel_flags: EVENT_TRACE_FLAG_PROCESS
            | EVENT_TRACE_FLAG_DPC
            | EVENT_TRACE_FLAG_INTERRUPT
            | EVENT_TRACE_FLAG_DISK_IO
            | EVENT_TRACE_FLAG_MEMORY_HARD_FAULTS,
        providers: Vec::<ProviderSpec>::new(),
        buffer_kb: 64,
        min_buffers: 4,
        max_buffers: 64,
        flush_s: 1,
    }
}

/// Latency histogram buckets (µs): <50, <100, <250, <500, <1000, <2000, >=2000.
pub fn hist_bucket(us: u32) -> usize {
    match us {
        0..=49 => 0,
        50..=99 => 1,
        100..=249 => 2,
        250..=499 => 3,
        500..=999 => 4,
        1000..=1999 => 5,
        _ => 6,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DriverStat {
    pub dpc_count: u64,
    pub dpc_total_us: u64,
    pub dpc_max_us: u32,
    pub isr_count: u64,
    pub isr_total_us: u64,
    pub isr_max_us: u32,
    pub hist: [u64; 7],
}

impl DriverStat {
    fn add(&mut self, us: u32, isr: bool) {
        if isr {
            self.isr_count += 1;
            self.isr_total_us += us as u64;
            self.isr_max_us = self.isr_max_us.max(us);
        } else {
            self.dpc_count += 1;
            self.dpc_total_us += us as u64;
            self.dpc_max_us = self.dpc_max_us.max(us);
        }
        self.hist[hist_bucket(us)] += 1;
    }

    pub fn merge(&mut self, o: &DriverStat) {
        self.dpc_count += o.dpc_count;
        self.dpc_total_us += o.dpc_total_us;
        self.dpc_max_us = self.dpc_max_us.max(o.dpc_max_us);
        self.isr_count += o.isr_count;
        self.isr_total_us += o.isr_total_us;
        self.isr_max_us = self.isr_max_us.max(o.isr_max_us);
        for (a, b) in self.hist.iter_mut().zip(o.hist.iter()) {
            *a += b;
        }
    }

    /// Counts and totals between two cumulative snapshots. Maxima can't be diffed: they are
    /// left at 0 and come from `KernelData::window_max` instead.
    pub fn since(&self, prev: &DriverStat) -> DriverStat {
        let mut hist = [0u64; 7];
        for (i, h) in hist.iter_mut().enumerate() {
            *h = self.hist[i].saturating_sub(prev.hist[i]);
        }
        DriverStat {
            dpc_count: self.dpc_count.saturating_sub(prev.dpc_count),
            dpc_total_us: self.dpc_total_us.saturating_sub(prev.dpc_total_us),
            dpc_max_us: 0,
            isr_count: self.isr_count.saturating_sub(prev.isr_count),
            isr_total_us: self.isr_total_us.saturating_sub(prev.isr_total_us),
            isr_max_us: 0,
            hist,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RawProcEvent {
    pub t_us: i64,
    pub pid: u32,
    pub ppid: u32,
    pub start: bool,
    pub image: String,
}

/// What the consumer thread publishes; the engine drains it every tick.
#[derive(Debug, Default)]
pub struct KernelData {
    pub buckets: Vec<KernelBucket>,
    pub procs: Vec<RawProcEvent>,
    /// Cumulative per driver id since the session started.
    pub drivers: Vec<DriverStat>,
    /// Per-driver max of the last published window (reset when drained), for accurate peaks.
    pub window_max: Vec<(u32, u32)>,
    pub bad_timestamps: u64,
    pub modules_available: bool,
}

pub struct KernelSink {
    clock: Clock,
    modules: ModuleMap,
    last_refresh_us: i64,
    pending: Vec<KernelBucket>,
    procs: Vec<RawProcEvent>,
    drv_delta: Vec<DriverStat>,
    drv_max: Vec<(u32, u32)>,
    dirty: Vec<u16>,
    bad_timestamps: u64,
    shared: Arc<Mutex<KernelData>>,
}

impl KernelSink {
    pub fn new(clock: Clock, registry: Arc<DriverRegistry>, shared: Arc<Mutex<KernelData>>) -> KernelSink {
        let modules = ModuleMap::new(registry);
        if let Ok(mut s) = shared.lock() {
            s.modules_available = modules.available();
        }
        KernelSink {
            clock,
            modules,
            last_refresh_us: clock.now_us(),
            pending: Vec::with_capacity(MAX_PENDING),
            procs: Vec::with_capacity(64),
            drv_delta: Vec::with_capacity(512),
            drv_max: Vec::with_capacity(512),
            dirty: Vec::with_capacity(64),
            bad_timestamps: 0,
            shared,
        }
    }

    #[inline]
    fn bucket(&mut self, t_us: i64) -> &mut KernelBucket {
        let idx = t_us.div_euclid(BUCKET_US);
        if let Some(pos) = self.pending.iter().rposition(|b| b.idx == idx) {
            return &mut self.pending[pos];
        }
        if self.pending.len() >= MAX_PENDING {
            self.publish();
        }
        self.pending.push(KernelBucket::empty(idx));
        self.pending.last_mut().expect("just pushed")
    }

    fn driver_event(&mut self, rec: &EVENT_RECORD, data: &[u8], isr: bool) {
        let (Some(initial), Some(routine)) = (read_u64(data, 0), read_u64(data, 8)) else {
            return;
        };
        let end = rec.EventHeader.TimeStamp;
        let ticks = end - initial as i64;
        if ticks < 0 || ticks > self.clock.freq() {
            self.bad_timestamps += 1;
            return;
        }
        let us = self.clock.ticks_to_us(ticks) as u32;
        let drv = self.modules.lookup(routine);
        let t_us = self.clock.qpc_to_us(end);
        let b = self.bucket(t_us);
        if isr {
            b.add_isr(us, drv);
        } else {
            b.add_dpc(us, drv);
        }
        if drv != NO_DRIVER {
            let i = drv as usize;
            if self.drv_delta.len() <= i {
                self.drv_delta.resize(i + 1, DriverStat::default());
                self.drv_max.resize(i + 1, (0, 0));
            }
            if self.drv_delta[i] == DriverStat::default() {
                self.dirty.push(drv);
            }
            self.drv_delta[i].add(us, isr);
            let m = &mut self.drv_max[i];
            if isr {
                m.1 = m.1.max(us);
            } else {
                m.0 = m.0.max(us);
            }
        }
    }

    fn disk_event(&mut self, rec: &EVENT_RECORD, data: &[u8], write: bool) {
        let (Some(disk), Some(size), Some(resp)) = (read_u32(data, 0), read_u32(data, 8), read_u64(data, 40)) else {
            return;
        };
        let lat_us = self.clock.ticks_to_us(resp as i64).clamp(0, u32::MAX as i64) as u32;
        let t_us = self.clock.qpc_to_us(rec.EventHeader.TimeStamp);
        let slot = KernelBucket::disk_slot(disk);
        self.bucket(t_us).disks[slot].add_io(write, size, lat_us);
    }

    fn process_event(&mut self, rec: &EVENT_RECORD, data: &[u8], start: bool) {
        let (Some(pid), Some(ppid)) = (read_u32(data, 8), read_u32(data, 12)) else {
            return;
        };
        let image = parse_process_image(data, rec.EventHeader.EventDescriptor.Version).unwrap_or_default();
        let t_us = self.clock.qpc_to_us(rec.EventHeader.TimeStamp);
        self.procs.push(RawProcEvent { t_us, pid, ppid, start, image });
    }

    fn publish(&mut self) {
        let Ok(mut s) = self.shared.lock() else {
            return;
        };
        s.buckets.append(&mut self.pending);
        s.procs.append(&mut self.procs);
        if s.drivers.len() < self.drv_delta.len() {
            s.drivers.resize(self.drv_delta.len(), DriverStat::default());
            s.window_max.resize(self.drv_delta.len(), (0, 0));
        }
        for &id in &self.dirty {
            let i = id as usize;
            s.drivers[i].merge(&self.drv_delta[i]);
            let (d, r) = self.drv_max[i];
            let w = &mut s.window_max[i];
            w.0 = w.0.max(d);
            w.1 = w.1.max(r);
            self.drv_delta[i] = DriverStat::default();
            self.drv_max[i] = (0, 0);
        }
        self.dirty.clear();
        s.bad_timestamps = self.bad_timestamps;
    }
}

impl EventSink for KernelSink {
    fn on_event(&mut self, rec: &EVENT_RECORD) {
        let h = &rec.EventHeader;
        let op = h.EventDescriptor.Opcode;
        let data = user_data(rec);
        if guid_eq(&h.ProviderId, &PERFINFO) {
            match op {
                OP_DPC | OP_DPC_THREADED | OP_DPC_TIMER => self.driver_event(rec, data, false),
                OP_ISR | OP_ISR_MSI => self.driver_event(rec, data, true),
                _ => {}
            }
        } else if guid_eq(&h.ProviderId, &DISKIO) {
            match op {
                OP_DISK_READ => self.disk_event(rec, data, false),
                OP_DISK_WRITE => self.disk_event(rec, data, true),
                _ => {}
            }
        } else if guid_eq(&h.ProviderId, &PAGEFAULT) {
            if op == OP_HARD_FAULT {
                let t_us = self.clock.qpc_to_us(h.TimeStamp);
                self.bucket(t_us).hard_faults += 1;
            }
        } else if guid_eq(&h.ProviderId, &PROCESS) {
            match op {
                OP_PROCESS_START => self.process_event(rec, data, true),
                OP_PROCESS_END => self.process_event(rec, data, false),
                _ => {}
            }
        }
    }

    fn on_buffer(&mut self) {
        let now = self.clock.now_us();
        if now - self.last_refresh_us > MODULE_REFRESH_S * 1_000_000 {
            self.last_refresh_us = now;
            self.modules.refresh();
            if let Ok(mut s) = self.shared.lock() {
                s.modules_available = self.modules.available();
            }
        }
        self.publish();
    }
}

/// Image file name from a Process_TypeGroup1 payload (x64).
///
/// Layout: UniqueProcessKey(8) ProcessId(4) ParentId(4) SessionId(4) ExitStatus(4)
/// DirectoryTableBase(8) [Flags(4) since v4] UserSID(TOKEN_USER + SID) ImageFileName(ANSI)...
pub fn parse_process_image(data: &[u8], version: u8) -> Option<String> {
    let mut off = if version >= 4 { 36 } else { 32 };
    // TOKEN_USER is two pointers (SID pointer + attributes); a NULL pointer means no SID.
    let sid_ptr = read_u64(data, off)?;
    off += 16;
    if sid_ptr != 0 {
        let sub_count = *data.get(off + 1)? as usize;
        off += 8 + 4 * sub_count;
    }
    let rest = data.get(off..)?;
    let end = rest.iter().position(|&b| b == 0)?;
    let name = &rest[..end];
    // ANSI, possibly non-ASCII: accept any printable byte, reject control characters (which
    // would mean we mis-skipped the SID).
    if name.is_empty() || name.len() > 260 || name.iter().any(|&b| b < 0x20 || b == 0x7f) {
        return None;
    }
    Some(String::from_utf8_lossy(name).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process_payload(version: u8, name: &str) -> Vec<u8> {
        let mut d = vec![0u8; if version >= 4 { 36 } else { 32 }];
        d[8..12].copy_from_slice(&1234u32.to_le_bytes());
        d[12..16].copy_from_slice(&42u32.to_le_bytes());
        // TOKEN_USER: SID pointer + attributes
        d.extend_from_slice(&0xffff_8000_0000_1000u64.to_le_bytes());
        d.extend_from_slice(&0u64.to_le_bytes());
        // SID S-1-5-21-a-b-c-1001: revision 1, 5 sub-authorities
        d.extend_from_slice(&[1, 5, 0, 0, 0, 0, 0, 5]);
        for v in [21u32, 1, 2, 3, 1001] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.extend_from_slice(name.as_bytes());
        d.push(0);
        d.extend_from_slice(&[b'x', 0, 0, 0]); // start of the UTF-16 command line
        d
    }

    #[test]
    fn parses_process_image_v3_and_v4() {
        assert_eq!(
            parse_process_image(&process_payload(4, "CompatTelRunner.exe"), 4).as_deref(),
            Some("CompatTelRunner.exe")
        );
        assert_eq!(parse_process_image(&process_payload(3, "notepad.exe"), 3).as_deref(), Some("notepad.exe"));
        assert_eq!(parse_process_image(&[0u8; 20], 4), None);
    }

    #[test]
    fn histogram_buckets() {
        assert_eq!(hist_bucket(10), 0);
        assert_eq!(hist_bucket(499), 3);
        assert_eq!(hist_bucket(1500), 5);
        assert_eq!(hist_bucket(50_000), 6);
    }

    #[test]
    fn driver_stat_window() {
        let mut a = DriverStat::default();
        a.add(100, false);
        let prev = a;
        a.add(900, false);
        a.add(20, true);
        let w = a.since(&prev);
        assert_eq!((w.dpc_count, w.dpc_total_us, w.isr_count), (1, 900, 1));
    }
}
