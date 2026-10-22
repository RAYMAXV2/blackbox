// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod instance;
mod logging;
mod overlay;
mod sink;
mod tray;

use blackbox_core::model::*;
use blackbox_engine::{Engine, EngineConfig};
use overlay::Overlay;
use sink::AppSink;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder, Window, WindowEvent};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const MAIN: &str = "main";

pub struct AppState {
    pub engine: Arc<Engine>,
    pub sink: Arc<AppSink>,
    pub data_dir: PathBuf,
    _instance: Mutex<Option<instance::Instance>>,
}

fn data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Blackbox")
}

/// Shows the dashboard, creating its window (and WebView) on demand. Any thread.
pub fn show_main(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = handle.get_webview_window(MAIN) {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
            return;
        }
        let built = WebviewWindowBuilder::new(&handle, MAIN, WebviewUrl::App("index.html".into()))
            .title("Blackbox")
            .inner_size(1280.0, 820.0)
            .min_inner_size(960.0, 640.0)
            .decorations(false)
            .background_color(tauri::window::Color(0, 0, 0, 255))
            .center()
            .build();
        match built {
            Ok(_) => {
                if let Some(s) = handle.try_state::<AppState>() {
                    s.sink.window_open.store(true, Ordering::Relaxed);
                }
            }
            Err(e) => log::error!("dashboard window: {e}"),
        }
    });
}

fn on_window_event(window: &Window, event: &WindowEvent) {
    let Some(state) = window.app_handle().try_state::<AppState>() else { return };
    match event {
        WindowEvent::Focused(f) => state.sink.window_focused.store(*f, Ordering::Relaxed),
        // Closing the dashboard destroys the WebView: its processes and memory go away,
        // the recorder keeps running in the tray.
        WindowEvent::Destroyed => {
            state.sink.window_open.store(false, Ordering::Relaxed);
            state.sink.window_focused.store(false, Ordering::Relaxed);
            state.engine.set_ui_active(false);
        }
        _ => {}
    }
}

// ───────────────────────────── commands (see src/lib/contract.ts)
// Anything that may touch the disk or spawn a process runs off the main thread (`async`);
// the rest only reads in-memory state.

type CmdResult<T> = Result<T, String>;

#[tauri::command(async)]
fn get_bootstrap(state: State<'_, AppState>) -> CmdResult<Bootstrap> {
    Ok(state.engine.bootstrap())
}

#[tauri::command(async)]
fn get_incident(state: State<'_, AppState>, id: String) -> CmdResult<Option<Incident>> {
    Ok(state.engine.incident(&id))
}

#[tauri::command]
fn mark_read(state: State<AppState>, id: Option<String>) {
    state.engine.mark_read(id);
}

#[tauri::command]
fn delete_incident(state: State<AppState>, id: String) {
    state.engine.delete(id);
}

#[tauri::command]
fn clear_incidents(state: State<AppState>) {
    state.engine.clear();
}

#[tauri::command(async)]
fn export_incident(state: State<'_, AppState>, id: String) -> CmdResult<String> {
    let dir = state.data_dir.join("exports");
    let path = state.engine.export(&id, &dir).map_err(|e| e.to_string())?;
    reveal(&path);
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command(async)]
fn get_drivers(state: State<'_, AppState>) -> CmdResult<Vec<DriverInfo>> {
    Ok(state.engine.drivers())
}

#[tauri::command(async)]
fn get_processes(state: State<'_, AppState>) -> CmdResult<Vec<ProcLive>> {
    Ok(state.engine.processes())
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.engine.settings()
}

#[tauri::command(async)]
fn set_settings(state: State<'_, AppState>, settings: Settings) -> CmdResult<Settings> {
    let current = state.engine.settings();
    let mut settings = settings;
    if settings.autostart != current.autostart {
        if let Err(e) = autostart::set(settings.autostart) {
            log::warn!("autostart: {e}");
            // Keep the rest of the change, report why autostart didn't switch.
            settings.autostart = current.autostart;
            state.engine.set_settings(settings);
            return Err(if e == "admin" { "autostart_needs_admin".into() } else { e });
        }
    }
    Ok(state.engine.set_settings(settings))
}

#[tauri::command]
fn set_ui_active(state: State<AppState>, active: bool) {
    state.engine.set_ui_active(active);
}

#[tauri::command]
fn restart_elevated(app: AppHandle) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let w = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (verb, file, args) = (w("runas"), w(&exe.to_string_lossy()), w("--elevated-restart"));
    let r = unsafe {
        ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), args.as_ptr(), std::ptr::null(), SW_SHOWNORMAL)
    };
    if r as isize > 32 {
        app.exit(0);
        Ok(())
    } else {
        Err("cancelled".into())
    }
}

#[tauri::command]
fn test_notification(state: State<AppState>) {
    state.engine.test_notification();
}

#[tauri::command(async)]
fn open_data_folder(state: State<'_, AppState>) -> CmdResult<()> {
    let _ = std::fs::create_dir_all(&state.data_dir);
    std::process::Command::new("explorer.exe").arg(&state.data_dir).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn get_games(state: State<'_, AppState>) -> CmdResult<Vec<GameRecord>> {
    Ok(state.engine.games())
}

#[tauri::command(async)]
fn delete_game(state: State<'_, AppState>, name: String) -> CmdResult<()> {
    state.engine.delete_game(&name);
    Ok(())
}

fn reveal(path: &std::path::Path) {
    use std::os::windows::process::CommandExt;
    // explorer parses /select itself: pass the quoted path verbatim.
    let _ = std::process::Command::new("explorer.exe").raw_arg(format!("/select,\"{}\"", path.display())).spawn();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let background = args.iter().any(|a| a == "--background");
    let restarted = args.iter().any(|a| a == "--elevated-restart");
    let data_dir = data_dir();
    logging::init(&data_dir);
    log::info!("Blackbox {VERSION} starting, elevated={}", blackbox_sys::util::is_elevated());

    let Some(instance) = instance::acquire(restarted) else {
        log::info!("already running: asking it to show the dashboard");
        instance::signal_show();
        return;
    };

    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_bootstrap,
            get_incident,
            mark_read,
            delete_incident,
            clear_incidents,
            export_incident,
            get_drivers,
            get_processes,
            get_settings,
            set_settings,
            set_ui_active,
            restart_elevated,
            test_notification,
            open_data_folder,
            get_games,
            delete_game,
        ])
        .on_window_event(on_window_event)
        .setup(move |app| {
            let handle = app.handle().clone();
            let click = handle.clone();
            let overlay = Overlay::start(Arc::new(move || show_main(&click)));
            let sink = Arc::new(AppSink::new(handle.clone(), overlay));
            let engine = Arc::new(Engine::start(
                EngineConfig { data_dir: data_dir.clone(), version: VERSION.into() },
                sink.clone(),
            ));
            let settings = engine.settings();
            app.manage(AppState { engine, sink, data_dir, _instance: Mutex::new(Some(instance)) });
            tray::create(&handle, settings.lang, settings.paused)?;
            let show = handle.clone();
            instance::listen_show(move || show_main(&show));
            if !background {
                show_main(&handle);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build Blackbox");

    app.run(|app, event| match event {
        // Closing the dashboard must not quit: the recorder lives in the tray.
        RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
        RunEvent::Exit => {
            if let Some(s) = app.try_state::<AppState>() {
                s.engine.shutdown();
            }
            log::info!("Blackbox stopped");
        }
        _ => {}
    });
}
