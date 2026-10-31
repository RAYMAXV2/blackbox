//! SystemInfo + configuration health checks for the System page.

use crate::recorder::Recorder;
use blackbox_core::model::*;
use blackbox_core::params;
use blackbox_sys::cputemp::Unavailable as CpuTempUnavailable;
use blackbox_sys::sysinfo as sys;

fn yes_no(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

fn check(id: &str, state: CheckState, params: Params) -> HealthCheck {
    HealthCheck { id: id.to_owned(), state, params }
}

pub(crate) fn build(r: &Recorder) -> SystemInfo {
    let m = &r.machine;
    let mut checks = Vec::new();

    checks.push(check("chk.elevation", if r.elevated { CheckState::Ok } else { CheckState::Warn }, Params::new()));
    let etw = |err: Option<u32>, running: bool| -> (CheckState, Params) {
        match (running, err) {
            (true, _) => (CheckState::Ok, Params::new()),
            (false, Some(e)) => (CheckState::Warn, params! {"error" => e}),
            (false, None) => (CheckState::Info, Params::new()),
        }
    };
    let full = r.mode() == Mode::Full;
    let (s, p) = etw(r.kernel_err, full);
    checks.push(check("chk.etw_kernel", s, p));
    let (s, p) = etw(r.frames_err, full && r.frames_err.is_none());
    checks.push(check("chk.etw_presents", s, p));

    if let Some(plan) = &r.power_plan {
        let state = if plan.is_power_saver { CheckState::Warn } else { CheckState::Ok };
        checks.push(check("chk.power_plan", state, params! {"name" => plan.name.as_str()}));
    }
    if let Some(pct) = r.battery {
        checks.push(check("chk.on_battery", CheckState::Warn, params! {"pct" => pct}));
    }
    let gm = sys::game_mode_enabled();
    checks.push(check(
        "chk.game_mode",
        if gm { CheckState::Ok } else { CheckState::Info },
        params! {"enabled" => yes_no(gm)},
    ));
    let hags = match sys::hags_enabled() {
        Some(true) => "yes",
        Some(false) => "no",
        None => "unknown",
    };
    checks.push(check("chk.hags", CheckState::Info, params! {"enabled" => hags}));
    // Memory integrity costs a few % in some games: worth knowing when it's on.
    let hvci = sys::hvci_enabled();
    checks.push(check(
        "chk.vbs",
        if hvci { CheckState::Info } else { CheckState::Ok },
        params! {"hvci" => yes_no(hvci)},
    ));
    if let Some((cur, fine, coarse)) = sys::timer_resolution_ms() {
        checks.push(check(
            "chk.timer_resolution",
            CheckState::Info,
            params! {"currentMs" => round2(cur), "minMs" => round2(fine), "maxMs" => round2(coarse)},
        ));
    }
    // CPU temperature needs the PawnIO driver (installed by FanControl, LibreHardwareMonitor, or on its own).
    let (state, reason) = match (r.has_cpu_temp(), r.cpu_temp_err) {
        (true, _) => (CheckState::Ok, "ok"),
        (false, Some(CpuTempUnavailable::NotInstalled)) => (CheckState::Info, "not_installed"),
        (false, Some(CpuTempUnavailable::NotElevated)) => (CheckState::Info, "not_elevated"),
        (false, Some(CpuTempUnavailable::UnsupportedCpu)) => (CheckState::Info, "unsupported"),
        (false, _) => (CheckState::Info, "failed"),
    };
    let mut p = params! {"reason" => reason};
    if let Some(t) = r.cpu_temp_c {
        p.insert("c".into(), ParamValue::from(t.round()));
    }
    checks.push(check("chk.cpu_temp", state, p));
    let nvml = r.gpu_name.is_some();
    checks.push(check(
        "chk.nvml",
        if nvml { CheckState::Ok } else { CheckState::Info },
        params! {"available" => yes_no(nvml)},
    ));

    let now_ms = r.clock.now_unix_ms();
    SystemInfo {
        version: r.version.clone(),
        os: m.os.clone(),
        build: m.build.clone(),
        cpu: m.cpu.clone(),
        cores: m.cores,
        threads: m.threads,
        gpu: r.gpu_name.clone().or_else(|| m.gpu_registry_name.clone()),
        ram_mb: r.mem.total_mb.max(blackbox_sys::system::memory().total_mb),
        elevated: r.elevated,
        boot_ts: now_ms - m.uptime_ms as i64,
        checks,
    }
}

fn round2(v: f32) -> f64 {
    ((v as f64) * 100.0).round() / 100.0
}
