//! Windows collectors for Blackbox.
//!
//! - [`etw`]: real-time ETW sessions (kernel: DPC/ISR/disk/hard faults/processes; frames: DXGI/D3D9)
//! - [`snapshot`]: one-syscall snapshots of every process and every logical processor
//! - [`system`]: memory, CPU frequency limits, power plan, battery, network, own footprint
//! - [`nvml`]: NVIDIA GPU telemetry (loaded dynamically)
//! - [`cputemp`]: CPU temperature through the PawnIO driver, when installed
//! - [`foreground`]: event-driven foreground window tracking
//! - [`input`]: cursor, keyboard/mouse idle time and controller activity (no hooks)
//! - [`drivers`]: kernel module map for DPC/ISR attribution
//! - [`sysinfo`]: static machine description and configuration checks
#![cfg(windows)]

pub mod cputemp;
pub mod drivers;
pub mod etw;
pub mod foreground;
pub mod input;
pub mod nt;
pub mod nvml;
pub mod snapshot;
pub mod sysinfo;
pub mod system;
pub mod util;
