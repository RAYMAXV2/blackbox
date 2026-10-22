//! Frame presents (DXGI for D3D10/11/12, D3D9) from user-mode providers, filtered in the
//! provider to the single "Present_Start" event. Only the foreground process's frames are kept.

use super::{read_u32, read_u64, user_data, EventSink, ProviderSpec, SessionSpec};
use crate::foreground::Foreground;
use crate::util::{guid, guid_eq, Clock};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use windows_sys::core::GUID;
use windows_sys::Win32::System::Diagnostics::Etw::EVENT_RECORD;

const DXGI: GUID = guid(0xca11c036, 0x0102, 0x4a2d, [0xa6, 0xad, 0xf0, 0x3c, 0xfe, 0xd5, 0xd3, 0xc9]);
const D3D9: GUID = guid(0x783aca0a, 0x790e, 0x4d7f, [0x84, 0x51, 0xaa, 0x85, 0x05, 0x11, 0xc6, 0xb9]);
const SESSION_GUID: GUID = guid(0xb1ac0b0c, 0x0d1e, 0x4b5f, [0x9a, 0x1d, 0x6b, 0x3c, 0x2e, 0x7f, 0x4a, 0x02]);

pub const SESSION_NAME: &str = "Blackbox-Frames";

const DXGI_PRESENT_START: u16 = 42;
const D3D9_PRESENT_START: u16 = 1;
const DXGI_PRESENT_TEST: u32 = 0x1;

pub fn spec() -> SessionSpec {
    SessionSpec {
        name: SESSION_NAME.into(),
        session_guid: SESSION_GUID,
        kernel_flags: 0,
        providers: vec![
            ProviderSpec { guid: DXGI, level: 5, any_keyword: u64::MAX, event_ids: vec![DXGI_PRESENT_START] },
            ProviderSpec { guid: D3D9, level: 5, any_keyword: u64::MAX, event_ids: vec![D3D9_PRESENT_START] },
        ],
        buffer_kb: 32,
        min_buffers: 2,
        max_buffers: 16,
        flush_s: 1,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PresentEvent {
    pub t_us: i64,
    pub pid: u32,
    pub swapchain: u64,
}

#[derive(Debug, Default)]
pub struct PresentData {
    pub frames: Vec<PresentEvent>,
    /// Presents per process since the last drain (who is rendering).
    pub counts: HashMap<u32, u32>,
}

pub struct PresentSink {
    clock: Clock,
    fg: Arc<Foreground>,
    pending: Vec<PresentEvent>,
    counts: HashMap<u32, u32>,
    shared: Arc<Mutex<PresentData>>,
}

impl PresentSink {
    pub fn new(clock: Clock, fg: Arc<Foreground>, shared: Arc<Mutex<PresentData>>) -> PresentSink {
        PresentSink { clock, fg, pending: Vec::with_capacity(512), counts: HashMap::with_capacity(32), shared }
    }
}

impl EventSink for PresentSink {
    fn on_event(&mut self, rec: &EVENT_RECORD) {
        let h = &rec.EventHeader;
        let id = h.EventDescriptor.Id;
        let data = user_data(rec);
        let dxgi = guid_eq(&h.ProviderId, &DXGI) && id == DXGI_PRESENT_START;
        let d3d9 = guid_eq(&h.ProviderId, &D3D9) && id == D3D9_PRESENT_START;
        if !dxgi && !d3d9 {
            return;
        }
        let swapchain = read_u64(data, 0).unwrap_or(0);
        if dxgi && read_u32(data, 8).unwrap_or(0) & DXGI_PRESENT_TEST != 0 {
            return; // occlusion test, not a frame
        }
        let pid = h.ProcessId;
        *self.counts.entry(pid).or_insert(0) += 1;
        if pid == self.fg.pid() {
            self.pending.push(PresentEvent { t_us: self.clock.qpc_to_us(h.TimeStamp), pid, swapchain });
        }
    }

    fn on_buffer(&mut self) {
        let Ok(mut s) = self.shared.lock() else {
            return;
        };
        s.frames.append(&mut self.pending);
        for (pid, n) in self.counts.drain() {
            *s.counts.entry(pid).or_insert(0) += n;
        }
    }
}
