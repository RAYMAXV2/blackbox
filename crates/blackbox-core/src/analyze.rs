//! Cause analysis: turns the data around an incident into ranked causes, the evidence behind
//! each one, and the checks that came back clean.
//!
//! Every rule is a small, explicit heuristic with a confidence in 0..1. Rules never guess
//! silently: what they saw goes into `evidence`, what they ruled out goes into `ruled_out`.

use crate::kb;
use crate::model::*;
use crate::params;
use crate::series::*;

/// Resolves driver ids recorded by the kernel collector.
pub trait DriverLookup {
    fn driver(&self, id: u16) -> Option<(&str, DriverCategory)>;
}

/// Simple owned driver table: index = driver id.
#[derive(Debug, Default, Clone)]
pub struct DriverTable {
    pub entries: Vec<(String, DriverCategory)>,
}

impl DriverLookup for DriverTable {
    fn driver(&self, id: u16) -> Option<(&str, DriverCategory)> {
        self.entries.get(id as usize).map(|(n, c)| (n.as_str(), *c))
    }
}

pub struct AnalysisInput<'a> {
    pub kind: IncidentKind,
    /// When the incident was observed (end of the long frame, spike time, hang detection).
    pub marker_us: Micros,
    /// Where the cause must be: e.g. the long frame itself, slightly widened.
    pub focus_from: Micros,
    pub focus_to: Micros,
    /// Hitches: how much longer than usual the frame took. Other kinds: 0.
    pub excess_ms: f32,
    /// Hangs: seconds not responding. Other kinds: unused.
    pub duration_s: f32,
    pub app: Option<(u32, &'a str)>,
    /// False in limited mode (not elevated): no DPC/ISR/disk latency data.
    pub kernel_tracing: bool,
    /// Kernel buckets covering at least `[focus_from - 20 s, focus_to]`.
    pub kernel: &'a [KernelBucket],
    pub seconds: &'a [SecondSample],
    pub events: &'a [ProcEvent],
    pub drivers: &'a dyn DriverLookup,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Verdict {
    pub causes: Vec<CauseMatch>,
    pub ruled_out: Vec<Reason>,
}

#[derive(Default)]
struct Candidates {
    list: Vec<(String, CauseMatch)>,
}

impl Candidates {
    fn add(&mut self, id: &str, confidence: f32, params: Params, evidence: Vec<Reason>) {
        let subject = ["process", "driver", "app"]
            .iter()
            .find_map(|k| match params.get(*k) {
                Some(ParamValue::Text(s)) => Some(s.to_ascii_lowercase()),
                _ => None,
            })
            .unwrap_or_default();
        let key = format!("{id}|{subject}");
        let confidence = confidence.clamp(0.0, 0.99);
        if let Some((_, c)) = self.list.iter_mut().find(|(k, _)| *k == key) {
            let stronger = confidence > c.confidence;
            if stronger {
                c.confidence = confidence;
            }
            // Merge per key: different rules know different facts about the same cause.
            for (k, v) in params {
                let keep_old = match (k.as_str(), c.params.get(&k), &v) {
                    // The highest CPU share observed.
                    ("pct", Some(ParamValue::Num(old)), ParamValue::Num(new)) => old >= new,
                    // "Started N s before" only comes from a real start event; 0 means unknown.
                    ("sec", Some(ParamValue::Num(old)), ParamValue::Num(new)) => *old > 0.0 || *new <= 0.0,
                    (_, Some(_), _) => !stronger,
                    (_, None, _) => false,
                };
                if !keep_old {
                    c.params.insert(k, v);
                }
            }
            for e in evidence {
                if !c.evidence.contains(&e) {
                    c.evidence.push(e);
                }
            }
        } else {
            self.list.push((key, CauseMatch { id: id.to_owned(), confidence, params, evidence }));
        }
    }

    fn best(&self) -> f32 {
        self.list.iter().map(|(_, c)| c.confidence).fold(0.0, f32::max)
    }

    fn boost(&mut self, pred: impl Fn(&str) -> bool, floor: f32) {
        for (_, c) in &mut self.list {
            if pred(&c.id) && c.confidence < floor {
                c.confidence = floor;
            }
        }
    }

    fn finish(self) -> Vec<CauseMatch> {
        let mut v: Vec<CauseMatch> = self.list.into_iter().map(|(_, c)| c).collect();
        v.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
        v.truncate(5);
        for c in &mut v {
            c.confidence = (c.confidence * 100.0).round() / 100.0;
        }
        v
    }
}

fn r1(x: f32) -> f64 {
    ((x as f64) * 10.0).round() / 10.0
}

fn r0(x: f32) -> f64 {
    (x as f64).round()
}

fn ramp(x: f32, from: f32, to: f32) -> f32 {
    ((x - from) / (to - from)).clamp(0.0, 1.0)
}

fn median(v: &mut [f32]) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let mid = v.len() / 2;
    v.select_nth_unstable_by(mid, f32::total_cmp);
    v[mid]
}

fn push_unique(v: &mut Vec<Reason>, r: Reason) {
    if !v.iter().any(|x| x.key == r.key) {
        v.push(r);
    }
}

pub fn analyze(inp: &AnalysisInput) -> Verdict {
    let mut c = Candidates::default();
    let mut ruled: Vec<Reason> = Vec::new();
    let game_pid = inp.app.map(|a| a.0);
    let app_name = inp.app.map(|a| a.1).unwrap_or("");

    let focus: Vec<&KernelBucket> = inp
        .kernel
        .iter()
        .filter(|b| b.start_us() + BUCKET_US > inp.focus_from && b.start_us() < inp.focus_to)
        .collect();
    let baseline: Vec<&KernelBucket> = inp
        .kernel
        .iter()
        .filter(|b| b.start_us() >= inp.focus_from - 20_000_000 && b.start_us() < inp.focus_from - 1_000_000)
        .collect();
    let secs: Vec<&SecondSample> =
        inp.seconds.iter().filter(|s| s.overlaps(inp.focus_from - 500_000, inp.focus_to + 500_000)).collect();

    // ── A. driver latency (DPC / ISR)
    if inp.kernel_tracing {
        let mut worst = (0u32, NO_DRIVER, false);
        for b in &focus {
            if b.dpc_max_us > worst.0 {
                worst = (b.dpc_max_us, b.dpc_max_drv, false);
            }
            if b.isr_max_us > worst.0 {
                worst = (b.isr_max_us, b.isr_max_drv, true);
            }
        }
        let (us, drv, is_isr) = worst;
        if us >= 500 {
            let (name, cat) = inp.drivers.driver(drv).unwrap_or(("unknown", DriverCategory::Other));
            let conf = if inp.kind == IncidentKind::Latency {
                0.95
            } else {
                let ratio = (us as f32 / 1000.0) / inp.excess_ms.max(1.0);
                let base = (0.25 + 0.7 * ratio.min(1.0)).clamp(0.25, 0.92);
                if us >= 1000 { base } else { base * 0.6 }
            };
            let ev_key = if is_isr { "ev.isr_spike" } else { "ev.dpc_spike" };
            c.add(
                cat.cause_id(),
                conf,
                params! {"driver" => name, "us" => us},
                vec![Reason::new(ev_key, params! {"driver" => name, "us" => us})],
            );
        } else {
            ruled.push(Reason::new("ro.dpc", params! {"us" => us}));
        }
    }

    // ── B. disk
    let top_io = secs
        .iter()
        .flat_map(|s| s.top_io.iter())
        .filter(|p| !kb::is_system_pseudo_process(&p.name) && p.tag != Some(ProcTag::Blackbox))
        .max_by(|a, b| a.io_bps.total_cmp(&b.io_bps));
    if inp.kernel_tracing {
        let lat_max_ms = focus.iter().map(|b| b.disk_lat_max_us()).max().unwrap_or(0) as f32 / 1000.0;
        let bytes: u64 = focus.iter().map(|b| b.disk_bytes()).sum();
        let dur_s = (focus.len().max(1) as f32) * (BUCKET_US as f32 / 1e6);
        let mbps = bytes as f32 / dur_s / 1e6;
        let mut base: Vec<f32> =
            baseline.iter().map(|b| b.disk_bytes() as f32 / (BUCKET_US as f32 / 1e6) / 1e6).collect();
        let base_mbps = median(&mut base);
        let busy = lat_max_ms >= 40.0 || (mbps >= 50.0 && mbps >= 4.0 * base_mbps.max(5.0) && lat_max_ms >= 10.0);
        if busy {
            let sev = ramp(lat_max_ms, 10.0, 100.0);
            let busy_ev = Reason::new("ev.disk_busy", params! {"mbps" => r0(mbps), "latMs" => r0(lat_max_ms)});
            match top_io {
                Some(p) if Some(p.pid) == game_pid => {
                    let mb = r0((p.io_bps / 1e6) as f32);
                    c.add(
                        "io.game_streaming",
                        0.45 + 0.35 * sev,
                        params! {"app" => app_name, "mb" => mb, "latMs" => r0(lat_max_ms)},
                        vec![busy_ev, Reason::new("ev.game_io", params! {"mb" => mb})],
                    );
                }
                Some(p) => {
                    let mb = r0((p.io_bps / 1e6) as f32);
                    let conf = 0.5 + 0.35 * sev;
                    let top_ev = Reason::new("ev.disk_top_process", params! {"process" => &*p.name, "mb" => mb});
                    c.add(
                        "io.background_disk",
                        conf,
                        params! {"process" => &*p.name, "mb" => mb, "latMs" => r0(lat_max_ms)},
                        vec![busy_ev.clone(), top_ev.clone()],
                    );
                    if let Some(id) = p.tag.and_then(ProcTag::cause_id) {
                        c.add(
                            id,
                            conf + 0.05,
                            params! {"process" => &*p.name, "pct" => r0(p.cpu_pct), "sec" => 0},
                            vec![busy_ev, top_ev],
                        );
                    }
                }
                None => {
                    if inp.app.is_some() {
                        c.add(
                            "io.game_streaming",
                            0.35,
                            params! {"app" => app_name, "mb" => r0(mbps), "latMs" => r0(lat_max_ms)},
                            vec![busy_ev],
                        );
                    }
                }
            }
        } else {
            ruled.push(Reason::new("ro.disk", params! {"latMs" => r1(lat_max_ms)}));
        }
    } else if let Some(p) = top_io.filter(|p| Some(p.pid) != game_pid && p.io_bps >= 150e6) {
        let mb = r0((p.io_bps / 1e6) as f32);
        c.add(
            "io.background_disk",
            0.35,
            params! {"process" => &*p.name, "mb" => mb, "latMs" => "?"},
            vec![Reason::new("ev.disk_top_process", params! {"process" => &*p.name, "mb" => mb})],
        );
    }

    // ── C. memory pressure / paging
    let avail_mb = secs.iter().map(|s| s.mem_avail_mb).min().unwrap_or(0);
    let total_mb = secs.iter().map(|s| s.mem_total_mb).max().unwrap_or(0);
    if total_mb > 0 {
        let avail_pct = avail_mb as f32 * 100.0 / total_mb as f32;
        let (hard, base_hard, threshold) = if inp.kernel_tracing {
            let hard: u32 = focus.iter().map(|b| b.hard_faults).sum();
            let mut base: Vec<f32> = baseline.iter().map(|b| b.hard_faults as f32).collect();
            (hard as f32, median(&mut base) * focus.len().max(1) as f32, 100.0)
        } else {
            (secs.iter().map(|s| s.hard_faults).max().unwrap_or(0) as f32, 0.0, 300.0)
        };
        if hard >= threshold && (avail_pct < 15.0 || hard >= 10.0 * base_hard.max(10.0)) {
            let conf = 0.45 + 0.4 * ramp(hard, threshold, 2000.0) + if avail_pct < 10.0 { 0.1 } else { 0.0 };
            c.add(
                "mem.paging",
                conf,
                params! {"count" => r0(hard), "mb" => avail_mb},
                vec![
                    Reason::new("ev.hard_faults", params! {"count" => r0(hard)}),
                    Reason::new("ev.mem_available", params! {"mb" => avail_mb, "pct" => r0(avail_pct)}),
                ],
            );
        } else {
            ruled.push(Reason::new("ro.memory", params! {"mb" => avail_mb}));
        }
    }

    // ── D. processes that started just before
    let busy_pids: Vec<u32> = secs
        .iter()
        .flat_map(|s| s.top_cpu.iter().filter(|p| p.cpu_pct >= 5.0).chain(s.top_io.iter().filter(|p| p.io_bps >= 20e6)))
        .map(|p| p.pid)
        .collect();
    for ev in inp.events.iter().filter(|e| e.start && e.t_us >= inp.marker_us - 6_000_000 && e.t_us <= inp.focus_to) {
        if Some(ev.pid) == game_pid || ev.name.eq_ignore_ascii_case("conhost.exe") || ev.tag == Some(ProcTag::Blackbox) {
            continue;
        }
        let dt_s = ((inp.marker_us - ev.t_us).max(0) as f32) / 1e6;
        let decay = 1.0 - (dt_s / 6.0).min(1.0);
        let mut boost = if busy_pids.contains(&ev.pid) { 0.15 } else { 0.0 };
        if Some(ev.ppid) == game_pid {
            boost -= 0.15;
        }
        let from_scheduler = ev.parent_name.as_deref().is_some_and(kb::is_scheduler_host);
        let (id, conf) = match ev.tag.and_then(ProcTag::cause_id) {
            Some(id) => (id, 0.35 + 0.45 * decay),
            None if from_scheduler => ("proc.scheduled_task", 0.3 + 0.35 * decay),
            None => ("proc.new_process", 0.15 + 0.3 * decay),
        };
        let sec = r1(dt_s);
        c.add(
            id,
            conf + boost,
            params! {"process" => &*ev.name, "sec" => sec, "pct" => 0},
            vec![Reason::new("ev.proc_started", params! {"process" => &*ev.name, "sec" => sec})],
        );
    }

    // ── E. background CPU hogs
    let cpu_total = secs.iter().map(|s| s.cpu_total).fold(0.0, f32::max);
    let mut any_hog = false;
    for p in secs.iter().flat_map(|s| s.top_cpu.iter()) {
        if Some(p.pid) == game_pid
            || p.tag == Some(ProcTag::Blackbox)
            || kb::is_system_pseudo_process(&p.name)
            || p.cpu_pct < 8.0
        {
            continue;
        }
        any_hog = true;
        let conf = 0.25 + 0.5 * ramp(p.cpu_pct, 8.0, 38.0) + if cpu_total >= 85.0 { 0.15 } else { 0.0 };
        let id = p.tag.and_then(ProcTag::cause_id).unwrap_or("proc.background_cpu");
        c.add(
            id,
            conf,
            params! {"process" => &*p.name, "pct" => r0(p.cpu_pct), "sec" => 0},
            vec![Reason::new("ev.proc_cpu", params! {"process" => &*p.name, "pct" => r0(p.cpu_pct)})],
        );
    }
    if !any_hog && !secs.is_empty() {
        ruled.push(Reason::bare("ro.processes"));
    }

    // ── F. CPU saturation
    if !secs.is_empty() {
        if cpu_total >= 95.0 {
            let game_cpu = secs
                .iter()
                .flat_map(|s| s.top_cpu.iter())
                .filter(|p| Some(p.pid) == game_pid)
                .map(|p| p.cpu_pct)
                .fold(0.0, f32::max);
            let mut ev = vec![Reason::new("ev.cpu_total", params! {"pct" => r0(cpu_total)})];
            if game_pid.is_some() {
                ev.push(Reason::new("ev.game_cpu", params! {"pct" => r0(game_cpu)}));
            }
            c.add("cpu.saturated", 0.4 + 0.4 * ramp(cpu_total, 95.0, 100.0), params! {"pct" => r0(cpu_total)}, ev);
        } else {
            ruled.push(Reason::new("ro.cpu", params! {"pct" => r0(cpu_total)}));
        }
    }

    // ── G. CPU frequency limit
    let limit = secs.iter().map(|s| s.freq_limit_pct).fold(f32::INFINITY, f32::min);
    if limit.is_finite() && limit > 0.0 {
        if limit < 95.0 {
            c.add(
                "cpu.throttled",
                0.3 + 0.5 * ramp(95.0 - limit, 0.0, 40.0),
                params! {"pct" => r0(limit)},
                vec![Reason::new("ev.cpu_freq_limit", params! {"pct" => r0(limit)})],
            );
        } else {
            ruled.push(Reason::bare("ro.cpu_throttle"));
        }
    }

    // ── H. GPU
    let gpus: Vec<&GpuSample> = secs.iter().filter_map(|s| s.gpu.as_ref()).collect();
    if !gpus.is_empty() {
        let thermal = gpus.iter().flat_map(|g| g.throttle.iter()).find(|t| t.is_thermal()).copied();
        let power = gpus.iter().any(|g| g.throttle.contains(&GpuThrottle::SwPowerCap));
        let temp = gpus.iter().filter_map(|g| g.temp_c).fold(0.0, f32::max);
        let util = gpus.iter().map(|g| g.util_pct).fold(0.0, f32::max);
        if let Some(t) = thermal {
            c.add(
                "gpu.thermal",
                0.55,
                params! {"c" => r0(temp), "reason" => t.id()},
                vec![
                    Reason::new("ev.gpu_throttle", params! {"reason" => t.id()}),
                    Reason::new("ev.gpu_temp", params! {"c" => r0(temp)}),
                ],
            );
        }
        if power {
            c.add(
                "gpu.power_limit",
                if util >= 97.0 { 0.3 } else { 0.2 },
                params! {"reason" => "sw_power_cap"},
                vec![Reason::new("ev.gpu_throttle", params! {"reason" => "sw_power_cap"})],
            );
        }
        if thermal.is_none() && !power {
            ruled.push(Reason::bare("ro.gpu_throttle"));
        }
        let (used, total) = gpus.iter().map(|g| (g.vram_used_mb, g.vram_total_mb)).max().unwrap_or((0, 0));
        if total > 0 {
            let pct = used as f32 * 100.0 / total as f32;
            if pct >= 97.0 {
                c.add(
                    "gpu.vram_full",
                    0.5,
                    params! {"usedMb" => used, "totalMb" => total},
                    vec![Reason::new("ev.vram", params! {"usedMb" => used, "totalMb" => total})],
                );
            } else {
                ruled.push(Reason::new("ro.vram", params! {"pct" => r0(pct)}));
            }
        }
    }

    // ── I. what the incident itself tells us
    match inp.kind {
        IncidentKind::Hang => {
            let sec = r0(inp.duration_s);
            c.add(
                "app.not_responding",
                0.9,
                params! {"app" => app_name, "sec" => sec},
                vec![Reason::new("ev.not_responding", params! {"sec" => sec})],
            );
        }
        IncidentKind::Throttle => c.boost(|id| matches!(id, "cpu.throttled" | "gpu.thermal" | "gpu.power_limit"), 0.85),
        IncidentKind::Pressure => c.boost(|id| matches!(id, "mem.paging" | "io.background_disk"), 0.85),
        _ => {}
    }

    // ── J. nothing outside the app explains it
    if matches!(inp.kind, IncidentKind::Hitch | IncidentKind::Freeze) && c.best() < 0.35 {
        let conf = if c.list.is_empty() { 0.5 } else { 0.3 };
        c.add("game.internal", conf, params! {"app" => app_name}, vec![Reason::bare("ev.no_external")]);
    }

    let mut ruled_out = Vec::new();
    for r in ruled {
        push_unique(&mut ruled_out, r);
    }
    Verdict { causes: c.finish(), ruled_out }
}

pub struct TimelineInput<'a> {
    pub t0_us: Micros,
    pub t1_us: Micros,
    pub marker_us: Micros,
    /// `KernelRing::window(t0, t1)`: contiguous buckets.
    pub kernel: &'a [KernelBucket],
    pub seconds: &'a [SecondSample],
    pub events: &'a [ProcEvent],
    /// Frames already offset from `t0` (see `FrameTracker::frames_between`).
    pub frames: Vec<FramePoint>,
    pub drivers: &'a dyn DriverLookup,
    pub verdict: &'a Verdict,
    pub to_unix_ms: &'a dyn Fn(Micros) -> i64,
}

pub fn build_timeline(inp: TimelineInput) -> Timeline {
    let off = |t: Micros| (t - inp.t0_us) / 1000;
    let flagged_names: Vec<String> = inp
        .verdict
        .causes
        .iter()
        .flat_map(|c| ["process", "driver"].into_iter().filter_map(|k| c.params.get(k)))
        .filter_map(|v| match v {
            ParamValue::Text(s) => Some(s.to_ascii_lowercase()),
            _ => None,
        })
        .collect();
    let is_flagged = |name: &str| flagged_names.iter().any(|f| f.eq_ignore_ascii_case(name));

    let bucket_s = BUCKET_US as f32 / 1e6;
    let mut tl = Timeline {
        t0: (inp.to_unix_ms)(inp.t0_us),
        t1: (inp.to_unix_ms)(inp.t1_us),
        marker_ts: (inp.to_unix_ms)(inp.marker_us),
        bucket_ms: (BUCKET_US / 1000) as u32,
        frames: inp.frames,
        dpc_max_us: inp.kernel.iter().map(|b| b.dpc_max_us).collect(),
        isr_max_us: inp.kernel.iter().map(|b| b.isr_max_us).collect(),
        disk_mbps: inp.kernel.iter().map(|b| (b.disk_bytes() as f32 / bucket_s / 1e6 * 10.0).round() / 10.0).collect(),
        disk_lat_ms: inp.kernel.iter().map(|b| (b.disk_lat_max_us() as f32 / 100.0).round() / 10.0).collect(),
        hard_faults: inp.kernel.iter().map(|b| b.hard_faults).collect(),
        cpu_pct: Vec::new(),
        gpu_pct: Vec::new(),
        events: Vec::new(),
    };
    for s in inp.seconds.iter().filter(|s| s.t_us > inp.t0_us && s.start_us() < inp.t1_us) {
        let t = off(s.t_us.min(inp.t1_us));
        tl.cpu_pct.push(TimePoint { t, v: s.cpu_total.round() });
        if let Some(g) = &s.gpu {
            tl.gpu_pct.push(TimePoint { t, v: g.util_pct.round() });
        }
    }
    for e in inp.events.iter().filter(|e| e.t_us >= inp.t0_us && e.t_us < inp.t1_us) {
        tl.events.push(TimelineEvent {
            t: off(e.t_us),
            kind: if e.start { TimelineEventKind::ProcessStart } else { TimelineEventKind::ProcessEnd },
            label: e.name.to_string(),
            flagged: e.start && is_flagged(&e.name),
        });
    }
    // The worst driver spike in the window gets its own marker.
    if let Some(b) = inp.kernel.iter().max_by_key(|b| b.dpc_max_us.max(b.isr_max_us)) {
        let (us, drv) = if b.dpc_max_us >= b.isr_max_us { (b.dpc_max_us, b.dpc_max_drv) } else { (b.isr_max_us, b.isr_max_drv) };
        if us >= 500 {
            let name = inp.drivers.driver(drv).map(|(n, _)| n.to_owned()).unwrap_or_else(|| "unknown".into());
            tl.events.push(TimelineEvent {
                t: off(b.start_us()),
                kind: TimelineEventKind::Driver,
                flagged: is_flagged(&name),
                label: format!("{name} · {us} µs"),
            });
        }
    }
    tl.events.sort_by_key(|e| e.t);
    tl
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn table() -> DriverTable {
        DriverTable {
            entries: vec![
                ("ndis.sys".into(), DriverCategory::Network),
                ("nvlddmkm.sys".into(), DriverCategory::Gpu),
            ],
        }
    }

    fn quiet_kernel(from_s: i64, to_s: i64) -> Vec<KernelBucket> {
        (from_s * 10..to_s * 10)
            .map(|i| {
                let mut b = KernelBucket::empty(i);
                b.add_dpc(40, 1);
                b.disks[0].add_io(false, 64 * 1024, 300);
                b
            })
            .collect()
    }

    fn second(t_s: i64, cpu: f32) -> SecondSample {
        SecondSample {
            t_us: t_s * 1_000_000,
            cpu_total: cpu,
            mem_total_mb: 32768,
            mem_avail_mb: 20000,
            freq_limit_pct: 100.0,
            gpu: Some(GpuSample { util_pct: 80.0, vram_used_mb: 9000, vram_total_mb: 32768, ..Default::default() }),
            top_cpu: vec![ProcUse {
                pid: 100,
                name: Arc::from("Game.exe"),
                tag: None,
                cpu_pct: 30.0,
                io_bps: 1e6,
                hard_faults: 0,
            }],
            ..Default::default()
        }
    }

    fn input<'a>(
        kernel: &'a [KernelBucket],
        seconds: &'a [SecondSample],
        events: &'a [ProcEvent],
        drivers: &'a DriverTable,
    ) -> AnalysisInput<'a> {
        AnalysisInput {
            kind: IncidentKind::Hitch,
            marker_us: 30_000_000,
            focus_from: 29_800_000,
            focus_to: 30_050_000,
            excess_ms: 180.0,
            duration_s: 0.0,
            app: Some((100, "Game.exe")),
            kernel_tracing: true,
            kernel,
            seconds,
            events,
            drivers,
        }
    }

    #[test]
    fn clean_window_points_inside_the_game() {
        let k = quiet_kernel(5, 35);
        let s: Vec<_> = (25..33).map(|t| second(t, 40.0)).collect();
        let d = table();
        let v = analyze(&input(&k, &s, &[], &d));
        assert_eq!(v.causes[0].id, "game.internal");
        let keys: Vec<_> = v.ruled_out.iter().map(|r| r.key.as_str()).collect();
        for k in ["ro.dpc", "ro.disk", "ro.memory", "ro.processes", "ro.cpu", "ro.cpu_throttle", "ro.gpu_throttle", "ro.vram"] {
            assert!(keys.contains(&k), "missing {k} in {keys:?}");
        }
    }

    #[test]
    fn background_disk_hog_with_scheduled_start_wins() {
        let mut k = quiet_kernel(5, 35);
        for b in k.iter_mut().filter(|b| b.idx >= 296 && b.idx <= 300) {
            b.disks[0].add_io(false, 40 << 20, 120_000);
        }
        let mut s: Vec<_> = (25..33).map(|t| second(t, 45.0)).collect();
        for x in s.iter_mut().filter(|x| x.t_us == 30_000_000 || x.t_us == 31_000_000) {
            x.top_io.push(ProcUse {
                pid: 555,
                name: Arc::from("CompatTelRunner.exe"),
                tag: Some(ProcTag::Telemetry),
                cpu_pct: 6.0,
                io_bps: 210e6,
                hard_faults: 0,
            });
        }
        let ev = vec![ProcEvent {
            t_us: 29_600_000,
            pid: 555,
            ppid: 42,
            start: true,
            name: Arc::from("CompatTelRunner.exe"),
            parent_name: Some(Arc::from("taskhostw.exe")),
            tag: Some(ProcTag::Telemetry),
        }];
        let d = table();
        let v = analyze(&input(&k, &s, &ev, &d));
        assert_eq!(v.causes[0].id, "proc.telemetry", "{:#?}", v.causes);
        assert!(v.causes.iter().any(|c| c.id == "io.background_disk"));
        assert!(v.causes[0].evidence.iter().any(|e| e.key == "ev.proc_started"));
        // Facts from the disk rule and the start rule are merged, not overwritten.
        assert_eq!(v.causes[0].params.get("sec"), Some(&ParamValue::Num(0.4)));
        assert_eq!(v.causes[0].params.get("pct"), Some(&ParamValue::Num(6.0)));
        assert!(!v.ruled_out.iter().any(|r| r.key == "ro.disk"));
    }

    #[test]
    fn long_network_dpc_is_blamed_on_the_driver() {
        let mut k = quiet_kernel(5, 35);
        k.iter_mut().find(|b| b.idx == 299).unwrap().add_dpc(160_000, 0);
        let s: Vec<_> = (25..33).map(|t| second(t, 40.0)).collect();
        let d = table();
        let v = analyze(&input(&k, &s, &[], &d));
        assert_eq!(v.causes[0].id, "drv.network");
        assert_eq!(v.causes[0].params.get("driver"), Some(&ParamValue::Text("ndis.sys".into())));
        assert!(v.causes[0].confidence > 0.8);
    }

    #[test]
    fn hang_always_names_the_app() {
        let k = quiet_kernel(5, 35);
        let s: Vec<_> = (25..33).map(|t| second(t, 40.0)).collect();
        let d = table();
        let mut i = input(&k, &s, &[], &d);
        i.kind = IncidentKind::Hang;
        i.duration_s = 7.0;
        let v = analyze(&i);
        assert_eq!(v.causes[0].id, "app.not_responding");
    }

    #[test]
    fn timeline_has_contiguous_buckets_and_flags_verdict() {
        let mut k = quiet_kernel(24, 32);
        k.iter_mut().find(|b| b.idx == 299).unwrap().add_dpc(2_000, 0);
        let s: Vec<_> = (24..32).map(|t| second(t, 40.0)).collect();
        let d = table();
        let verdict = Verdict {
            causes: vec![CauseMatch {
                id: "drv.network".into(),
                confidence: 0.9,
                params: params! {"driver" => "ndis.sys"},
                evidence: vec![],
            }],
            ruled_out: vec![],
        };
        let window: Vec<_> = k.iter().filter(|b| b.idx >= 250 && b.idx < 315).copied().collect();
        let tl = build_timeline(TimelineInput {
            t0_us: 25_000_000,
            t1_us: 31_500_000,
            marker_us: 30_000_000,
            kernel: &window,
            seconds: &s,
            events: &[],
            frames: vec![],
            drivers: &d,
            verdict: &verdict,
            to_unix_ms: &|us| 1_700_000_000_000 + us / 1000,
        });
        assert_eq!(tl.dpc_max_us.len(), 65);
        assert_eq!(tl.marker_ts - tl.t0, 5000);
        let drv = tl.events.iter().find(|e| e.kind == TimelineEventKind::Driver).unwrap();
        assert!(drv.flagged);
    }
}
