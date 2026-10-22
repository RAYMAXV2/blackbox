//! Tray icon: the recorder's only permanent UI. Left click opens the dashboard.

use crate::{show_main, AppState};
use blackbox_core::i18n;
use blackbox_core::model::{Health, Lang};
use blackbox_engine::TrayState;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

const ID: &str = "main";
const ICON_OK: &[u8] = include_bytes!("../icons/tray-ok.png");
const ICON_ALERT: &[u8] = include_bytes!("../icons/tray-alert.png");
const ICON_PAUSED: &[u8] = include_bytes!("../icons/tray-paused.png");

pub fn create(app: &AppHandle, lang: Lang, paused: bool) -> tauri::Result<()> {
    let menu = build_menu(app, lang, paused)?;
    TrayIconBuilder::with_id(ID)
        .icon(Image::from_bytes(if paused { ICON_PAUSED } else { ICON_OK })?)
        .tooltip(i18n::tray_tooltip(!paused, 0, lang))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "pause" => {
                if let Some(state) = app.try_state::<AppState>() {
                    let mut s = state.engine.settings();
                    s.paused = !s.paused;
                    state.engine.set_settings(s);
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn build_menu(app: &AppHandle, lang: Lang, paused: bool) -> tauri::Result<Menu<tauri::Wry>> {
    let l = i18n::tray_menu(lang);
    let open = MenuItem::with_id(app, "open", l.open, true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", if paused { l.resume } else { l.pause }, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", l.quit, true, None::<&str>)?;
    Menu::with_items(app, &[&open, &PredefinedMenuItem::separator(app)?, &pause, &quit])
}

/// Applies a new tray state. Safe to call from any thread.
pub fn update(app: &AppHandle, state: &TrayState) {
    let state = state.clone();
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(tray) = handle.tray_by_id(ID) else { return };
        let icon = if !state.recording {
            ICON_PAUSED
        } else if state.unread > 0 || state.health == Health::Bad {
            ICON_ALERT
        } else {
            ICON_OK
        };
        if let Ok(img) = Image::from_bytes(icon) {
            let _ = tray.set_icon(Some(img));
        }
        let _ = tray.set_tooltip(Some(i18n::tray_tooltip(state.recording, state.unread, state.lang)));
        if let Ok(menu) = build_menu(&handle, state.lang, !state.recording) {
            let _ = tray.set_menu(Some(menu));
        }
    });
}
