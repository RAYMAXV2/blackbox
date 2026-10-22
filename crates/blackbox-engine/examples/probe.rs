//! Headless run of the engine, for testing and measuring overhead.
//!
//! cargo run --release -p blackbox-engine --example probe -- 30
//! (run from an elevated terminal for full kernel tracing)

#[cfg(windows)]
fn main() {
    use blackbox_core::model::*;
    use blackbox_engine::{Engine, EngineConfig, EngineSink, Notification, TrayState};
    use std::sync::Arc;

    struct PrintSink;
    impl EngineSink for PrintSink {
        fn status(&self, s: &LiveStatus) {
            let game = s.game.as_ref().map(|g| format!(" game={} {:.0}fps p99={:.1}ms hitches={}", g.name, g.fps, g.p99_ms, g.hitches));
            println!(
                "[status] mode={:?} health={:?} cpu={:.0}% mem={}MB dpc/s={} dpcmax={}us isrmax={}us hf/s={} disks={} drivers={} self={:.2}%cpu {:.1}MB ev/s={} lost={}{}",
                s.mode,
                s.health,
                s.cpu.total_pct,
                s.memory.used_mb,
                s.kernel.dpc_per_sec,
                s.kernel.dpc_max_us,
                s.kernel.isr_max_us,
                s.kernel.hard_faults_per_sec,
                s.disks.len(),
                s.drivers.len(),
                s.overhead.cpu_pct,
                s.overhead.working_set_mb,
                s.overhead.events_per_sec,
                s.overhead.events_lost,
                game.unwrap_or_default()
            );
        }
        fn incident(&self, i: &Incident) {
            println!("[incident] {}", serde_json::to_string(i).unwrap_or_default());
        }
        fn notify(&self, n: &Notification) {
            println!("[notify] in_app={} overlay={:?}", n.in_app, n.overlay.as_ref().map(|o| (&o.title, &o.body)));
        }
        fn tray(&self, t: &TrayState) {
            println!("[tray] {t:?}");
        }
    }

    struct Stderr;
    impl log::Log for Stderr {
        fn enabled(&self, m: &log::Metadata) -> bool {
            m.level() <= log::Level::Info
        }
        fn log(&self, r: &log::Record) {
            eprintln!("[log] {} {}", r.level(), r.args());
        }
        fn flush(&self) {}
    }
    static LOGGER: Stderr = Stderr;
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Info);

    let secs: u64 = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(20);
    let dir = std::env::temp_dir().join("blackbox-probe");
    let engine = Engine::start(EngineConfig { data_dir: dir, version: env!("CARGO_PKG_VERSION").into() }, Arc::new(PrintSink));
    engine.set_ui_active(true);
    std::thread::sleep(std::time::Duration::from_secs(secs));

    let boot = engine.bootstrap();
    println!("[system] {} | {} | {} | elevated={}", boot.system.os, boot.system.cpu, boot.system.gpu.unwrap_or_default(), boot.system.elevated);
    for c in &boot.system.checks {
        println!("[check] {} {:?} {:?}", c.id, c.state, c.params);
    }
    for d in engine.drivers().iter().take(12) {
        println!(
            "[driver] {:<22} {:<10?} dpc={} total={}us max={}us isr={} max={}us",
            d.live.name, d.live.category, d.live.dpc_count, d.live.dpc_total_us, d.live.dpc_max_us, d.live.isr_count, d.live.isr_max_us
        );
    }
    for p in engine.processes().iter().take(5) {
        println!("[proc] {:<28} {:>5.1}% {:>8.0} B/s {:?}", p.name, p.cpu_pct, p.io_bps, p.tag);
    }
    println!("[incidents] {}", boot.incidents.len());
    engine.shutdown();
}

#[cfg(not(windows))]
fn main() {}
