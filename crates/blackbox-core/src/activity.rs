//! Is the player actually playing? Labels every second of a game session as gameplay, menu,
//! loading or away, from signals readable outside the game (no injection):
//!
//! - **Away**: the game isn't in front.
//! - **Loading**: the game stops rendering (no frames / under 15 fps for 2 s and more), or reads
//!   heavily from disk while barely rendering or while the player waits.
//! - **Menu**: in games that hide the cursor during play, a visible cursor; in any game, no
//!   keyboard, mouse or controller input for a few seconds (menus, AFK, cutscenes).
//! - **Gameplay**: everything else.
//!
//! Whether a game hides its cursor during play is learned (and remembered per game).

use crate::model::PhaseTotals;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Gameplay,
    Menu,
    Loading,
    Away,
}

/// Why a second got its label (logged on transitions, to tune the rules).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    NotFocused,
    NotRendering,
    DiskLoad,
    CursorVisible,
    NoInput,
    Playing,
}

/// What we know about one second of the session.
#[derive(Debug, Clone, Copy, Default)]
pub struct SecondSignals {
    pub focused: bool,
    pub frames: u32,
    pub fps: f32,
    pub cursor_visible: bool,
    pub cursor_confined: bool,
    pub idle_ms: u32,
    pub pad_active: bool,
    /// The game's own I/O, MB/s.
    pub game_io_mbps: f32,
}

const STARVED_FPS: f32 = 15.0;
const STARVED_SECONDS: u32 = 2;
const HEAVY_IO_MBPS: f32 = 40.0;
const IDLE_MS: u32 = 5_000;
/// Cursor learning: evidence needed before trusting it, and the decision thresholds.
const CURSOR_MIN_SECONDS: u32 = 30;
const CURSOR_HIDDEN_SHARE: f32 = 0.25;
const CURSOR_NEVER_SHARE: f32 = 0.05;
const CURSOR_NEVER_SECONDS: u32 = 120;
/// A menu needs two seconds of menu signals in a row (a quick wheel or map doesn't count).
const MENU_SECONDS: u32 = 2;

#[derive(Debug, Clone)]
pub struct PhaseClassifier {
    hint: Option<bool>,
    seen: u32,
    hidden: u32,
    starved: u32,
    menu_streak: u32,
    last: Phase,
}

impl PhaseClassifier {
    /// `cursor_hint`: what earlier sessions of this game learned.
    pub fn new(cursor_hint: Option<bool>) -> PhaseClassifier {
        PhaseClassifier { hint: cursor_hint, seen: 0, hidden: 0, starved: 0, menu_streak: 0, last: Phase::Away }
    }

    /// Does this game hide the cursor during play? `None` until there's enough evidence.
    pub fn cursor_game(&self) -> Option<bool> {
        if self.seen >= CURSOR_MIN_SECONDS {
            let share = self.hidden as f32 / self.seen as f32;
            if share >= CURSOR_HIDDEN_SHARE {
                return Some(true);
            }
            if share < CURSOR_NEVER_SHARE && self.seen >= CURSOR_NEVER_SECONDS {
                return Some(false);
            }
        }
        self.hint
    }

    pub fn last(&self) -> Phase {
        self.last
    }

    pub fn classify(&mut self, s: &SecondSignals) -> (Phase, Why) {
        let out = self.decide(s);
        self.last = out.0;
        out
    }

    fn decide(&mut self, s: &SecondSignals) -> (Phase, Why) {
        if !s.focused {
            self.starved = 0;
            self.menu_streak = 0;
            return (Phase::Away, Why::NotFocused);
        }
        let starved = s.frames == 0 || s.fps < STARVED_FPS;
        self.starved = if starved { self.starved + 1 } else { 0 };
        let waiting = s.idle_ms >= 1_500 && !s.pad_active;
        if self.starved >= STARVED_SECONDS {
            self.menu_streak = 0;
            return (Phase::Loading, Why::NotRendering);
        }
        if s.game_io_mbps >= HEAVY_IO_MBPS && (starved || waiting) {
            self.menu_streak = 0;
            return (Phase::Loading, Why::DiskLoad);
        }

        // Learn the cursor habit on rendering seconds only.
        self.seen += 1;
        if !s.cursor_visible || s.cursor_confined {
            self.hidden += 1;
        }
        let cursor_menu = self.cursor_game() == Some(true) && s.cursor_visible && !s.cursor_confined;
        let no_input = s.idle_ms >= IDLE_MS && !s.pad_active;
        if cursor_menu || no_input {
            self.menu_streak += 1;
            if self.menu_streak >= MENU_SECONDS || self.last == Phase::Menu {
                return (Phase::Menu, if cursor_menu { Why::CursorVisible } else { Why::NoInput });
            }
        } else {
            self.menu_streak = 0;
        }
        (Phase::Gameplay, Why::Playing)
    }
}

/// Time spent in each phase, plus loading screens (count and durations).
#[derive(Debug, Clone, Default)]
pub struct PhaseClock {
    totals: PhaseTotals,
    loading_run: u32,
}

impl PhaseClock {
    pub fn add(&mut self, phase: Phase) {
        let t = &mut self.totals;
        match phase {
            Phase::Gameplay => t.gameplay_sec += 1,
            Phase::Menu => t.menu_sec += 1,
            Phase::Loading => t.loading_sec += 1,
            Phase::Away => t.away_sec += 1,
        }
        if phase == Phase::Loading {
            if self.loading_run == 0 {
                t.loadings += 1;
            }
            self.loading_run += 1;
            t.loading_max_sec = t.loading_max_sec.max(self.loading_run);
        } else {
            self.loading_run = 0;
        }
        t.loading_avg_sec = if t.loadings > 0 { t.loading_sec as f32 / t.loadings as f32 } else { 0.0 };
    }

    pub fn totals(&self) -> PhaseTotals {
        let mut t = self.totals.clone();
        t.loading_avg_sec = (t.loading_avg_sec * 10.0).round() / 10.0;
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(cursor_visible: bool) -> SecondSignals {
        SecondSignals { focused: true, frames: 240, fps: 240.0, cursor_visible, idle_ms: 50, ..Default::default() }
    }

    fn run(c: &mut PhaseClassifier, s: SecondSignals, n: usize) -> Vec<Phase> {
        (0..n).map(|_| c.classify(&s).0).collect()
    }

    #[test]
    fn hidden_cursor_game_learns_and_detects_menus() {
        let mut c = PhaseClassifier::new(None);
        assert!(run(&mut c, play(false), 40).iter().all(|p| *p == Phase::Gameplay));
        assert_eq!(c.cursor_game(), Some(true));
        let menu = run(&mut c, play(true), 5);
        assert_eq!(menu[0], Phase::Gameplay, "a single second isn't a menu");
        assert!(menu[1..].iter().all(|p| *p == Phase::Menu));
        assert_eq!(run(&mut c, play(false), 1)[0], Phase::Gameplay);
    }

    #[test]
    fn visible_cursor_game_relies_on_input() {
        let mut c = PhaseClassifier::new(None);
        run(&mut c, play(true), 200); // a MOBA: cursor always visible
        assert_eq!(c.cursor_game(), Some(false));
        assert_eq!(run(&mut c, play(true), 3), vec![Phase::Gameplay; 3]);
        let idle = SecondSignals { idle_ms: 8_000, ..play(true) };
        assert_eq!(run(&mut c, idle, 3)[2], Phase::Menu);
        let pad = SecondSignals { idle_ms: 8_000, pad_active: true, ..play(true) };
        assert_eq!(run(&mut c, pad, 1)[0], Phase::Gameplay, "controller players are playing");
    }

    #[test]
    fn loading_needs_two_starved_seconds_or_disk_load() {
        let mut c = PhaseClassifier::new(Some(true));
        let stalled = SecondSignals { frames: 0, fps: 0.0, ..play(false) };
        assert_eq!(
            run(&mut c, stalled, 3),
            vec![Phase::Gameplay, Phase::Loading, Phase::Loading],
            "1 s freeze = hitch"
        );
        let disk = SecondSignals { game_io_mbps: 300.0, idle_ms: 4_000, ..play(false) };
        assert_eq!(run(&mut c, disk, 1)[0], Phase::Loading);
        let streaming = SecondSignals { game_io_mbps: 300.0, ..play(false) };
        assert_eq!(run(&mut c, streaming, 1)[0], Phase::Gameplay, "open-world streaming while playing");
        assert_eq!(run(&mut c, SecondSignals::default(), 1)[0], Phase::Away);
    }

    #[test]
    fn phase_clock_counts_loading_screens() {
        let mut k = PhaseClock::default();
        for p in [Phase::Loading, Phase::Loading, Phase::Gameplay, Phase::Loading, Phase::Menu, Phase::Away] {
            k.add(p);
        }
        let t = k.totals();
        assert_eq!((t.gameplay_sec, t.menu_sec, t.loading_sec, t.away_sec), (1, 1, 3, 1));
        assert_eq!((t.loadings, t.loading_max_sec, t.loading_avg_sec), (2, 2, 1.5));
    }
}
