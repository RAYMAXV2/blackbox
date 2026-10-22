//! Per-game play-session statistics.
//!
//! Everything is accumulated incrementally with fixed memory: a frame-time histogram (for
//! percentiles and 1 % / 0.1 % lows over hours of play) and running sums / extremes of the
//! once-per-second machine readings. Nothing grows with the session length.

use crate::model::{CauseCount, IncidentKind, SessionStats, Severity};
use crate::series::{GpuSample, Micros};

const FINE_BINS: usize = 500; // 0 – 50 ms by 0.1 ms
const FINE_STEP: f32 = 0.1;
const COARSE_BINS: usize = 190; // 50 – 1000 ms by 5 ms
const COARSE_STEP: f32 = 5.0;
const FINE_MAX: f32 = FINE_BINS as f32 * FINE_STEP;
const BINS: usize = FINE_BINS + COARSE_BINS + 1; // + overflow (>= 1 s)

/// Frame-time histogram: 0.1 ms resolution up to 50 ms, 5 ms up to 1 s.
#[derive(Debug, Clone)]
pub struct FrameHist {
    bins: Box<[u32; BINS]>,
    count: u64,
    sum_ms: f64,
}

impl Default for FrameHist {
    fn default() -> Self {
        FrameHist { bins: Box::new([0; BINS]), count: 0, sum_ms: 0.0 }
    }
}

impl FrameHist {
    fn bin(ms: f32) -> usize {
        if ms < FINE_MAX {
            ((ms.max(0.0) / FINE_STEP) as usize).min(FINE_BINS - 1)
        } else {
            (FINE_BINS + ((ms - FINE_MAX) / COARSE_STEP) as usize).min(BINS - 1)
        }
    }

    fn mid(i: usize) -> f32 {
        if i < FINE_BINS {
            (i as f32 + 0.5) * FINE_STEP
        } else if i < BINS - 1 {
            FINE_MAX + ((i - FINE_BINS) as f32 + 0.5) * COARSE_STEP
        } else {
            1000.0
        }
    }

    pub fn add(&mut self, ms: f32) {
        self.bins[Self::bin(ms)] += 1;
        self.count += 1;
        self.sum_ms += ms as f64;
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    pub fn mean_ms(&self) -> f32 {
        if self.count == 0 { 0.0 } else { (self.sum_ms / self.count as f64) as f32 }
    }

    /// Frame time at percentile `p` (0..1).
    pub fn percentile(&self, p: f32) -> f32 {
        if self.count == 0 {
            return 0.0;
        }
        let target = ((self.count as f64) * p as f64).ceil().max(1.0) as u64;
        let mut seen = 0u64;
        for (i, &n) in self.bins.iter().enumerate() {
            seen += n as u64;
            if seen >= target {
                return Self::mid(i);
            }
        }
        Self::mid(BINS - 1)
    }

    /// Average frame time of the slowest `frac` of frames (0.01 for the 1 % low).
    pub fn worst_mean_ms(&self, frac: f32) -> f32 {
        if self.count == 0 {
            return 0.0;
        }
        let want = ((self.count as f64 * frac as f64).ceil() as u64).max(1);
        let (mut taken, mut sum) = (0u64, 0f64);
        for i in (0..BINS).rev() {
            let n = self.bins[i] as u64;
            if n == 0 {
                continue;
            }
            let take = n.min(want - taken);
            sum += take as f64 * Self::mid(i) as f64;
            taken += take;
            if taken >= want {
                break;
            }
        }
        (sum / taken as f64) as f32
    }
}

/// What the engine knows about the machine and the game over the last second.
#[derive(Debug, Clone, Default)]
pub struct TickSample<'a> {
    pub cpu_pct: f32,
    pub game_cpu_pct: f32,
    pub game_ram_mb: f32,
    pub freq_limit_pct: f32,
    pub ram_used_mb: f32,
    pub gpu: Option<&'a GpuSample>,
    pub dpc_max_us: u32,
    pub isr_max_us: u32,
    pub disk_lat_max_ms: f32,
    pub fullscreen: bool,
    /// Average FPS over the last second of frames, when the game rendered.
    pub fps: Option<f32>,
}

#[derive(Debug, Clone, Copy, Default)]
struct Stat {
    sum: f64,
    n: u32,
    max: f32,
}

impl Stat {
    fn add(&mut self, v: f32) {
        self.sum += v as f64;
        self.n += 1;
        self.max = if self.n == 1 { v } else { self.max.max(v) };
    }
    fn avg(&self) -> Option<f32> {
        (self.n > 0).then(|| (self.sum / self.n as f64) as f32)
    }
}

#[derive(Debug, Clone)]
pub struct SessionAccum {
    pub game: String,
    pub start_ts: i64,
    pub full_tracing: bool,
    active_us: Micros,
    seconds: u32,
    fps_min: f32,
    fps_max: f32,
    hitches: [u32; 3],
    freezes: u32,
    incidents: u32,
    causes: Vec<CauseCount>,
    cpu: Stat,
    game_cpu: Stat,
    freq_min: f32,
    ram: Stat,
    game_ram_max: f32,
    gpu_util: Stat,
    gpu_temp: Stat,
    gpu_power: Stat,
    gpu_clock: Stat,
    gpu_fan: Stat,
    vram: Stat,
    gpu_seconds: u32,
    gpu_throttled: u32,
    dpc_max: u32,
    isr_max: u32,
    disk_lat_max: f32,
    fullscreen_s: u32,
}

impl SessionAccum {
    pub fn new(game: &str, start_ts: i64, full_tracing: bool) -> SessionAccum {
        SessionAccum {
            game: game.to_owned(),
            start_ts,
            full_tracing,
            active_us: 0,
            seconds: 0,
            fps_min: f32::INFINITY,
            fps_max: 0.0,
            hitches: [0; 3],
            freezes: 0,
            incidents: 0,
            causes: Vec::new(),
            cpu: Stat::default(),
            game_cpu: Stat::default(),
            freq_min: 100.0,
            ram: Stat::default(),
            game_ram_max: 0.0,
            gpu_util: Stat::default(),
            gpu_temp: Stat::default(),
            gpu_power: Stat::default(),
            gpu_clock: Stat::default(),
            gpu_fan: Stat::default(),
            vram: Stat::default(),
            gpu_seconds: 0,
            gpu_throttled: 0,
            dpc_max: 0,
            isr_max: 0,
            disk_lat_max: 0.0,
            fullscreen_s: 0,
        }
    }

    /// Time actually spent in the game (focused).
    pub fn add_active(&mut self, us: Micros) {
        self.active_us += us.max(0);
    }

    pub fn active_sec(&self) -> u64 {
        (self.active_us / 1_000_000) as u64
    }

    pub fn add_second(&mut self, s: &TickSample) {
        self.seconds += 1;
        if let Some(fps) = s.fps.filter(|f| *f > 0.0) {
            self.fps_min = self.fps_min.min(fps);
            self.fps_max = self.fps_max.max(fps);
        }
        self.cpu.add(s.cpu_pct);
        self.game_cpu.add(s.game_cpu_pct);
        self.freq_min = self.freq_min.min(s.freq_limit_pct);
        self.ram.add(s.ram_used_mb);
        self.game_ram_max = self.game_ram_max.max(s.game_ram_mb);
        if let Some(g) = s.gpu {
            self.gpu_seconds += 1;
            self.gpu_util.add(g.util_pct);
            if let Some(t) = g.temp_c {
                self.gpu_temp.add(t);
            }
            if let Some(p) = g.power_w {
                self.gpu_power.add(p);
            }
            if let Some(c) = g.core_mhz {
                self.gpu_clock.add(c as f32);
            }
            if let Some(f) = g.fan_pct {
                self.gpu_fan.add(f);
            }
            self.vram.add(g.vram_used_mb as f32);
            if !g.throttle.is_empty() {
                self.gpu_throttled += 1;
            }
        }
        self.dpc_max = self.dpc_max.max(s.dpc_max_us);
        self.isr_max = self.isr_max.max(s.isr_max_us);
        self.disk_lat_max = self.disk_lat_max.max(s.disk_lat_max_ms);
        if s.fullscreen {
            self.fullscreen_s += 1;
        }
    }

    pub fn add_hitch(&mut self, kind: IncidentKind, severity: Severity) {
        if kind == IncidentKind::Freeze {
            self.freezes += 1;
        }
        let i = match severity {
            Severity::Minor => 0,
            Severity::Major => 1,
            Severity::Severe => 2,
        };
        self.hitches[i] += 1;
    }

    pub fn add_incident(&mut self, cause_id: Option<&str>) {
        self.incidents += 1;
        if let Some(id) = cause_id {
            match self.causes.iter_mut().find(|c| c.id == id) {
                Some(c) => c.count += 1,
                None => self.causes.push(CauseCount { id: id.to_owned(), count: 1 }),
            }
        }
    }

    /// Statistics so far (`end_ts` = now for a live session).
    pub fn stats(&self, id: &str, end_ts: i64, frames: &FrameHist) -> SessionStats {
        let hours = (self.active_us as f64 / 3.6e9).max(1.0 / 3600.0);
        let hitches: u32 = self.hitches.iter().sum();
        let frame_ms = frames.mean_ms();
        let low = |frac: f32| {
            let ms = frames.worst_mean_ms(frac);
            if ms > 0.0 { 1000.0 / ms } else { 0.0 }
        };
        let mut causes = self.causes.clone();
        causes.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.id.cmp(&b.id)));
        causes.truncate(5);
        let gpu = self.gpu_seconds > 0;
        let opt = |s: &Stat| if gpu { s.avg() } else { None };
        let opt_max = |s: &Stat| if gpu && s.n > 0 { Some(s.max) } else { None };
        SessionStats {
            id: id.to_owned(),
            game: self.game.clone(),
            sessions: 1,
            start_ts: self.start_ts,
            end_ts,
            duration_sec: self.active_sec(),
            full_tracing: self.full_tracing,
            frames: frames.count(),
            fps_avg: if frame_ms > 0.0 { 1000.0 / frame_ms } else { 0.0 },
            fps_min1s: if self.fps_min.is_finite() { self.fps_min } else { 0.0 },
            fps_max1s: self.fps_max,
            frame_time_avg_ms: frame_ms,
            frame_time_p50_ms: frames.percentile(0.50),
            frame_time_p95_ms: frames.percentile(0.95),
            frame_time_p99_ms: frames.percentile(0.99),
            low1_fps: low(0.01),
            low01_fps: low(0.001),
            hitches,
            hitches_minor: self.hitches[0],
            hitches_major: self.hitches[1],
            hitches_severe: self.hitches[2],
            freezes: self.freezes,
            hitches_per_hour: (hitches as f64 / hours) as f32,
            incidents: self.incidents,
            top_causes: causes,
            cpu_avg_pct: self.cpu.avg().unwrap_or(0.0),
            cpu_max_pct: self.cpu.max,
            game_cpu_avg_pct: self.game_cpu.avg().unwrap_or(0.0),
            cpu_freq_limit_min_pct: self.freq_min,
            ram_used_avg_mb: self.ram.avg().unwrap_or(0.0),
            ram_used_max_mb: self.ram.max,
            game_ram_max_mb: self.game_ram_max,
            gpu_util_avg_pct: opt(&self.gpu_util),
            gpu_util_max_pct: opt_max(&self.gpu_util),
            gpu_temp_avg_c: opt(&self.gpu_temp),
            gpu_temp_max_c: opt_max(&self.gpu_temp),
            gpu_power_avg_w: opt(&self.gpu_power),
            gpu_power_max_w: opt_max(&self.gpu_power),
            gpu_clock_avg_mhz: opt(&self.gpu_clock),
            gpu_fan_avg_pct: opt(&self.gpu_fan),
            vram_avg_mb: opt(&self.vram),
            vram_max_mb: opt_max(&self.vram),
            gpu_throttle_pct: gpu.then(|| self.gpu_throttled as f32 * 100.0 / self.gpu_seconds as f32),
            dpc_max_us: self.dpc_max,
            isr_max_us: self.isr_max,
            disk_lat_max_ms: self.disk_lat_max,
            fullscreen_pct: if self.seconds > 0 { self.fullscreen_s as f32 * 100.0 / self.seconds as f32 } else { 0.0 },
        }
        .rounded()
    }
}

fn r1(v: f32) -> f32 {
    (v * 10.0).round() / 10.0
}

fn r2(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}

impl SessionStats {
    /// Display precision: 1 decimal for rates and temperatures, 2 for frame times.
    pub fn rounded(mut self) -> SessionStats {
        for v in [
            &mut self.fps_avg,
            &mut self.fps_min1s,
            &mut self.fps_max1s,
            &mut self.low1_fps,
            &mut self.low01_fps,
            &mut self.hitches_per_hour,
            &mut self.cpu_avg_pct,
            &mut self.cpu_max_pct,
            &mut self.game_cpu_avg_pct,
            &mut self.cpu_freq_limit_min_pct,
            &mut self.disk_lat_max_ms,
            &mut self.fullscreen_pct,
        ] {
            *v = r1(*v);
        }
        for v in [&mut self.frame_time_avg_ms, &mut self.frame_time_p50_ms, &mut self.frame_time_p95_ms, &mut self.frame_time_p99_ms] {
            *v = r2(*v);
        }
        self.ram_used_avg_mb = self.ram_used_avg_mb.round();
        self.ram_used_max_mb = self.ram_used_max_mb.round();
        self.game_ram_max_mb = self.game_ram_max_mb.round();
        for v in [
            &mut self.gpu_util_avg_pct,
            &mut self.gpu_util_max_pct,
            &mut self.gpu_temp_avg_c,
            &mut self.gpu_temp_max_c,
            &mut self.gpu_power_avg_w,
            &mut self.gpu_power_max_w,
            &mut self.gpu_fan_avg_pct,
            &mut self.gpu_throttle_pct,
        ]
        .into_iter()
        .flatten()
        {
            *v = r1(*v);
        }
        for v in [&mut self.gpu_clock_avg_mhz, &mut self.vram_avg_mb, &mut self.vram_max_mb].into_iter().flatten() {
            *v = v.round();
        }
        self
    }
}

/// Duration-weighted average of `sessions` (newest first). Maxima stay maxima, minima stay
/// minima, counts are summed and rates recomputed from the sums.
pub fn average(sessions: &[SessionStats]) -> Option<SessionStats> {
    let first = sessions.first()?;
    let total_w: f64 = sessions.iter().map(|s| s.duration_sec.max(1) as f64).sum();
    let wavg = |f: &dyn Fn(&SessionStats) -> f32| -> f32 {
        (sessions.iter().map(|s| f(s) as f64 * s.duration_sec.max(1) as f64).sum::<f64>() / total_w) as f32
    };
    let wavg_opt = |f: &dyn Fn(&SessionStats) -> Option<f32>| -> Option<f32> {
        let (mut sum, mut w) = (0f64, 0f64);
        for s in sessions {
            if let Some(v) = f(s) {
                let sw = s.duration_sec.max(1) as f64;
                sum += v as f64 * sw;
                w += sw;
            }
        }
        (w > 0.0).then(|| (sum / w) as f32)
    };
    let max_opt = |f: &dyn Fn(&SessionStats) -> Option<f32>| sessions.iter().filter_map(f).fold(None, |a: Option<f32>, v| Some(a.map_or(v, |a| a.max(v))));
    let maxf = |f: &dyn Fn(&SessionStats) -> f32| sessions.iter().map(f).fold(0.0, f32::max);
    let minf = |f: &dyn Fn(&SessionStats) -> f32| sessions.iter().map(f).filter(|v| *v > 0.0).fold(f32::INFINITY, f32::min);
    let sum = |f: &dyn Fn(&SessionStats) -> u32| sessions.iter().map(f).sum::<u32>();

    let duration: u64 = sessions.iter().map(|s| s.duration_sec).sum();
    let hitches = sum(&|s| s.hitches);
    let mut causes: Vec<CauseCount> = Vec::new();
    for c in sessions.iter().flat_map(|s| s.top_causes.iter()) {
        match causes.iter_mut().find(|x| x.id == c.id) {
            Some(x) => x.count += c.count,
            None => causes.push(c.clone()),
        }
    }
    causes.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.id.cmp(&b.id)));
    causes.truncate(5);
    let fps_min = minf(&|s| s.fps_min1s);

    Some(
        SessionStats {
            id: format!("avg{}", sessions.len()),
            game: first.game.clone(),
            sessions: sessions.len() as u32,
            start_ts: sessions.iter().map(|s| s.start_ts).min().unwrap_or(0),
            end_ts: sessions.iter().map(|s| s.end_ts).max().unwrap_or(0),
            duration_sec: duration,
            full_tracing: sessions.iter().all(|s| s.full_tracing),
            frames: sessions.iter().map(|s| s.frames).sum(),
            fps_avg: wavg(&|s| s.fps_avg),
            fps_min1s: if fps_min.is_finite() { fps_min } else { 0.0 },
            fps_max1s: maxf(&|s| s.fps_max1s),
            frame_time_avg_ms: wavg(&|s| s.frame_time_avg_ms),
            frame_time_p50_ms: wavg(&|s| s.frame_time_p50_ms),
            frame_time_p95_ms: wavg(&|s| s.frame_time_p95_ms),
            frame_time_p99_ms: wavg(&|s| s.frame_time_p99_ms),
            low1_fps: wavg(&|s| s.low1_fps),
            low01_fps: wavg(&|s| s.low01_fps),
            hitches,
            hitches_minor: sum(&|s| s.hitches_minor),
            hitches_major: sum(&|s| s.hitches_major),
            hitches_severe: sum(&|s| s.hitches_severe),
            freezes: sum(&|s| s.freezes),
            hitches_per_hour: (hitches as f64 / (duration as f64 / 3600.0).max(1.0 / 3600.0)) as f32,
            incidents: sum(&|s| s.incidents),
            top_causes: causes,
            cpu_avg_pct: wavg(&|s| s.cpu_avg_pct),
            cpu_max_pct: maxf(&|s| s.cpu_max_pct),
            game_cpu_avg_pct: wavg(&|s| s.game_cpu_avg_pct),
            cpu_freq_limit_min_pct: sessions.iter().map(|s| s.cpu_freq_limit_min_pct).fold(100.0, f32::min),
            ram_used_avg_mb: wavg(&|s| s.ram_used_avg_mb),
            ram_used_max_mb: maxf(&|s| s.ram_used_max_mb),
            game_ram_max_mb: maxf(&|s| s.game_ram_max_mb),
            gpu_util_avg_pct: wavg_opt(&|s| s.gpu_util_avg_pct),
            gpu_util_max_pct: max_opt(&|s| s.gpu_util_max_pct),
            gpu_temp_avg_c: wavg_opt(&|s| s.gpu_temp_avg_c),
            gpu_temp_max_c: max_opt(&|s| s.gpu_temp_max_c),
            gpu_power_avg_w: wavg_opt(&|s| s.gpu_power_avg_w),
            gpu_power_max_w: max_opt(&|s| s.gpu_power_max_w),
            gpu_clock_avg_mhz: wavg_opt(&|s| s.gpu_clock_avg_mhz),
            gpu_fan_avg_pct: wavg_opt(&|s| s.gpu_fan_avg_pct),
            vram_avg_mb: wavg_opt(&|s| s.vram_avg_mb),
            vram_max_mb: max_opt(&|s| s.vram_max_mb),
            gpu_throttle_pct: wavg_opt(&|s| s.gpu_throttle_pct),
            dpc_max_us: sessions.iter().map(|s| s.dpc_max_us).max().unwrap_or(0),
            isr_max_us: sessions.iter().map(|s| s.isr_max_us).max().unwrap_or(0),
            disk_lat_max_ms: maxf(&|s| s.disk_lat_max_ms),
            fullscreen_pct: wavg(&|s| s.fullscreen_pct),
        }
        .rounded(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_percentiles_and_lows() {
        let mut h = FrameHist::default();
        for _ in 0..990 {
            h.add(4.0);
        }
        for _ in 0..10 {
            h.add(40.0);
        }
        assert_eq!(h.count(), 1000);
        assert!((h.percentile(0.5) - 4.05).abs() < 0.06);
        assert!((h.percentile(0.995) - 40.05).abs() < 0.06);
        // 1 % low = average of the 10 slowest frames = 40 ms -> 25 fps
        assert!((1000.0 / h.worst_mean_ms(0.01) - 25.0).abs() < 0.1);
        assert!((h.mean_ms() - 4.36).abs() < 0.01);
        h.add(5000.0); // overflow bin, still counted
        assert_eq!(h.count(), 1001);
    }

    fn gpu(temp: f32, throttled: bool) -> GpuSample {
        GpuSample {
            util_pct: 90.0,
            temp_c: Some(temp),
            power_w: Some(400.0),
            core_mhz: Some(2700),
            vram_used_mb: 9000,
            vram_total_mb: 32768,
            throttle: if throttled { vec![crate::model::GpuThrottle::SwPowerCap] } else { vec![] },
            ..Default::default()
        }
    }

    #[test]
    fn session_stats_accumulate() {
        let mut a = SessionAccum::new("Game.exe", 1000, true);
        let (g1, g2) = (gpu(70.0, false), gpu(80.0, true));
        for i in 0..120 {
            a.add_active(1_000_000);
            a.add_second(&TickSample {
                cpu_pct: 30.0,
                game_cpu_pct: 12.0,
                game_ram_mb: 4000.0 + i as f32,
                freq_limit_pct: if i == 50 { 85.0 } else { 100.0 },
                ram_used_mb: 16000.0,
                gpu: Some(if i % 2 == 0 { &g1 } else { &g2 }),
                dpc_max_us: 120,
                fps: Some(if i == 10 { 90.0 } else { 240.0 }),
                fullscreen: true,
                ..Default::default()
            });
        }
        a.add_hitch(IncidentKind::Hitch, Severity::Major);
        a.add_hitch(IncidentKind::Freeze, Severity::Severe);
        a.add_incident(Some("game.internal"));
        a.add_incident(Some("game.internal"));
        a.add_incident(Some("drv.gpu"));
        let mut h = FrameHist::default();
        for _ in 0..1000 {
            h.add(4.17);
        }
        let s = a.stats("s1", 200_000, &h);
        assert_eq!(s.duration_sec, 120);
        assert!((s.fps_avg - 240.0).abs() < 2.0);
        assert_eq!((s.fps_min1s, s.fps_max1s), (90.0, 240.0));
        assert_eq!((s.hitches, s.freezes, s.hitches_per_hour), (2, 1, 60.0));
        assert_eq!(s.top_causes[0], CauseCount { id: "game.internal".into(), count: 2 });
        assert_eq!(s.gpu_temp_avg_c, Some(75.0));
        assert_eq!(s.gpu_temp_max_c, Some(80.0));
        assert_eq!(s.gpu_throttle_pct, Some(50.0));
        assert_eq!(s.cpu_freq_limit_min_pct, 85.0);
        assert_eq!(s.game_ram_max_mb, 4119.0);
        assert_eq!(s.fullscreen_pct, 100.0);
    }

    #[test]
    fn no_gpu_means_null_gpu_fields() {
        let mut a = SessionAccum::new("Game.exe", 0, false);
        a.add_second(&TickSample { cpu_pct: 10.0, ..Default::default() });
        let s = a.stats("s", 1, &FrameHist::default());
        assert_eq!((s.gpu_util_avg_pct, s.gpu_temp_max_c, s.gpu_throttle_pct), (None, None, None));
        assert_eq!(s.fps_avg, 0.0);
    }

    #[test]
    fn average_is_duration_weighted() {
        let mk = |fps: f32, dur: u64, hitches: u32, tmax: f32| SessionStats {
            game: "G.exe".into(),
            sessions: 1,
            duration_sec: dur,
            fps_avg: fps,
            fps_min1s: fps / 2.0,
            fps_max1s: fps * 1.2,
            hitches,
            gpu_temp_max_c: Some(tmax),
            gpu_temp_avg_c: Some(tmax - 10.0),
            cpu_freq_limit_min_pct: 100.0,
            full_tracing: true,
            ..Default::default()
        };
        let a = average(&[mk(200.0, 3600, 10, 80.0), mk(100.0, 1800, 20, 70.0)]).unwrap();
        assert_eq!(a.sessions, 2);
        assert!((a.fps_avg - 166.7).abs() < 0.1, "{}", a.fps_avg);
        assert_eq!(a.hitches, 30);
        assert!((a.hitches_per_hour - 20.0).abs() < 0.01);
        assert_eq!(a.gpu_temp_max_c, Some(80.0));
        assert_eq!(a.fps_min1s, 50.0);
        assert_eq!(a.duration_sec, 5400);
        assert!(average(&[]).is_none());
    }
}
