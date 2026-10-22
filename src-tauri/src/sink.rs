//! Engine -> app bridge: Tauri events for the UI, the native overlay, the tray icon.
//! Called from the engine thread, so nothing here may block on the main thread.

use crate::overlay::Overlay;
use crate::tray;
use blackbox_core::model::{Incident, LiveStatus, Settings};
use blackbox_engine::{EngineSink, Notification, TrayState};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};

pub struct AppSink {
    app: AppHandle,
    overlay: Overlay,
    /// Maintained from window events on the main thread.
    pub window_open: AtomicBool,
    pub window_focused: AtomicBool,
}

impl AppSink {
    pub fn new(app: AppHandle, overlay: Overlay) -> AppSink {
        AppSink { app, overlay, window_open: AtomicBool::new(false), window_focused: AtomicBool::new(false) }
    }
}

impl EngineSink for AppSink {
    fn status(&self, status: &LiveStatus) {
        if self.window_open.load(Ordering::Relaxed) {
            let _ = self.app.emit("bb:status", status);
        }
    }

    fn incident(&self, incident: &Incident) {
        if self.window_open.load(Ordering::Relaxed) {
            let _ = self.app.emit("bb:incident", incident);
        }
    }

    fn notify(&self, n: &Notification) {
        let open = self.window_open.load(Ordering::Relaxed);
        if n.in_app && open {
            let _ = self.app.emit("bb:toast", &n.toast);
        }
        // The dashboard already shows it when focused: no need for the overlay too.
        if let Some(text) = &n.overlay {
            if !(open && self.window_focused.load(Ordering::Relaxed)) {
                self.overlay.show(text.clone());
            }
        }
    }

    fn tray(&self, state: &TrayState) {
        tray::update(&self.app, state);
    }

    fn settings(&self, settings: &Settings) {
        if self.window_open.load(Ordering::Relaxed) {
            let _ = self.app.emit("bb:settings", settings);
        }
    }
}
