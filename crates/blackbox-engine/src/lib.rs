//! Blackbox recorder engine.
//!
//! One background thread ticks once per second: it drains what the ETW consumers aggregated,
//! takes one-syscall snapshots of processes and processors, runs the detectors, and analyzes
//! incidents once the data around them is complete. Queries from the UI read published state
//! and never wait on the tick.
#![cfg(windows)]

mod detect;
mod engine;
mod recorder;
mod status;
pub mod store;
mod sysinfo;

pub use engine::{Engine, EngineConfig, EngineSink, Notification, OverlayText, TrayState};
