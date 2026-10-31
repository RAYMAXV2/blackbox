//! One play session of a game: labels every second (gameplay / menu / loading / away) once its
//! frames have arrived, and keeps two sets of statistics: gameplay only, and everything.

use blackbox_core::activity::{Phase, PhaseClassifier, PhaseClock, SecondSignals};
use blackbox_core::frames::{FrameTracker, Hitch};
use blackbox_core::model::SessionStats;
use blackbox_core::series::Micros;
use blackbox_core::session::{SessionAccum, TickSample};
use blackbox_sys::input::InputSample;
use std::collections::VecDeque;

const SECOND: Micros = 1_000_000;
/// A second is labelled once frames up to its end arrived, or after this long anyway.
const MAX_WAIT_US: Micros = 4_000_000;
/// Labels kept to place incidents (they are analysed a couple of seconds late).
const RING_LEN: usize = 120;

/// Everything sampled at a tick, waiting for the frames of that second.
#[derive(Debug, Clone)]
pub(crate) struct PendingSecond {
    /// End of the second.
    pub t: Micros,
    pub focused: bool,
    pub input: InputSample,
    pub game_io_mbps: f32,
    pub tick: TickSample,
}

#[derive(Debug)]
pub(crate) struct PlaySession {
    name: String,
    classifier: PhaseClassifier,
    clock: PhaseClock,
    gameplay: SessionAccum,
    all: SessionAccum,
    pending: VecDeque<PendingSecond>,
    hitches: Vec<Hitch>,
    ring: VecDeque<(Micros, Phase)>,
}

impl PlaySession {
    pub fn new(name: &str, start_ts: i64, full_tracing: bool, cursor_hint: Option<bool>) -> PlaySession {
        PlaySession {
            name: name.to_owned(),
            classifier: PhaseClassifier::new(cursor_hint),
            clock: PhaseClock::default(),
            gameplay: SessionAccum::new(name, start_ts, full_tracing),
            all: SessionAccum::new(name, start_ts, full_tracing),
            pending: VecDeque::with_capacity(8),
            hitches: Vec::new(),
            ring: VecDeque::with_capacity(RING_LEN),
        }
    }

    pub fn push_second(&mut self, s: PendingSecond) {
        self.pending.push_back(s);
    }

    pub fn push_hitch(&mut self, h: Hitch) {
        self.hitches.push(h);
    }

    /// Labels the seconds whose frames are in, and accumulates them.
    pub fn classify(&mut self, now: Micros, tracker: &FrameTracker) {
        let horizon = tracker.last_present_us().unwrap_or(Micros::MIN);
        while let Some(front) = self.pending.front() {
            if front.focused && front.t > horizon && now - front.t < MAX_WAIT_US {
                break;
            }
            let Some(mut p) = self.pending.pop_front() else {
                break;
            };
            let from = p.t - SECOND;
            let frames = tracker.intervals(from, p.t).count() as u32;
            let signals = SecondSignals {
                focused: p.focused,
                frames,
                fps: frames as f32,
                cursor_visible: p.input.cursor_visible,
                cursor_confined: p.input.cursor_confined,
                idle_ms: p.input.idle_ms,
                pad_active: p.input.pad_active,
                game_io_mbps: p.game_io_mbps,
            };
            let before = self.classifier.last();
            let (phase, why) = self.classifier.classify(&signals);
            if phase != before {
                log::info!(
                    "{}: {before:?} -> {phase:?} ({why:?}; {frames} fps, cursor {}, idle {} ms, pad {}, io {:.0} MB/s)",
                    self.name,
                    if p.input.cursor_visible { "visible" } else { "hidden" },
                    p.input.idle_ms,
                    p.input.pad_active,
                    p.game_io_mbps
                );
            }
            self.clock.add(phase);
            if self.ring.len() == RING_LEN {
                self.ring.pop_front();
            }
            self.ring.push_back((p.t, phase));

            let playing = phase == Phase::Gameplay;
            if phase != Phase::Away {
                p.tick.fps = (frames > 0).then_some(frames as f32);
                for ms in tracker.intervals(from, p.t) {
                    self.all.add_frame(ms);
                    if playing {
                        self.gameplay.add_frame(ms);
                    }
                }
                self.all.add_active(SECOND);
                self.all.add_second(&p.tick);
                if playing {
                    self.gameplay.add_active(SECOND);
                    self.gameplay.add_second(&p.tick);
                }
            }
            let (all, gameplay) = (&mut self.all, &mut self.gameplay);
            self.hitches.retain(|h| {
                let inside = h.end_us > from && h.end_us <= p.t;
                if inside && phase != Phase::Away {
                    all.add_hitch(h.kind, h.severity);
                    if playing {
                        gameplay.add_hitch(h.kind, h.severity);
                    }
                }
                !inside
            });
        }
        // Hitches whose second was never labelled (session paused, clock jumps).
        self.hitches.retain(|h| now - h.end_us < 10 * SECOND);
    }

    pub fn phase_at(&self, t: Micros) -> Option<Phase> {
        self.ring.iter().find(|(end, _)| t > end - SECOND && t <= *end).map(|&(_, p)| p)
    }

    pub fn add_incident(&mut self, marker_us: Micros, cause: Option<&str>) {
        match self.phase_at(marker_us) {
            Some(Phase::Away) => {}
            Some(Phase::Gameplay) | None => {
                // None: analysed before its second was labelled; most likely gameplay.
                self.all.add_incident(cause);
                self.gameplay.add_incident(cause);
            }
            Some(_) => self.all.add_incident(cause),
        }
    }

    /// Seconds spent in the game (menus and loading screens included).
    pub fn played_sec(&self) -> u64 {
        self.all.active_sec()
    }

    pub fn cursor_game(&self) -> Option<bool> {
        self.classifier.cursor_game()
    }

    pub fn start_ts(&self) -> i64 {
        self.all.start_ts
    }

    /// Gameplay statistics, with the "everything" view in `all` and the time breakdown.
    pub fn stats(&self, id: &str, end_ts: i64) -> SessionStats {
        let phases = self.clock.totals();
        let mut all = self.all.stats(id, end_ts, "all");
        all.phases = phases.clone();
        let mut s = self.gameplay.stats(id, end_ts, "gameplay");
        s.phases = phases;
        s.all = Some(Box::new(all));
        s
    }
}
