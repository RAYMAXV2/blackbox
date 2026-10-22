//! Minimal file logger: `<data dir>\blackbox.log`, reset when it grows past 1 MB.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_BYTES: u64 = 1024 * 1024;

struct FileLogger {
    file: Mutex<File>,
    level: log::LevelFilter,
}

impl log::Log for FileLogger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= self.level
    }

    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        if let Ok(mut f) = self.file.lock() {
            let _ = writeln!(f, "{secs} {:<5} {}: {}", r.level(), r.target(), r.args());
        }
    }

    fn flush(&self) {
        if let Ok(mut f) = self.file.lock() {
            let _ = f.flush();
        }
    }
}

pub fn init(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    let path = dir.join("blackbox.log");
    let too_big = std::fs::metadata(&path).map(|m| m.len() > MAX_BYTES).unwrap_or(false);
    let file = if too_big { File::create(&path) } else { OpenOptions::new().create(true).append(true).open(&path) };
    let Ok(file) = file else { return };
    let level = if cfg!(debug_assertions) { log::LevelFilter::Debug } else { log::LevelFilter::Info };
    if log::set_boxed_logger(Box::new(FileLogger { file: Mutex::new(file), level })).is_ok() {
        log::set_max_level(level);
    }
}
