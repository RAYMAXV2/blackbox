//! Detectors (hitch, driver latency, hang, sustained throttling/pressure), incident
//! finalization (analysis once the surrounding data is complete) and notification routing.

use crate::engine::{EngineSink, Notification, OverlayText};
use crate::recorder::{HangState, Pending, Recorder};
use blackbox_core::analyze::{analyze, build_timeline, AnalysisInput, DriverTable, TimelineInput};
use blackbox_core::frames::Hitch;
use blackbox_core::i18n;
use blackbox_core::kb;
use blackbox_core::model::*;
use blackbox_core::params;
use blackbox_core::series::*;
use std::sync::Arc;

/// Minimum spacing between two hitch incidents of the same severity; hitches in between are
/// counted, not reported. A worse one always gets through.
const HITCH_SPACING_US: Micros = 5_000_000;
const LATENCY_DEBOUNCE_US: Micros = 120_000_000;
const HANG_DEBOUNCE_US: Micros = 60_000_000;
const SUSTAINED_DEBOUNCE_US: Micros = 600_000_000;
const SUSTAINED_FOR_US: Micros = 10_000_000;
/// Timeline stored with a spike-like incident: 5 s before, 1.5 s after.
const TL_BEFORE_US: Micros = 5_000_000;
const TL_AFTER_US: Micros = 1_500_000;
/// Give up waiting for late kernel data after this long past `ready_at`.
const MAX_WAIT_US: Micros = 4_000_000;

/// Single DPC/ISR duration that makes a latency incident.
fn latency_threshold_us(s: Sensitivity) -> u32 {
    match s {
        Sensitivity::Low => 4000,
        Sensitivity::Normal => 2000,
        Sensitivity::High => 1000,
    }
}

fn floor_bucket(t: Micros) -> Micros {
    t.div_euclid(BUCKET_US) * BUCKET_US
}

fn ceil_bucket(t: Micros) -> Micros {
    floor_bucket(t + BUCKET_US - 1)
}

impl Recorder {
    fn debounced(&mut self, key: &str, at: Micros, window: Micros) -> bool {
        match self.last_by_key.get(key) {
            Some(&last) if at - last < window => true,
            _ => {
                self.last_by_key.insert(key.to_owned(), at);
                false
            }
        }
    }

    fn fg_app(&self) -> Option<(u32, Arc<str>)> {
        let pid = self.fg.pid();
        (pid != 0).then(|| self.names.get(&pid).map(|(n, _)| (pid, n.clone()))).flatten()
    }

    /// Spike-like incident around `marker`: timeline aligned on kernel buckets.
    fn spike_window(marker: Micros) -> (Micros, Micros) {
        (floor_bucket(marker - TL_BEFORE_US), ceil_bucket(marker + TL_AFTER_US))
    }

    pub(crate) fn on_hitch(&mut self, h: Hitch) {
        if h.severity == Severity::Minor && self.settings.sensitivity != Sensitivity::High {
            return; // counted in the session stats, not worth an incident
        }
        // The driver spike behind this long frame, if any, is reported by the hitch itself.
        self.pending
            .retain(|p| !(p.kind == IncidentKind::Latency && p.marker_us >= h.start_us - 1_000_000 && p.marker_us <= h.end_us + 1_000_000));

        let app = self.game.as_ref().map(|g| (g.pid, g.name.clone()));
        let (tl_from, tl_to) = Self::spike_window(h.end_us);
        let pending = Pending {
            kind: h.kind,
            severity: h.severity,
            marker_us: h.end_us,
            focus_from: h.start_us - 300_000,
            focus_to: h.end_us + 50_000,
            tl_from,
            tl_to,
            excess_ms: h.excess_ms(),
            duration_s: 0.0,
            magnitude: Magnitude { value: h.ms.round() as f64, unit: Unit::Ms },
            app,
            ready_at: tl_to + 500_000,
        };
        // Merge with a nearby hitch still waiting for data: keep the worst frame.
        if let Some(p) = self
            .pending
            .iter_mut()
            .find(|p| matches!(p.kind, IncidentKind::Hitch | IncidentKind::Freeze) && (h.end_us - p.marker_us).abs() < HITCH_SPACING_US)
        {
            if pending.magnitude.value > p.magnitude.value {
                *p = pending;
            }
            return;
        }
        if let Some((last, sev)) = self.last_hitch {
            if h.end_us - last < HITCH_SPACING_US && h.severity <= sev {
                return;
            }
        }
        self.last_hitch = Some((h.end_us, h.severity));
        self.pending.push(pending);
    }

    pub(crate) fn detect_latency(&mut self, fresh: &[KernelBucket]) {
        let threshold = latency_threshold_us(self.settings.sensitivity);
        for b in fresh {
            let (us, drv) = if b.dpc_max_us >= b.isr_max_us { (b.dpc_max_us, b.dpc_max_drv) } else { (b.isr_max_us, b.isr_max_drv) };
            if us < threshold {
                continue;
            }
            let marker = b.start_us() + BUCKET_US / 2;
            // A hitch at the same moment already carries this driver as evidence.
            let near_hitch = self.last_hitch.is_some_and(|(t, _)| (t - marker).abs() < 1_500_000)
                || self.pending.iter().any(|p| p.kind != IncidentKind::Latency && (p.marker_us - marker).abs() < 1_500_000);
            if near_hitch || self.debounced(&format!("lat:{drv}"), marker, LATENCY_DEBOUNCE_US) {
                continue;
            }
            let severity = if us >= 4000 {
                Severity::Severe
            } else if us >= 2000 {
                Severity::Major
            } else {
                Severity::Minor
            };
            let (tl_from, tl_to) = Self::spike_window(marker);
            self.pending.push(Pending {
                kind: IncidentKind::Latency,
                severity,
                marker_us: marker,
                focus_from: b.start_us(),
                focus_to: b.start_us() + BUCKET_US,
                tl_from,
                tl_to,
                excess_ms: 0.0,
                duration_s: 0.0,
                magnitude: Magnitude { value: us as f64, unit: Unit::Us },
                app: self.fg_app(),
                ready_at: tl_to + 500_000,
            });
        }
    }

    pub(crate) fn detect_hang(&mut self, now: Micros) {
        let pid = self.fg.pid();
        let hung = pid != 0 && pid != self.own_pid && self.fg.is_hung();
        // IsHungAppWindow turns true after ~5 s without message processing.
        let report = match (&mut self.hang, hung) {
            (None, true) => {
                self.hang = Some(HangState { pid, since: now - 5_000_000, reported: false });
                None
            }
            (Some(h), true) if h.pid == pid && !h.reported && now - h.since >= 20_000_000 => {
                h.reported = true;
                Some((h.pid, h.since))
            }
            (Some(h), _) if !hung || h.pid != pid => {
                let r = (!h.reported).then_some((h.pid, h.since));
                self.hang = None;
                r
            }
            _ => None,
        };
        if let Some((pid, since)) = report {
            if self.debounced(&format!("hang:{pid}"), now, HANG_DEBOUNCE_US) {
                return;
            }
            let secs = ((now - since) as f32 / 1e6).round();
            let name = self.names.get(&pid).map(|(n, _)| n.clone()).unwrap_or_else(|| Arc::from("app"));
            self.pending.push(Pending {
                kind: IncidentKind::Hang,
                severity: if secs >= 15.0 { Severity::Severe } else { Severity::Major },
                marker_us: since,
                focus_from: since - 2_000_000,
                focus_to: now,
                // The whole hang, from a few seconds before it started.
                tl_from: floor_bucket(since - 3_000_000),
                tl_to: ceil_bucket(now.min(since + 30_000_000)),
                excess_ms: 0.0,
                duration_s: secs,
                magnitude: Magnitude { value: secs as f64, unit: Unit::S },
                app: Some((pid, name)),
                ready_at: now + 1_500_000,
            });
        }
    }

    /// Throttling and memory/disk pressure that last at least 10 s.
    pub(crate) fn detect_sustained(&mut self, now: Micros) {
        let gpu_thermal = self.gpu.as_ref().is_some_and(|g| g.throttle.iter().any(|t| t.is_thermal()));
        let throttled = (self.freq_limit < 90.0 && self.cpu_total > 20.0) || gpu_thermal;
        if throttled {
            let since = *self.throttle_since.get_or_insert(now);
            if now - since >= SUSTAINED_FOR_US && !self.debounced("throttle", now, SUSTAINED_DEBOUNCE_US) {
                let secs = ((now - since) / 1_000_000) as f32;
                self.pending.push(Pending {
                    kind: IncidentKind::Throttle,
                    severity: Severity::Major,
                    marker_us: now,
                    focus_from: since,
                    focus_to: now,
                    tl_from: floor_bucket(since - 2_000_000),
                    tl_to: ceil_bucket(now),
                    excess_ms: 0.0,
                    duration_s: secs,
                    magnitude: Magnitude { value: secs as f64, unit: Unit::S },
                    app: self.game.as_ref().map(|g| (g.pid, g.name.clone())),
                    ready_at: now + 1_500_000,
                });
            }
        } else {
            self.throttle_since = None;
        }

        let (ios, lat_sum) = self
            .kring
            .range(now - 11_000_000, now - 1_000_000)
            .fold((0u64, 0u64), |(n, s), b| (n + b.disk_ios() as u64, s + b.disks.iter().map(|d| d.lat_sum_us).sum::<u64>()));
        let disk_lat_ms = if ios >= 50 { lat_sum as f32 / ios as f32 / 1000.0 } else { 0.0 };
        let pressured = self.mem.load_pct >= 95.0 || disk_lat_ms >= 100.0;
        if pressured {
            let since = *self.pressure_since.get_or_insert(now);
            if now - since >= SUSTAINED_FOR_US && !self.debounced("pressure", now, SUSTAINED_DEBOUNCE_US) {
                let magnitude = if self.mem.load_pct >= 95.0 {
                    Magnitude { value: self.mem.load_pct.round() as f64, unit: Unit::Pct }
                } else {
                    Magnitude { value: disk_lat_ms.round() as f64, unit: Unit::Ms }
                };
                self.pending.push(Pending {
                    kind: IncidentKind::Pressure,
                    severity: Severity::Major,
                    marker_us: now,
                    focus_from: since,
                    focus_to: now,
                    tl_from: floor_bucket(since - 2_000_000),
                    tl_to: ceil_bucket(now),
                    excess_ms: 0.0,
                    duration_s: ((now - since) / 1_000_000) as f32,
                    magnitude,
                    app: self.fg_app(),
                    ready_at: now + 1_500_000,
                });
            }
        } else {
            self.pressure_since = None;
        }
    }

    pub(crate) fn finalize_ready(&mut self, now: Micros, sink: &dyn EngineSink) {
        if self.pending.is_empty() {
            return;
        }
        let tracing = self.mode() == Mode::Full;
        let horizon = self.kring.last_idx().map_or(i64::MIN, |i| (i + 1) * BUCKET_US);
        let mut ready = Vec::new();
        self.pending.retain(|p| {
            let data_complete = !tracing || horizon >= p.tl_to;
            let go = now >= p.ready_at && (data_complete || now >= p.ready_at + MAX_WAIT_US);
            if go {
                ready.push(p.clone());
            }
            !go
        });
        for p in ready {
            let inc = self.finalize(p);
            self.recent.push_back((now, inc.severity));
            self.incidents_session += 1;
            log::info!("incident {} {:?} {:?}", inc.id, inc.kind, inc.causes.first().map(|c| &c.id));
            let summary = inc.summary();
            self.store.lock().unwrap_or_else(|e| e.into_inner()).add(inc);
            sink.incident(&summary);
            self.notify(&summary, sink);
        }
    }

    fn finalize(&mut self, p: Pending) -> Incident {
        let (t0, t1) = (p.tl_from, p.tl_to);
        let ctx_from = p.focus_from.min(t0) - 21_000_000;
        let ctx_to = t1.max(p.focus_to);
        let kernel = self.kring.window(ctx_from, ctx_to);
        let seconds: Vec<SecondSample> =
            self.seconds.iter().filter(|s| s.t_us > ctx_from && s.start_us() < ctx_to + 1_000_000).cloned().collect();
        let events: Vec<ProcEvent> = self.events.iter().filter(|e| e.t_us >= ctx_from && e.t_us <= ctx_to).cloned().collect();
        let drivers = DriverTable {
            entries: self
                .registry
                .snapshot()
                .into_iter()
                .map(|(name, _)| {
                    let cat = kb::driver_info(&name).1;
                    (name, cat)
                })
                .collect(),
        };
        let app_name: Option<Arc<str>> = p.app.as_ref().map(|(_, n)| n.clone());
        let verdict = analyze(&AnalysisInput {
            kind: p.kind,
            marker_us: p.marker_us,
            focus_from: p.focus_from,
            focus_to: p.focus_to,
            excess_ms: p.excess_ms,
            duration_s: p.duration_s,
            app: p.app.as_ref().map(|(pid, _)| (*pid, app_name.as_deref().unwrap_or(""))),
            kernel_tracing: self.mode() == Mode::Full,
            kernel: &kernel,
            seconds: &seconds,
            events: &events,
            drivers: &drivers,
        });
        let frames = match (&self.game, &p.app) {
            (Some(g), Some((pid, _))) if g.pid == *pid => g.tracker.frames_between(t0, t1),
            _ => Vec::new(),
        };
        let window = self.kring.window(t0, t1);
        let clock = self.clock;
        let timeline = build_timeline(TimelineInput {
            t0_us: t0,
            t1_us: t1,
            marker_us: p.marker_us,
            kernel: &window,
            seconds: &seconds,
            events: &events,
            frames,
            drivers: &drivers,
            verdict: &verdict,
            to_unix_ms: &|us| clock.us_to_unix_ms(us),
        });
        let ts = self.clock.us_to_unix_ms(p.marker_us);
        Incident {
            id: self.next_incident_id(ts),
            ts,
            kind: p.kind,
            severity: p.severity,
            magnitude: p.magnitude,
            app: p.app.map(|(pid, name)| AppRef { pid, name: name.to_string() }),
            causes: verdict.causes,
            ruled_out: verdict.ruled_out,
            read: false,
            timeline: Some(timeline),
        }
    }

    fn notify(&mut self, inc: &Incident, sink: &dyn EngineSink) {
        if inc.severity < self.settings.notify.min_severity {
            return;
        }
        // Unit ids are translated by the UI (`unit.*`).
        let unit = match inc.magnitude.unit {
            Unit::Ms => "ms",
            Unit::Us => "us",
            Unit::S => "s",
            Unit::Pct => "pct",
        };
        let app = inc.app.as_ref().map(|a| a.name.as_str()).unwrap_or("");
        let toast = Toast {
            id: format!("t-{}", inc.id),
            ts: inc.ts,
            severity: inc.severity,
            incident_id: Some(inc.id.clone()),
            title: Reason::new(
                "toast.incident",
                params! {"kind" => inc.kind.id(), "value" => inc.magnitude.value, "unit" => unit, "app" => app},
            ),
            cause: inc.causes.first().cloned(),
        };
        let (title, body) = i18n::incident_notification(&self.kb, inc, self.settings.lang);
        let text = OverlayText { title, body, severity: inc.severity };
        let in_game_fullscreen =
            self.game.as_ref().is_some_and(|g| g.fullscreen && g.away_since.is_none() && self.fg.pid() == g.pid);
        let overlay = match self.settings.notify.overlay {
            OverlayMode::Off => None,
            OverlayMode::Always => Some(text),
            OverlayMode::Smart if in_game_fullscreen => {
                if let Some(g) = self.game.as_mut() {
                    g.held.push(inc.clone());
                }
                None
            }
            OverlayMode::Smart => Some(text),
        };
        sink.notify(&Notification { toast, in_app: self.settings.notify.in_app, overlay });
    }
}
