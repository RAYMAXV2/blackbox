//! Windows collectors for Blackbox.
//!
//! - [`etw`]: real-time ETW sessions (kernel: DPC/ISR/disk/hard faults/processes; frames: DXGI/D3D9)
//! - [`snapshot`]: one-syscall snapshots of every process and every logical processor
//! - [`system`]: memory, CPU frequency limits, power plan, battery, network, own footprint
//! - [`nvml`]: NVIDIA GPU telemetry (loaded dynamically)
//! - [`foreground`]: event-driven foreground window tracking
//! - [`drivers`]: kernel module map for DPC/ISR attribution
//! - [`sysinfo`]: static machine description and configuration checks
#![cfg(windows)]

pub mod drivers;
pub mod etw;
pub mod foreground;
pub mod nt;
pub mod nvml;
pub mod snapshot;
pub mod sysinfo;
pub mod system;
pub mod util;
