//! Builds the live status pushed to the UI (only while the UI is visible).

use crate::recorder::Recorder;
use blackbox_core::health::{self, HealthInput};
use blackbox_core::kb;
use blackbox_core::model::*;
use blackbox_core::series::*;
use blackbox_sys::etw::kernel::DriverStat;

impl Recorder {
    pub(crate) fn build_status(&mut self, now: Micros) -> LiveStatus {
        let mode = self.mode();
        // Kernel data lags ETW's 1 s flush: "the last second" ends at the newest bucket.
        let horizon = self.kring.last_idx().map_or(now, |i| (i + 1) * BUCKET_US);
        let last_s: Vec<&KernelBucket> = self.kring.range(horizon - 1_000_000, horizon).collect();

        let kernel = if mode == Mode::Full {
            let total_us = 1e6 * self.ncpu as f32;
            KernelLive {
                dpc_per_sec: last_s.iter().map(|b| b.dpc_count).sum(),
                isr_per_sec: last_s.iter().map(|b| b.isr_count).sum(),
                dpc_max_us: last_s.iter().map(|b| b.dpc_max_us).max().unwrap_or(0),
                isr_max_us: last_s.iter().map(|b| b.isr_max_us).max().unwrap_or(0),
                dpc_time_pct: round2(last_s.iter().map(|b| b.dpc_total_us as f32).sum::<f32>() / total_us * 100.0),
                isr_time_pct: round2(last_s.iter().map(|b| b.isr_total_us as f32).sum::<f32>() / total_us * 100.0),
                hard_faults_per_sec: last_s.iter().map(|b| b.hard_faults).sum(),
                timer_resolution_ms: round2(self.timer_res_ms),
            }
        } else {
            KernelLive {
                dpc_time_pct: round2(self.dpc_pct),
                isr_time_pct: round2(self.isr_pct),
                hard_faults_per_sec: self.procs.iter().map(|p| p.hard_faults).sum(),
                timer_resolution_ms: round2(self.timer_res_ms),
                ..Default::default()
            }
        };

        // Disks seen in the ring, with their activity over the last second.
        let mut disks = Vec::new();
        for slot in 0..MAX_DISKS {
            let seen = self.kring.range(horizon - 60_000_000, horizon).any(|b| b.disks[slot].ios > 0);
            if !seen {
                continue;
            }
            let mut acc = DiskAcc::default();
            for b in &last_s {
                acc.merge(&b.disks[slot]);
            }
            disks.push(DiskLive {
                id: slot as u32,
                label: format!("Disk {slot}"),
                read_bps: acc.read_bytes as f64,
                write_bps: acc.write_bytes as f64,
                iops: acc.ios,
                lat_avg_ms: if acc.ios > 0 { round1(acc.lat_sum_us as f32 / acc.ios as f32 / 1000.0) } else { 0.0 },
                lat_max_ms: round1(acc.lat_max_us as f32 / 1000.0),
            });
        }

        let drivers = self.top_drivers(8);
        let mut procs: Vec<&crate::recorder::ProcNow> = self.procs.iter().filter(|p| p.pid != 0).collect();
        procs.sort_unstable_by(|a, b| b.cpu_pct.total_cmp(&a.cpu_pct));
        let processes = procs.iter().take(12).map(|p| p.to_live()).collect();

        let game = self.game.as_mut().map(|g| {
            let s = g.tracker.stats(now);
            GameLive {
                pid: g.pid,
                name: g.name.to_string(),
                fps: round1(s.fps),
                frame_time_ms: round2(s.frame_time_ms),
                p99_ms: round2(s.p99_ms),
                low1_fps: round1(s.low1_fps),
                hitches: g.tracker.hitches,
                session_sec: ((now - g.started_us) / 1_000_000) as u64,
                frames: s.recent.iter().map(|v| round2(*v)).collect(),
            }
        });

        let worst = drivers
            .iter()
            .map(|d| (d.dpc_max_us.max(d.isr_max_us), d.name.clone()))
            .max_by_key(|(us, _)| *us)
            .unwrap_or_default();
        let disk_lat_avg = disks.iter().map(|d| d.lat_avg_ms).fold(0.0, f32::max);
        let (health, reasons) = health::evaluate(&HealthInput {
            mode: Some(mode),
            recent: self.recent.iter().filter(|(t, _)| now - t < 300_000_000).map(|(_, s)| *s).collect(),
            worst_latency_us: worst.0,
            worst_latency_driver: worst.1,
            mem_load_pct: self.mem.load_pct,
            disk_lat_avg_ms: disk_lat_avg,
            gpu_thermal: self.gpu.as_ref().and_then(|g| g.throttle.iter().copied().find(|t| t.is_thermal())),
            cpu_limit_pct: self.freq_limit,
            cpu_total_pct: self.cpu_total,
            events_lost_recent: self.lost_recent,
        });

        let (total, unread) = {
            let s = self.store.lock().unwrap_or_else(|e| e.into_inner());
            (s.len() as u32, s.unread())
        };

        LiveStatus {
            ts: self.clock.us_to_unix_ms(now),
            mode,
            health,
            health_reasons: reasons,
            overhead: Overhead {
                cpu_pct: round2(self.self_cpu_pct),
                working_set_mb: round1(self.self_usage.working_set as f32 / 1048576.0),
                private_mb: round1(self.self_usage.private_bytes as f32 / 1048576.0),
                events_per_sec: self.events_rate,
                events_lost: self.events_lost,
                uptime_sec: ((now - self.start_us) / 1_000_000) as u64,
            },
            cpu: CpuLive {
                name: self.cpu_name.clone(),
                total_pct: round1(self.cpu_total),
                kernel_pct: round1(self.cpu_kernel),
                per_core: self.per_core.clone(),
                freq_limit_pct: round1(self.freq_limit),
                throttled: self.freq_limit < 90.0,
                processes: self.proc_count,
                threads: self.thread_count,
            },
            memory: MemLive {
                total_mb: self.mem.total_mb,
                used_mb: self.mem.total_mb.saturating_sub(self.mem.avail_mb),
                available_mb: self.mem.avail_mb,
                load_pct: self.mem.load_pct,
                commit_mb: self.mem.commit_mb,
                commit_limit_mb: self.mem.commit_limit_mb,
                hard_faults_per_sec: kernel.hard_faults_per_sec,
            },
            gpu: self.gpu.as_ref().map(|g| GpuLive {
                name: self.gpu_name.clone().unwrap_or_default(),
                util_pct: g.util_pct,
                mem_util_pct: g.mem_util_pct,
                temp_c: g.temp_c,
                power_w: g.power_w.map(round1),
                power_limit_w: g.power_limit_w.map(round1),
                core_mhz: g.core_mhz,
                mem_mhz: g.mem_mhz,
                fan_pct: g.fan_pct,
                vram_used_mb: g.vram_used_mb,
                vram_total_mb: g.vram_total_mb,
                throttle: g.throttle.clone(),
            }),
            disks,
            net: NetLive { rx_bps: self.net_rate.0.round(), tx_bps: self.net_rate.1.round() },
            kernel,
            drivers,
            processes,
            game,
            counters: Counters { incidents_total: total, incidents_session: self.incidents_session, unread },
        }
    }

    /// Drivers with the most DPC+ISR time over the last 10 s.
    pub(crate) fn top_drivers(&self, n: usize) -> Vec<DriverLive> {
        let mut agg: Vec<(u16, DriverStat)> = Vec::new();
        for second in &self.drv_window {
            for (id, d) in second {
                match agg.iter_mut().find(|(i, _)| i == id) {
                    Some((_, a)) => {
                        a.merge(d);
                    }
                    None => agg.push((*id, *d)),
                }
            }
        }
        agg.sort_unstable_by_key(|(_, d)| std::cmp::Reverse(d.dpc_total_us + d.isr_total_us));
        agg.into_iter()
            .take(n)
            .map(|(id, d)| {
                let name = self.registry.name(id).unwrap_or_else(|| "unknown".into());
                let (friendly, category) = kb::driver_info(&name);
                DriverLive {
                    friendly: friendly.map(str::to_owned),
                    category,
                    dpc_count: d.dpc_count,
                    dpc_total_us: d.dpc_total_us,
                    dpc_max_us: d.dpc_max_us,
                    isr_count: d.isr_count,
                    isr_total_us: d.isr_total_us,
                    isr_max_us: d.isr_max_us,
                    name,
                }
            })
            .collect()
    }
}

fn round1(v: f32) -> f32 {
    (v * 10.0).round() / 10.0
}

fn round2(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}
