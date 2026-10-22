//! Minimal real-time ETW session + consumer.
//!
//! Design goals: no TDH schema lookups on hot paths (payloads of the few events we use are
//! parsed at fixed offsets), event-id filtering in the kernel for user providers, raw QPC
//! timestamps, and a single consumer thread per session that publishes aggregates once per
//! ETW buffer instead of once per event.

pub mod kernel;
pub mod present;

use crate::util::{guid, wide};
use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use windows_sys::core::GUID;
use windows_sys::Win32::System::Diagnostics::Etw::{
    CloseTrace, ControlTraceW, EnableTraceEx2, OpenTraceW, ProcessTrace, StartTraceW, CONTROLTRACE_HANDLE,
    ENABLE_TRACE_PARAMETERS, ENABLE_TRACE_PARAMETERS_VERSION_2, EVENT_CONTROL_CODE_ENABLE_PROVIDER, EVENT_FILTER_DESCRIPTOR,
    EVENT_FILTER_TYPE_EVENT_ID, EVENT_RECORD, EVENT_TRACE_CONTROL_QUERY, EVENT_TRACE_CONTROL_STOP, EVENT_TRACE_LOGFILEW,
    EVENT_TRACE_PROPERTIES, EVENT_TRACE_REAL_TIME_MODE, EVENT_TRACE_SYSTEM_LOGGER_MODE, PROCESSTRACE_HANDLE,
    PROCESS_TRACE_MODE_EVENT_RECORD, PROCESS_TRACE_MODE_RAW_TIMESTAMP, PROCESS_TRACE_MODE_REAL_TIME, WNODE_FLAG_TRACED_GUID,
};

pub const ERROR_ALREADY_EXISTS: u32 = 183;
pub const ERROR_ACCESS_DENIED: u32 = 5;
const INVALID_PROCESSTRACE_HANDLE: u64 = u64::MAX;
/// Room after EVENT_TRACE_PROPERTIES for the logger name and log file name.
const PROPS_TAIL_BYTES: usize = 2048;

pub const SYSTEM_TRACE_CONTROL_GUID: GUID =
    guid(0x9e814aad, 0x3204, 0x11d2, [0x9a, 0x82, 0x00, 0x60, 0x08, 0xa8, 0x69, 0x39]);

#[derive(Clone)]
pub struct ProviderSpec {
    pub guid: GUID,
    pub level: u8,
    pub any_keyword: u64,
    /// When non-empty, only these event ids are produced (filtered inside the provider).
    pub event_ids: Vec<u16>,
}

#[derive(Clone)]
pub struct SessionSpec {
    pub name: String,
    pub session_guid: GUID,
    /// SystemTraceProvider flags (EVENT_TRACE_FLAG_*). Non-zero makes this a system logger.
    pub kernel_flags: u32,
    pub providers: Vec<ProviderSpec>,
    pub buffer_kb: u32,
    pub min_buffers: u32,
    pub max_buffers: u32,
    pub flush_s: u32,
}

/// Receives events on the session's consumer thread.
pub trait EventSink: Send + 'static {
    fn on_event(&mut self, rec: &EVENT_RECORD);
    /// Called after each ETW buffer: publish what was aggregated.
    fn on_buffer(&mut self) {}
}

struct Ctx {
    sink: Box<dyn EventSink>,
    local_events: u64,
    events: Arc<AtomicU64>,
}

struct SendPtr(*mut Ctx);
unsafe impl Send for SendPtr {}

#[derive(Debug, Clone, Copy, Default)]
pub struct SessionStats {
    pub events_lost: u64,
    pub buffers_lost: u64,
}

pub struct Session {
    name: Vec<u16>,
    control: CONTROLTRACE_HANDLE,
    trace: PROCESSTRACE_HANDLE,
    thread: Option<JoinHandle<()>>,
    events: Arc<AtomicU64>,
    // Kept alive for the lifetime of the consumer.
    _logfile: Box<EVENT_TRACE_LOGFILEW>,
}

unsafe impl Send for Session {}

fn props_buffer() -> Vec<u64> {
    let bytes = std::mem::size_of::<EVENT_TRACE_PROPERTIES>() + PROPS_TAIL_BYTES;
    let mut buf = vec![0u64; bytes.div_ceil(8)];
    let p = buf.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES;
    unsafe {
        (*p).Wnode.BufferSize = (buf.len() * 8) as u32;
        (*p).LoggerNameOffset = std::mem::size_of::<EVENT_TRACE_PROPERTIES>() as u32;
        (*p).LogFileNameOffset = 0;
    }
    buf
}

/// Stops a session by name (e.g. left over by a crash).
pub fn stop_by_name(name: &str) -> u32 {
    let w = wide(name);
    let mut buf = props_buffer();
    unsafe {
        ControlTraceW(
            CONTROLTRACE_HANDLE { Value: 0 },
            w.as_ptr(),
            buf.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES,
            EVENT_TRACE_CONTROL_STOP,
        )
    }
}

unsafe extern "system" fn on_event(rec: *mut EVENT_RECORD) {
    // SAFETY: ETW passes a valid record for the duration of the callback; UserContext is the
    // Ctx we registered, only ever touched from this consumer thread.
    let (rec, ctx) = unsafe {
        let rec = &*rec;
        (rec, rec.UserContext as *mut Ctx)
    };
    if ctx.is_null() {
        return;
    }
    let ctx = unsafe { &mut *ctx };
    ctx.local_events += 1;
    let _ = catch_unwind(AssertUnwindSafe(|| ctx.sink.on_event(rec)));
}

unsafe extern "system" fn on_buffer(logfile: *mut EVENT_TRACE_LOGFILEW) -> u32 {
    // SAFETY: same contract as `on_event`.
    let ctx = unsafe { (*logfile).Context as *mut Ctx };
    if !ctx.is_null() {
        let ctx = unsafe { &mut *ctx };
        ctx.events.fetch_add(std::mem::take(&mut ctx.local_events), Ordering::Relaxed);
        let _ = catch_unwind(AssertUnwindSafe(|| ctx.sink.on_buffer()));
    }
    1 // keep processing
}

fn enable_provider(control: CONTROLTRACE_HANDLE, p: &ProviderSpec) -> u32 {
    // EVENT_FILTER_EVENT_ID: { FilterIn: u8, Reserved: u8, Count: u16, Events: [u16] }
    let mut filter: Vec<u16> = Vec::with_capacity(2 + p.event_ids.len());
    filter.push(1); // FilterIn = TRUE, Reserved = 0
    filter.push(p.event_ids.len() as u16);
    filter.extend_from_slice(&p.event_ids);
    let mut desc = EVENT_FILTER_DESCRIPTOR {
        Ptr: filter.as_ptr() as u64,
        Size: (filter.len() * 2) as u32,
        Type: EVENT_FILTER_TYPE_EVENT_ID,
    };
    let mut params: ENABLE_TRACE_PARAMETERS = unsafe { std::mem::zeroed() };
    params.Version = ENABLE_TRACE_PARAMETERS_VERSION_2;
    if !p.event_ids.is_empty() {
        params.EnableFilterDesc = &mut desc;
        params.FilterDescCount = 1;
    }
    unsafe {
        EnableTraceEx2(control, &p.guid, EVENT_CONTROL_CODE_ENABLE_PROVIDER, p.level, p.any_keyword, 0, 0, &params)
    }
}

impl Session {
    pub fn start(spec: &SessionSpec, sink: Box<dyn EventSink>) -> Result<Session, u32> {
        let name = wide(&spec.name);
        let mut buf = props_buffer();
        let p = buf.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES;
        let mut control = CONTROLTRACE_HANDLE { Value: 0 };
        unsafe {
            (*p).Wnode.Guid = spec.session_guid;
            (*p).Wnode.ClientContext = 1; // QPC timestamps
            (*p).Wnode.Flags = WNODE_FLAG_TRACED_GUID;
            (*p).BufferSize = spec.buffer_kb;
            (*p).MinimumBuffers = spec.min_buffers;
            (*p).MaximumBuffers = spec.max_buffers;
            (*p).FlushTimer = spec.flush_s;
            (*p).LogFileMode = EVENT_TRACE_REAL_TIME_MODE;
            if spec.kernel_flags != 0 {
                (*p).LogFileMode |= EVENT_TRACE_SYSTEM_LOGGER_MODE;
                (*p).EnableFlags = spec.kernel_flags;
            }
            let mut rc = StartTraceW(&mut control, name.as_ptr(), p);
            if rc == ERROR_ALREADY_EXISTS {
                log::info!("etw: session {} left over, restarting it", spec.name);
                stop_by_name(&spec.name);
                rc = StartTraceW(&mut control, name.as_ptr(), p);
            }
            if rc != 0 {
                return Err(rc);
            }
        }
        for prov in &spec.providers {
            let rc = enable_provider(control, prov);
            if rc != 0 {
                stop_by_name(&spec.name);
                return Err(rc);
            }
        }

        let events = Arc::new(AtomicU64::new(0));
        let ctx = Box::into_raw(Box::new(Ctx { sink, local_events: 0, events: events.clone() }));
        let mut logfile: Box<EVENT_TRACE_LOGFILEW> = Box::new(unsafe { std::mem::zeroed() });
        logfile.LoggerName = name.as_ptr() as *mut u16;
        logfile.Anonymous1.ProcessTraceMode =
            PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD | PROCESS_TRACE_MODE_RAW_TIMESTAMP;
        logfile.Anonymous2.EventRecordCallback = Some(on_event);
        logfile.BufferCallback = Some(on_buffer);
        logfile.Context = ctx as *mut c_void;
        let trace = unsafe { OpenTraceW(&mut *logfile) };
        if trace.Value == INVALID_PROCESSTRACE_HANDLE {
            let err = unsafe { windows_sys::Win32::Foundation::GetLastError() };
            stop_by_name(&spec.name);
            drop(unsafe { Box::from_raw(ctx) });
            return Err(err);
        }
        let ctx_ptr = SendPtr(ctx);
        let trace_value = trace.Value;
        let thread_name = format!("bb-etw-{}", spec.name.to_ascii_lowercase());
        let spawned = std::thread::Builder::new().name(thread_name).spawn(move || {
            let ctx_ptr = ctx_ptr;
            // Normal priority on purpose: the consumer sleeps between buffers, and starving it
            // when the CPU is saturated (exactly when incidents happen) would lose events.
            let handle = PROCESSTRACE_HANDLE { Value: trace_value };
            let rc = unsafe { ProcessTrace(&handle, 1, std::ptr::null(), std::ptr::null()) };
            if rc != 0 && rc != 1223 {
                log::warn!("etw: ProcessTrace ended with {rc}");
            }
            // SAFETY: ProcessTrace has returned, no callback can run anymore.
            drop(unsafe { Box::from_raw(ctx_ptr.0) });
        });
        match spawned {
            Ok(thread) => Ok(Session { name, control, trace, thread: Some(thread), events, _logfile: logfile }),
            Err(_) => {
                unsafe { CloseTrace(trace) };
                stop_by_name(&spec.name);
                // SAFETY: the consumer thread never ran, nothing else references ctx.
                drop(unsafe { Box::from_raw(ctx) });
                Err(8)
            }
        }
    }

    /// False once the consumer thread has ended (session stopped from outside, error).
    pub fn is_alive(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| !t.is_finished())
    }

    /// Events delivered to the sink since start.
    pub fn events(&self) -> u64 {
        self.events.load(Ordering::Relaxed)
    }

    pub fn stats(&self) -> SessionStats {
        let mut buf = props_buffer();
        let p = buf.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES;
        let rc = unsafe { ControlTraceW(self.control, std::ptr::null(), p, EVENT_TRACE_CONTROL_QUERY) };
        if rc != 0 {
            return SessionStats::default();
        }
        unsafe {
            SessionStats { events_lost: (*p).EventsLost as u64, buffers_lost: ((*p).RealTimeBuffersLost + (*p).LogBuffersLost) as u64 }
        }
    }

    pub fn stop(&mut self) {
        if let Some(t) = self.thread.take() {
            let mut buf = props_buffer();
            unsafe {
                ControlTraceW(self.control, self.name.as_ptr(), buf.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES, EVENT_TRACE_CONTROL_STOP);
                CloseTrace(self.trace);
            }
            let _ = t.join();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Payload of an event as a byte slice.
pub fn user_data(rec: &EVENT_RECORD) -> &[u8] {
    if rec.UserData.is_null() || rec.UserDataLength == 0 {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(rec.UserData as *const u8, rec.UserDataLength as usize) }
}

#[inline]
pub fn read_u16(d: &[u8], off: usize) -> Option<u16> {
    d.get(off..off + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

#[inline]
pub fn read_u32(d: &[u8], off: usize) -> Option<u32> {
    d.get(off..off + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

#[inline]
pub fn read_u64(d: &[u8], off: usize) -> Option<u64> {
    d.get(off..off + 8).map(|b| u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
}
