//! Whole-system snapshots with one syscall each: every process (CPU, I/O, memory, hard faults)
//! and every logical processor (busy, kernel, DPC and interrupt time).

use crate::nt::{
    query_system_information, AlignedBuf, SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION, SYSTEM_PROCESS_INFORMATION,
};
use std::collections::HashMap;
use std::sync::Arc;

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *const u16,
}

#[repr(C)]
struct SystemProcessInformation {
    next_entry_offset: u32,
    number_of_threads: u32,
    working_set_private_size: i64,
    hard_fault_count: u32,
    number_of_threads_high_watermark: u32,
    cycle_time: u64,
    create_time: i64,
    user_time: i64,
    kernel_time: i64,
    image_name: UnicodeString,
    base_priority: i32,
    unique_process_id: usize,
    inherited_from_unique_process_id: usize,
    handle_count: u32,
    session_id: u32,
    unique_process_key: usize,
    peak_virtual_size: usize,
    virtual_size: usize,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
    private_page_count: usize,
    read_operation_count: i64,
    write_operation_count: i64,
    other_operation_count: i64,
    read_transfer_count: i64,
    write_transfer_count: i64,
    other_transfer_count: i64,
}

const _: () = assert!(std::mem::size_of::<SystemProcessInformation>() == 256);

#[repr(C)]
#[derive(Clone, Copy)]
struct ProcessorPerformance {
    idle_time: i64,
    kernel_time: i64,
    user_time: i64,
    dpc_time: i64,
    interrupt_time: i64,
    interrupt_count: u32,
    _pad: u32,
}

const _: () = assert!(std::mem::size_of::<ProcessorPerformance>() == 48);

/// Cumulative counters of one process at snapshot time.
#[derive(Debug, Clone)]
pub struct ProcRaw {
    pub pid: u32,
    pub ppid: u32,
    pub create_time: i64,
    pub name: Arc<str>,
    /// User + kernel time, 100 ns units.
    pub cpu_100ns: i64,
    /// Read + write + other transfer bytes (all devices, including network and cache hits).
    pub io_bytes: u64,
    pub working_set: u64,
    pub private_bytes: u64,
    pub hard_faults: u32,
    pub threads: u32,
}

/// Cumulative counters of one logical processor, 100 ns units.
#[derive(Debug, Clone, Copy, Default)]
pub struct CoreTimes {
    pub idle: i64,
    pub kernel: i64, // includes idle
    pub user: i64,
    pub dpc: i64,
    pub interrupt: i64,
}

#[derive(Debug)]
pub struct Snapshotter {
    proc_buf: AlignedBuf,
    cpu_buf: AlignedBuf,
    names: HashMap<(u32, i64), Arc<str>>,
    generation: u64,
    seen: HashMap<(u32, i64), u64>,
}

impl Default for Snapshotter {
    fn default() -> Self {
        Snapshotter::new()
    }
}

impl Snapshotter {
    pub fn new() -> Snapshotter {
        Snapshotter {
            proc_buf: AlignedBuf::with_bytes(1024 * 1024),
            cpu_buf: AlignedBuf::with_bytes(64 * 48),
            names: HashMap::with_capacity(512),
            generation: 0,
            seen: HashMap::with_capacity(512),
        }
    }

    /// Fills `out` with every process. Reuses `out`'s allocation.
    pub fn processes(&mut self, out: &mut Vec<ProcRaw>) -> Result<(), i32> {
        out.clear();
        let len = query_system_information(SYSTEM_PROCESS_INFORMATION, &mut self.proc_buf)?;
        self.generation += 1;
        let base = self.proc_buf.as_ptr();
        let mut offset = 0usize;
        let entry = std::mem::size_of::<SystemProcessInformation>();
        while offset + entry <= len {
            // SAFETY: offset stays inside the bytes written by the kernel; entries are 8-aligned.
            let p = unsafe { &*(base.add(offset) as *const SystemProcessInformation) };
            let pid = p.unique_process_id as u32;
            let key = (pid, p.create_time);
            let name = match self.names.get(&key) {
                Some(n) => n.clone(),
                None => {
                    let n: Arc<str> = if pid == 0 {
                        Arc::from("Idle")
                    } else if p.image_name.buffer.is_null() || p.image_name.length == 0 {
                        Arc::from("System")
                    } else {
                        let chars = (p.image_name.length / 2) as usize;
                        // SAFETY: the kernel points Buffer into our own buffer, `length` bytes long.
                        let s = unsafe { std::slice::from_raw_parts(p.image_name.buffer, chars) };
                        Arc::from(String::from_utf16_lossy(s))
                    };
                    self.names.insert(key, n.clone());
                    n
                }
            };
            self.seen.insert(key, self.generation);
            out.push(ProcRaw {
                pid,
                ppid: p.inherited_from_unique_process_id as u32,
                create_time: p.create_time,
                name,
                cpu_100ns: p.user_time + p.kernel_time,
                io_bytes: (p.read_transfer_count + p.write_transfer_count + p.other_transfer_count).max(0) as u64,
                working_set: p.working_set_size as u64,
                private_bytes: p.pagefile_usage as u64,
                hard_faults: p.hard_fault_count,
                threads: p.number_of_threads,
            });
            if p.next_entry_offset == 0 {
                break;
            }
            offset += p.next_entry_offset as usize;
        }
        // Forget names of processes gone for a while (bounded memory).
        if self.generation % 30 == 0 {
            let g = self.generation;
            self.seen.retain(|_, last| g - *last < 30);
            let seen = &self.seen;
            self.names.retain(|k, _| seen.contains_key(k));
        }
        Ok(())
    }

    /// Fills `out` with one entry per logical processor (current processor group).
    pub fn cores(&mut self, out: &mut Vec<CoreTimes>) -> Result<(), i32> {
        out.clear();
        let len = query_system_information(SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION, &mut self.cpu_buf)?;
        let n = len / std::mem::size_of::<ProcessorPerformance>();
        let base = self.cpu_buf.as_ptr() as *const ProcessorPerformance;
        for i in 0..n {
            // SAFETY: `n` entries were written by the kernel.
            let c = unsafe { *base.add(i) };
            out.push(CoreTimes {
                idle: c.idle_time,
                kernel: c.kernel_time,
                user: c.user_time,
                dpc: c.dpc_time,
                interrupt: c.interrupt_time,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_sees_this_process_and_cores() {
        let mut s = Snapshotter::new();
        let mut procs = Vec::new();
        s.processes(&mut procs).unwrap();
        let me = std::process::id();
        assert!(procs.iter().any(|p| p.pid == me));
        assert!(procs.iter().any(|p| p.pid == 4 && &*p.name == "System"));
        assert!(procs.len() > 20);
        let mut cores = Vec::new();
        s.cores(&mut cores).unwrap();
        assert!(!cores.is_empty());
        assert!(cores.iter().all(|c| c.kernel >= c.idle));
    }
}
