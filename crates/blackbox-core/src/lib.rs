//! Blackbox core: the platform-independent half of the recorder.
//!
//! - [`model`]: types shared with the UI (mirrors `src/lib/contract.ts`)
//! - [`series`]: bounded in-memory time series
//! - [`frames`]: frame pacing and hitch detection
//! - [`analyze`]: cause analysis and incident timelines
//! - [`health`]: global health state
//! - [`kb`]: cause -> consequence -> fix knowledge base, driver and process knowledge
//! - [`i18n`]: the few strings rendered natively

pub mod analyze;
pub mod frames;
pub mod health;
pub mod i18n;
pub mod kb;
pub mod model;
pub mod series;
