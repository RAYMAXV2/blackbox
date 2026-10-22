//! Frame pacing for the foreground app and hitch detection.
//!
//! Works on present timestamps only. The "typical" frame time is a rolling median of the last
//! 64 normal frames, so a hitch is judged against what the game normally does, not a fixed FPS.
//!
//! A hitch is a *spike*: a long frame (or a short burst of them) between steady frames. It is
//! only confirmed once a few normal frames follow. A sustained slowdown (menu capped at 30 fps,
//! heavy scene) is a new normal, not a stream of hitches: after `LEVEL_SHIFT_FRAMES` long
//! frames in a row the median is reset to the new level.

use crate::model::{FramePoint, IncidentKind, Sensitivity, Severity};
use crate::series::Micros;
use std::collections::VecDeque;

/// Frames kept for statistics and incident timelines (~30 s at 240 fps).
const FRAME_RING: usize = 8192;
const MEDIAN_WINDOW: usize = 64;
/// Intervals longer than this are pauses (alt-tab, loading screen, minimized), not hitches.
const MAX_INTERVAL_MS: f32 = 3000.0;
/// No hitch is reported before this many frames, nor during the first seconds of a session.
const WARMUP_FRAMES: u32 = 90;
const WARMUP_US: Micros = 3_000_000;
/// Normal frames required after a long frame to call it a hitch.
const CONFIRM_FRAMES: u32 = 3;
/// Long frames in a row that mean "the game now runs slower".
const LEVEL_SHIFT_FRAMES: usize = 8;
/// Steady frames required before a long frame can start a hitch (filters bursty renderers).
const STEADY_FRAMES: u32 = 30;

#[derive(Debug, Clone, Copy)]
pub struct HitchRule {
    pub mult: f32,
    pub add_ms: f32,
    pub min_ms: f32,
}

impl HitchRule {
    pub fn for_sensitivity(s: Sensitivity) -> HitchRule {
        match s {
            Sensitivity::Low => HitchRule { mult: 3.0, add_ms: 40.0, min_ms: 50.0 },
            Sensitivity::Normal => HitchRule { mult: 2.5, add_ms: 20.0, min_ms: 25.0 },
            Sensitivity::High => HitchRule { mult: 2.0, add_ms: 12.0, min_ms: 18.0 },
        }
    }

    pub fn threshold_ms(&self, typical_ms: f32) -> f32 {
        (typical_ms * self.mult).max(typical_ms + self.add_ms).max(self.min_ms)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hitch {
    /// Present that ended the long frame.
    pub end_us: Micros,
    /// Previous present: the long frame started here.
    pub start_us: Micros,
    pub ms: f32,
    pub typical_ms: f32,
    pub kind: IncidentKind,
    pub severity: Severity,
}

impl Hitch {
    /// How much longer than usual the frame took.
    pub fn excess_ms(&self) -> f32 {
        (self.ms - self.typical_ms).max(1.0)
    }
}

#[derive(Debug, Clone, Default)]
pub struct FrameStats {
    pub fps: f32,
    pub frame_time_ms: f32,
    pub p99_ms: f32,
    pub low1_fps: f32,
    pub recent: Vec<f32>,
}

#[derive(Debug)]
pub struct FrameTracker {
    pub pid: u32,
    pub started_us: Micros,
    pub hitches: u32,
    last_us: Option<Micros>,
    frames_seen: u32,
    quiet_until_us: Micros,
    frames: VecDeque<(Micros, f32)>,
    window: [f32; MEDIAN_WINDOW],
    window_len: usize,
    window_pos: usize,
    typical_ms: f32,
    candidate: Option<Hitch>,
    streak: usize,
    streak_vals: [f32; LEVEL_SHIFT_FRAMES],
    normal_after: u32,
    steady: u32,
    scratch: Vec<f32>,
}

impl FrameTracker {
    pub fn new(pid: u32, now_us: Micros) -> FrameTracker {
        FrameTracker {
            pid,
            started_us: now_us,
            hitches: 0,
            last_us: None,
            frames_seen: 0,
            quiet_until_us: now_us + WARMUP_US,
            frames: VecDeque::with_capacity(FRAME_RING),
            window: [0.0; MEDIAN_WINDOW],
            window_len: 0,
            window_pos: 0,
            typical_ms: 0.0,
            candidate: None,
            streak: 0,
            streak_vals: [0.0; LEVEL_SHIFT_FRAMES],
            normal_after: 0,
            steady: 0,
            scratch: Vec::with_capacity(FRAME_RING),
        }
    }

    pub fn typical_ms(&self) -> f32 {
        self.typical_ms
    }

    /// Forget the current cadence (focus lost or regained): the next interval is not a frame,
    /// and nothing is judged during the following second.
    pub fn reset_cadence(&mut self, now_us: Micros) {
        self.last_us = None;
        self.candidate = None;
        self.streak = 0;
        self.normal_after = 0;
        self.steady = 0;
        self.quiet_until_us = self.quiet_until_us.max(now_us + 1_000_000);
    }

    /// Feeds one present. Returns a hitch once it is confirmed (a few presents later).
    pub fn push(&mut self, t_us: Micros, rule: &HitchRule) -> Option<Hitch> {
        let prev = self.last_us.replace(t_us)?;
        if t_us <= prev {
            self.last_us = Some(prev);
            return None; // duplicate or out-of-order present
        }
        let ms = (t_us - prev) as f32 / 1000.0;
        if ms > MAX_INTERVAL_MS {
            self.reset_cadence(t_us);
            self.last_us = Some(t_us);
            return None;
        }
        self.frames_seen += 1;
        if self.frames.len() == FRAME_RING {
            self.frames.pop_front();
        }
        self.frames.push_back((t_us, ms));

        let judging = self.frames_seen > WARMUP_FRAMES && t_us >= self.quiet_until_us && self.typical_ms > 0.0;
        if !judging {
            self.add_to_window(ms);
            return None;
        }

        if ms >= rule.threshold_ms(self.typical_ms) {
            self.normal_after = 0;
            if self.streak < LEVEL_SHIFT_FRAMES {
                self.streak_vals[self.streak] = ms;
            }
            self.streak += 1;
            let could_start = self.candidate.is_some() || self.steady >= STEADY_FRAMES;
            self.steady = 0;
            if could_start && self.candidate.is_none_or(|c| ms > c.ms) {
                self.candidate = Some(self.make_hitch(t_us, prev, ms));
            }
            if self.streak >= LEVEL_SHIFT_FRAMES {
                // The game now runs slower: that's the new normal, not a series of hitches.
                self.window_len = 0;
                self.window_pos = 0;
                let vals = self.streak_vals;
                for v in vals {
                    self.add_to_window(v);
                }
                self.candidate = None;
                self.streak = 0;
            }
            return None;
        }

        self.add_to_window(ms);
        self.streak = 0;
        self.steady = self.steady.saturating_add(1);
        if self.candidate.is_some() {
            self.normal_after += 1;
            if self.normal_after >= CONFIRM_FRAMES {
                self.normal_after = 0;
                self.hitches += 1;
                return self.candidate.take();
            }
        }
        None
    }

    fn make_hitch(&self, end_us: Micros, start_us: Micros, ms: f32) -> Hitch {
        let severity = if ms >= 250.0 {
            Severity::Severe
        } else if ms >= 80.0 || ms >= self.typical_ms * 5.0 {
            Severity::Major
        } else {
            Severity::Minor
        };
        let kind = if ms >= 500.0 { IncidentKind::Freeze } else { IncidentKind::Hitch };
        Hitch { end_us, start_us, ms, typical_ms: self.typical_ms, kind, severity }
    }

    fn add_to_window(&mut self, ms: f32) {
        self.window[self.window_pos] = ms;
        self.window_pos = (self.window_pos + 1) % MEDIAN_WINDOW;
        self.window_len = (self.window_len + 1).min(MEDIAN_WINDOW);
        if self.window_len <= 8 || self.window_pos % 8 == 0 {
            let mut tmp = self.window;
            let slice = &mut tmp[..self.window_len];
            let mid = slice.len() / 2;
            slice.select_nth_unstable_by(mid, f32::total_cmp);
            self.typical_ms = slice[mid];
        }
    }

    pub fn stats(&mut self, now_us: Micros) -> FrameStats {
        let one_s = now_us - 1_000_000;
        let ten_s = now_us - 10_000_000;
        let mut last_sec = (0u32, 0f32);
        self.scratch.clear();
        for &(t, ms) in self.frames.iter().rev() {
            if t < ten_s {
                break;
            }
            if t >= one_s {
                last_sec.0 += 1;
                last_sec.1 += ms;
            }
            self.scratch.push(ms);
        }
        let mut s = FrameStats::default();
        if last_sec.0 > 0 {
            s.frame_time_ms = last_sec.1 / last_sec.0 as f32;
            s.fps = 1000.0 / s.frame_time_ms;
        }
        if !self.scratch.is_empty() {
            self.scratch.sort_unstable_by(f32::total_cmp);
            let n = self.scratch.len();
            s.p99_ms = self.scratch[((n as f32 * 0.99) as usize).min(n - 1)];
            let worst = (n / 100).max(1);
            let avg_worst: f32 = self.scratch[n - worst..].iter().sum::<f32>() / worst as f32;
            s.low1_fps = if avg_worst > 0.0 { 1000.0 / avg_worst } else { 0.0 };
        }
        let take = self.frames.len().min(240);
        s.recent = self.frames.iter().skip(self.frames.len() - take).map(|&(_, ms)| ms).collect();
        s
    }

    /// Frames whose present falls in `[from, to)`, as offsets in ms from `from` (frame end).
    pub fn frames_between(&self, from: Micros, to: Micros) -> Vec<FramePoint> {
        self.frames
            .iter()
            .filter(|(t, _)| *t >= from && *t < to)
            .map(|&(t, ms)| FramePoint { t: (t - from) / 1000, ms })
            .collect()
    }

    pub fn last_present_us(&self) -> Option<Micros> {
        self.last_us
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(tr: &mut FrameTracker, t: &mut Micros, n: usize, ms: f32, rule: &HitchRule) -> Vec<Hitch> {
        let mut out = vec![];
        for _ in 0..n {
            *t += (ms * 1000.0) as Micros;
            if let Some(h) = tr.push(*t, rule) {
                out.push(h);
            }
        }
        out
    }

    fn rule() -> HitchRule {
        HitchRule::for_sensitivity(Sensitivity::Normal)
    }

    #[test]
    fn steady_frames_produce_no_hitch() {
        let mut tr = FrameTracker::new(1, 0);
        let mut t = 0;
        assert!(feed(&mut tr, &mut t, 2000, 6.9, &rule()).is_empty());
        assert!((tr.typical_ms() - 6.9).abs() < 0.1);
    }

    #[test]
    fn spike_is_confirmed_by_the_frames_after_it() {
        let mut tr = FrameTracker::new(1, 0);
        let mut t = 0;
        feed(&mut tr, &mut t, 1000, 6.9, &rule());
        assert!(feed(&mut tr, &mut t, 1, 186.0, &rule()).is_empty(), "not confirmed yet");
        let h = feed(&mut tr, &mut t, 3, 6.9, &rule());
        assert_eq!(h.len(), 1);
        assert_eq!((h[0].kind, h[0].severity), (IncidentKind::Hitch, Severity::Major));
        assert!((h[0].excess_ms() - 179.1).abs() < 0.5);
        assert_eq!(tr.hitches, 1);

        feed(&mut tr, &mut t, 100, 6.9, &rule());
        feed(&mut tr, &mut t, 1, 700.0, &rule());
        let h = feed(&mut tr, &mut t, 3, 6.9, &rule());
        assert_eq!((h[0].kind, h[0].severity), (IncidentKind::Freeze, Severity::Severe));
        assert!((tr.typical_ms() - 6.9).abs() < 0.1, "the median is not polluted by hitches");
    }

    #[test]
    fn short_burst_reports_its_worst_frame_once() {
        let mut tr = FrameTracker::new(1, 0);
        let mut t = 0;
        feed(&mut tr, &mut t, 1000, 6.9, &rule());
        feed(&mut tr, &mut t, 1, 60.0, &rule());
        feed(&mut tr, &mut t, 1, 6.9, &rule());
        feed(&mut tr, &mut t, 1, 140.0, &rule());
        let h = feed(&mut tr, &mut t, 5, 6.9, &rule());
        assert_eq!(h.len(), 1);
        assert!((h[0].ms - 140.0).abs() < 0.1);
    }

    #[test]
    fn sustained_slowdown_becomes_the_new_normal() {
        let mut tr = FrameTracker::new(1, 0);
        let mut t = 0;
        feed(&mut tr, &mut t, 1000, 4.17, &rule()); // 240 fps
        let h = feed(&mut tr, &mut t, 600, 33.3, &rule()); // 30 fps menu for 20 s
        assert!(h.is_empty(), "{h:?}");
        assert_eq!(tr.hitches, 0);
        assert!((tr.typical_ms() - 33.3).abs() < 0.5);
    }

    #[test]
    fn no_hitch_during_warmup_after_refocus_or_for_pauses() {
        let mut tr = FrameTracker::new(1, 0);
        let mut t = 0;
        feed(&mut tr, &mut t, 20, 16.6, &rule());
        feed(&mut tr, &mut t, 1, 200.0, &rule());
        assert!(feed(&mut tr, &mut t, 3, 16.6, &rule()).is_empty(), "warmup");
        feed(&mut tr, &mut t, 400, 16.6, &rule());
        feed(&mut tr, &mut t, 1, 5000.0, &rule());
        assert!(feed(&mut tr, &mut t, 3, 16.6, &rule()).is_empty(), "pause");
        // Focus left for 1.5 s, then came back.
        tr.reset_cadence(t);
        t += 1_500_000;
        feed(&mut tr, &mut t, 1, 16.6, &rule());
        assert!(feed(&mut tr, &mut t, 200, 16.6, &rule()).is_empty(), "refocus");
    }

    #[test]
    fn bursty_renderer_does_not_hitch() {
        let mut tr = FrameTracker::new(1, 0);
        let mut t = 0;
        feed(&mut tr, &mut t, 200, 16.6, &rule());
        // bursts of 10 frames separated by 400 ms pauses (editor/terminal style)
        let mut hitches = 0;
        for _ in 0..30 {
            hitches += feed(&mut tr, &mut t, 1, 400.0, &rule()).len();
            hitches += feed(&mut tr, &mut t, 10, 16.6, &rule()).len();
        }
        assert!(hitches <= 1, "{hitches}");
    }

    #[test]
    fn threshold_scales_with_frame_rate() {
        let r = rule();
        assert!((r.threshold_ms(16.67) - 41.7).abs() < 0.1);
        assert!((r.threshold_ms(4.17) - 25.0).abs() < 0.1);
    }

    #[test]
    fn stats_compute_fps_and_lows() {
        let mut tr = FrameTracker::new(1, 0);
        let mut t = 0;
        feed(&mut tr, &mut t, 990, 10.0, &rule());
        feed(&mut tr, &mut t, 10, 30.0, &rule());
        let s = tr.stats(t);
        assert!(s.fps > 50.0 && s.fps < 100.0);
        assert!((s.p99_ms - 30.0).abs() < 0.01);
        assert!((s.low1_fps - 33.3).abs() < 0.5);
        assert_eq!(s.recent.len(), 240);
    }
}
