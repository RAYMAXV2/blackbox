//! Global health state shown at the top of the dashboard.

use crate::model::{GpuThrottle, Health, Mode, Reason, Severity};
use crate::params;

#[derive(Debug, Clone, Default)]
pub struct HealthInput {
    pub mode: Option<Mode>,
    /// Incidents of the last 5 minutes.
    pub recent: Vec<Severity>,
    /// Worst DPC/ISR over the last 10 s and the driver responsible.
    pub worst_latency_us: u32,
    pub worst_latency_driver: String,
    pub mem_load_pct: f32,
    pub disk_lat_avg_ms: f32,
    pub gpu_thermal: Option<GpuThrottle>,
    pub cpu_limit_pct: f32,
    pub cpu_total_pct: f32,
    pub events_lost_recent: u64,
}

pub fn evaluate(h: &HealthInput) -> (Health, Vec<Reason>) {
    let mut reasons = Vec::new();
    let mut level = 0u8; // 0 ok, 1 warn, 2 bad
    let mut raise = |l: u8, r: Reason, reasons: &mut Vec<Reason>| {
        level = level.max(l);
        reasons.push(r);
    };
    match h.mode {
        Some(Mode::Paused) => return (Health::Idle, vec![Reason::bare("hr.paused")]),
        Some(Mode::Limited) => raise(1, Reason::bare("hr.limited"), &mut reasons),
        _ => {}
    }
    if !h.recent.is_empty() {
        let severe = h.recent.contains(&Severity::Severe);
        let l = if severe || h.recent.len() >= 3 { 2 } else { 1 };
        raise(l, Reason::new("hr.incidents_recent", params! {"count" => h.recent.len()}), &mut reasons);
    }
    if h.worst_latency_us >= 1000 {
        let l = if h.worst_latency_us >= 4000 { 2 } else { 1 };
        raise(
            l,
            Reason::new("hr.dpc_high", params! {"driver" => h.worst_latency_driver.as_str(), "us" => h.worst_latency_us}),
            &mut reasons,
        );
    }
    if h.mem_load_pct >= 90.0 {
        let l = if h.mem_load_pct >= 97.0 { 2 } else { 1 };
        raise(l, Reason::new("hr.mem_high", params! {"pct" => h.mem_load_pct.round()}), &mut reasons);
    }
    if h.disk_lat_avg_ms >= 50.0 {
        raise(1, Reason::new("hr.disk_slow", params! {"ms" => h.disk_lat_avg_ms.round()}), &mut reasons);
    }
    if let Some(t) = h.gpu_thermal {
        raise(1, Reason::new("hr.gpu_throttle", params! {"reason" => t.id()}), &mut reasons);
    }
    if h.cpu_limit_pct > 0.0 && h.cpu_limit_pct < 90.0 {
        raise(1, Reason::new("hr.cpu_throttle", params! {"pct" => h.cpu_limit_pct.round()}), &mut reasons);
    }
    if h.cpu_total_pct >= 95.0 {
        raise(1, Reason::new("hr.cpu_saturated", params! {"pct" => h.cpu_total_pct.round()}), &mut reasons);
    }
    if h.events_lost_recent > 0 {
        raise(1, Reason::new("hr.events_lost", params! {"count" => h.events_lost_recent}), &mut reasons);
    }
    let health = match level {
        0 => Health::Ok,
        1 => Health::Warn,
        _ => Health::Bad,
    };
    (health, reasons)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_machine_is_ok() {
        let (h, r) = evaluate(&HealthInput { mode: Some(Mode::Full), cpu_limit_pct: 100.0, ..Default::default() });
        assert_eq!(h, Health::Ok);
        assert!(r.is_empty());
    }

    #[test]
    fn severe_incident_is_bad_and_paused_is_idle() {
        let (h, _) = evaluate(&HealthInput { recent: vec![Severity::Severe], ..Default::default() });
        assert_eq!(h, Health::Bad);
        let (h, r) = evaluate(&HealthInput { mode: Some(Mode::Paused), recent: vec![Severity::Severe], ..Default::default() });
        assert_eq!((h, r[0].key.as_str()), (Health::Idle, "hr.paused"));
    }

    #[test]
    fn limited_mode_warns() {
        let (h, r) = evaluate(&HealthInput { mode: Some(Mode::Limited), ..Default::default() });
        assert_eq!(h, Health::Warn);
        assert_eq!(r[0].key, "hr.limited");
    }
}
