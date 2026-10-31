//! In-memory time series. Everything is bounded and pre-sized: memory use is fixed no matter
//! how long Blackbox runs.
//!
//! Time base: `Micros` = microseconds on a monotonic clock (QPC) since the engine started.

use crate::model::{GpuThrottle, ProcTag};
use std::collections::VecDeque;
use std::sync::Arc;

pub type Micros = i64;

/// Resolution of kernel-level series (DPC/ISR, disk, hard faults).
pub const BUCKET_US: Micros = 100_000;
/// Physical disks tracked individually; higher disk numbers are folded into the last slot.
pub const MAX_DISKS: usize = 8;
/// Kernel buckets kept in memory (2 minutes).
pub const KERNEL_RING_LEN: usize = 1200;
/// Per-second samples kept in memory (3 minutes).
pub const SECOND_RING_LEN: usize = 180;

/// Driver id `u16::MAX` means "unknown module".
pub const NO_DRIVER: u16 = u16::MAX;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DiskAcc {
    pub ios: u32,
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub lat_sum_us: u64,
    pub lat_max_us: u32,
}

impl DiskAcc {
    pub fn add_io(&mut self, write: bool, bytes: u32, lat_us: u32) {
        self.ios += 1;
        if write {
            self.write_bytes += bytes as u64;
        } else {
            self.read_bytes += bytes as u64;
        }
        self.lat_sum_us += lat_us as u64;
        self.lat_max_us = self.lat_max_us.max(lat_us);
    }

    pub fn merge(&mut self, o: &DiskAcc) {
        self.ios += o.ios;
        self.read_bytes += o.read_bytes;
        self.write_bytes += o.write_bytes;
        self.lat_sum_us += o.lat_sum_us;
        self.lat_max_us = self.lat_max_us.max(o.lat_max_us);
    }

    pub fn bytes(&self) -> u64 {
        self.read_bytes + self.write_bytes
    }
}

/// Aggregated kernel activity over `BUCKET_US`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KernelBucket {
    pub idx: i64,
    pub dpc_count: u32,
    pub dpc_total_us: u32,
    pub dpc_max_us: u32,
    pub dpc_max_drv: u16,
    pub isr_count: u32,
    pub isr_total_us: u32,
    pub isr_max_us: u32,
    pub isr_max_drv: u16,
    pub hard_faults: u32,
    pub disks: [DiskAcc; MAX_DISKS],
}

impl KernelBucket {
    pub fn empty(idx: i64) -> KernelBucket {
        KernelBucket {
            idx,
            dpc_count: 0,
            dpc_total_us: 0,
            dpc_max_us: 0,
            dpc_max_drv: NO_DRIVER,
            isr_count: 0,
            isr_total_us: 0,
            isr_max_us: 0,
            isr_max_drv: NO_DRIVER,
            hard_faults: 0,
            disks: [DiskAcc::default(); MAX_DISKS],
        }
    }

    pub fn start_us(&self) -> Micros {
        self.idx * BUCKET_US
    }

    pub fn add_dpc(&mut self, us: u32, drv: u16) {
        self.dpc_count += 1;
        self.dpc_total_us = self.dpc_total_us.saturating_add(us);
        if us > self.dpc_max_us {
            self.dpc_max_us = us;
            self.dpc_max_drv = drv;
        }
    }

    pub fn add_isr(&mut self, us: u32, drv: u16) {
        self.isr_count += 1;
        self.isr_total_us = self.isr_total_us.saturating_add(us);
        if us > self.isr_max_us {
            self.isr_max_us = us;
            self.isr_max_drv = drv;
        }
    }

    pub fn disk_slot(disk: u32) -> usize {
        (disk as usize).min(MAX_DISKS - 1)
    }

    pub fn merge(&mut self, o: &KernelBucket) {
        debug_assert_eq!(self.idx, o.idx);
        self.dpc_count += o.dpc_count;
        self.dpc_total_us = self.dpc_total_us.saturating_add(o.dpc_total_us);
        if o.dpc_max_us > self.dpc_max_us {
            self.dpc_max_us = o.dpc_max_us;
            self.dpc_max_drv = o.dpc_max_drv;
        }
        self.isr_count += o.isr_count;
        self.isr_total_us = self.isr_total_us.saturating_add(o.isr_total_us);
        if o.isr_max_us > self.isr_max_us {
            self.isr_max_us = o.isr_max_us;
            self.isr_max_drv = o.isr_max_drv;
        }
        self.hard_faults += o.hard_faults;
        for (a, b) in self.disks.iter_mut().zip(o.disks.iter()) {
            a.merge(b);
        }
    }

    pub fn disk_bytes(&self) -> u64 {
        self.disks.iter().map(DiskAcc::bytes).sum()
    }

    pub fn disk_lat_max_us(&self) -> u32 {
        self.disks.iter().map(|d| d.lat_max_us).max().unwrap_or(0)
    }

    pub fn disk_ios(&self) -> u32 {
        self.disks.iter().map(|d| d.ios).sum()
    }
}

/// Contiguous ring of kernel buckets indexed by bucket number. Late buckets (ETW delivers
/// per-CPU buffers out of order) are merged in place as long as they are still in the window.
#[derive(Debug)]
pub struct KernelRing {
    buckets: VecDeque<KernelBucket>,
    cap: usize,
}

impl KernelRing {
    pub fn new(cap: usize) -> KernelRing {
        KernelRing { buckets: VecDeque::with_capacity(cap), cap }
    }

    pub fn first_idx(&self) -> Option<i64> {
        self.buckets.front().map(|b| b.idx)
    }

    pub fn last_idx(&self) -> Option<i64> {
        self.buckets.back().map(|b| b.idx)
    }

    pub fn merge(&mut self, b: &KernelBucket) {
        let Some(first) = self.first_idx() else {
            self.buckets.push_back(*b);
            return;
        };
        if b.idx < first {
            return; // too old
        }
        let last = self.last_idx().unwrap_or(first);
        if b.idx > last {
            // Never pad more than the ring can hold.
            let gap = (b.idx - last - 1).min(self.cap as i64) as usize;
            let start = b.idx - gap as i64;
            for i in 0..gap {
                self.push_back(KernelBucket::empty(start + i as i64));
            }
            self.push_back(*b);
        } else {
            let pos = (b.idx - first) as usize;
            self.buckets[pos].merge(b);
        }
    }

    fn push_back(&mut self, b: KernelBucket) {
        if self.buckets.len() == self.cap {
            self.buckets.pop_front();
        }
        self.buckets.push_back(b);
    }

    /// Buckets whose start lies in `[from, to)`.
    pub fn range(&self, from: Micros, to: Micros) -> impl Iterator<Item = &KernelBucket> {
        self.buckets.iter().filter(move |b| {
            let s = b.start_us();
            s >= from && s < to
        })
    }

    /// Copy of the buckets in `[from, to)`, padded with empty buckets so the result is contiguous.
    pub fn window(&self, from: Micros, to: Micros) -> Vec<KernelBucket> {
        let i0 = from.div_euclid(BUCKET_US);
        let i1 = to.div_euclid(BUCKET_US);
        let mut out = Vec::with_capacity((i1 - i0).max(0) as usize);
        let first = self.first_idx();
        for idx in i0..i1 {
            let b = first
                .and_then(|f| {
                    let pos = idx - f;
                    (pos >= 0).then(|| self.buckets.get(pos as usize)).flatten()
                })
                .copied()
                .unwrap_or_else(|| KernelBucket::empty(idx));
            out.push(b);
        }
        out
    }

    pub fn len(&self) -> usize {
        self.buckets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buckets.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct ProcUse {
    pub pid: u32,
    pub name: Arc<str>,
    pub tag: Option<ProcTag>,
    pub cpu_pct: f32,
    pub io_bps: f64,
    pub hard_faults: u32,
    pub ram_mb: f32,
    pub gpu_pct: Option<f32>,
}

#[derive(Debug, Clone, Default)]
pub struct GpuSample {
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
    pub throttle: Vec<GpuThrottle>,
}

/// Machine state over one second, sampled by the engine tick.
#[derive(Debug, Clone, Default)]
pub struct SecondSample {
    /// End of the sampled second.
    pub t_us: Micros,
    pub cpu_total: f32,
    pub cpu_kernel: f32,
    pub dpc_pct: f32,
    pub isr_pct: f32,
    pub mem_total_mb: u32,
    pub mem_avail_mb: u32,
    pub mem_load: f32,
    pub commit_mb: u32,
    pub commit_limit_mb: u32,
    pub hard_faults: u32,
    pub freq_limit_pct: f32,
    pub gpu: Option<GpuSample>,
    pub top_cpu: Vec<ProcUse>,
    pub top_io: Vec<ProcUse>,
    /// Biggest working sets.
    pub top_mem: Vec<ProcUse>,
    /// Biggest GPU users (NVIDIA only).
    pub top_gpu: Vec<ProcUse>,
    pub cpu_temp_c: Option<f32>,
    pub fg_pid: u32,
}

impl SecondSample {
    pub fn start_us(&self) -> Micros {
        self.t_us - 1_000_000
    }
    pub fn overlaps(&self, from: Micros, to: Micros) -> bool {
        self.start_us() < to && self.t_us > from
    }
}

#[derive(Debug, Clone)]
pub struct ProcEvent {
    pub t_us: Micros,
    pub pid: u32,
    pub ppid: u32,
    pub start: bool,
    pub name: Arc<str>,
    pub parent_name: Option<Arc<str>>,
    pub tag: Option<ProcTag>,
}

/// Bounded FIFO.
#[derive(Debug)]
pub struct Ring<T> {
    items: VecDeque<T>,
    cap: usize,
}

impl<T> Ring<T> {
    pub fn new(cap: usize) -> Ring<T> {
        Ring { items: VecDeque::with_capacity(cap), cap }
    }
    pub fn push(&mut self, v: T) {
        if self.items.len() == self.cap {
            self.items.pop_front();
        }
        self.items.push_back(v);
    }
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &T> + ExactSizeIterator {
        self.items.iter()
    }
    pub fn iter_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut T> + ExactSizeIterator {
        self.items.iter_mut()
    }
    pub fn last(&self) -> Option<&T> {
        self.items.back()
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn clear(&mut self) {
        self.items.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bucket(idx: i64, dpc: u32) -> KernelBucket {
        let mut b = KernelBucket::empty(idx);
        b.add_dpc(dpc, 1);
        b
    }

    #[test]
    fn ring_merges_late_buckets_and_pads_gaps() {
        let mut r = KernelRing::new(10);
        r.merge(&bucket(5, 100));
        r.merge(&bucket(8, 300));
        assert_eq!(r.len(), 4);
        r.merge(&bucket(6, 900)); // late
        let w = r.window(5 * BUCKET_US, 9 * BUCKET_US);
        assert_eq!(w.iter().map(|b| b.dpc_max_us).collect::<Vec<_>>(), vec![100, 900, 0, 300]);
        r.merge(&bucket(4, 1)); // older than the window start: dropped
        assert_eq!(r.first_idx(), Some(5));
    }

    #[test]
    fn ring_is_bounded() {
        let mut r = KernelRing::new(10);
        r.merge(&bucket(0, 1));
        r.merge(&bucket(1000, 1)); // huge gap: padding is capped
        assert_eq!(r.len(), 10);
        assert_eq!(r.last_idx(), Some(1000));
    }

    #[test]
    fn window_pads_missing_data() {
        let r = KernelRing::new(10);
        let w = r.window(0, 3 * BUCKET_US);
        assert_eq!(w.len(), 3);
        assert!(w.iter().all(|b| b.dpc_count == 0));
    }

    #[test]
    fn merge_keeps_max_driver() {
        let mut a = KernelBucket::empty(1);
        a.add_dpc(10, 3);
        let mut b = KernelBucket::empty(1);
        b.add_dpc(50, 7);
        b.add_isr(20, 9);
        a.merge(&b);
        assert_eq!((a.dpc_max_us, a.dpc_max_drv, a.dpc_count), (50, 7, 2));
        assert_eq!((a.isr_max_us, a.isr_max_drv), (20, 9));
    }
}
