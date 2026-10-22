# Blackbox architecture

Blackbox is a flight recorder for Windows performance. It records what the kernel, drivers,
disks, CPU, GPU and apps do, detects stutters, freezes, latency spikes and hangs, and explains
each one with evidence, ruled-out checks and fixes. The whole design is driven by one rule:
**the recorder must cost (almost) nothing.**

## Processes and threads

```
blackbox.exe (one process)
├─ main thread            Tauri event loop, tray icon, commands from the UI
├─ bb-engine              1 Hz tick: snapshots, detectors, analysis, persistence   (background priority, EcoQoS)
├─ bb-etw-blackbox-kernel ETW consumer: DPC/ISR, disk I/O, hard faults, processes  (normal: never starved)
├─ bb-etw-blackbox-frames ETW consumer: DXGI/D3D9 Present_Start                    (normal: never starved)
├─ bb-foreground          WinEvent hook, sleeps in GetMessage between focus changes
├─ bb-overlay             native GDI notification window, sleeps in GetMessage
└─ bb-instance            waits on the "show" event of a second launch

msedgewebview2.exe (only while the dashboard is open)
```

Closing the dashboard **destroys** the WebView: its processes and memory go away, the recorder
keeps running in the tray. While the dashboard is hidden or minimized, the engine does not even
build the live status.

## Data flow

```
 Kernel (SystemTraceProvider)            User providers (DXGI, D3D9)
   DPC / ISR / DiskIo / HardFault / Process   Present_Start only (filtered in-provider)
        │                                              │
        ▼                                              ▼
   KernelSink (fixed-offset parsing,             PresentSink (foreground pid only)
   100 ms buckets, per-driver stats)                    │
        │  publish once per ETW buffer                  │
        └──────────────► Mutex ◄────────────────────────┘
                           │  drained once per second
                           ▼
 NtQuerySystemInformation ─► Recorder (bb-engine) ◄─ NVML, power, memory, net
   (all processes + all cores in 2 syscalls)
                           │
          ┌────────────────┼─────────────────────────────┐
          ▼                ▼                             ▼
   KernelRing (2 min,   SecondSample ring (3 min)   FrameTracker (foreground game)
   100 ms buckets)      ProcEvent ring (512)        rolling median, hitch rule
          └────────────────┼─────────────────────────────┘
                           ▼
              Detectors ──► Pending incidents ──► analyze() once data is complete
                                                     │
                       Store (index + one file per incident) ◄┘
                                                     │
                        EngineSink ──► Tauri events / native overlay / tray
```

## Collectors (crates/blackbox-sys)

| Source | How | Cost |
| --- | --- | --- |
| DPC / ISR per driver | ETW system logger session, `EVENT_TRACE_FLAG_DPC \| INTERRUPT`; duration = event time − `InitialTime` (raw QPC); routine → driver via one `SystemModuleInformation` query (refreshed every 30 s) | one binary search per event |
| Disk latency | ETW `DiskIo` read/write completions, `HighResResponseTime` | fixed-offset read |
| Hard page faults | ETW `PageFault/HardFault` | counter |
| Process start/stop | ETW `Process` events; image name parsed past the TOKEN_USER/SID | rare |
| Frames | ETW user providers DXGI (event 42) and D3D9 (event 1) with an **event-id filter in the provider**, foreground process only | a few hundred events/s |
| Processes | `NtQuerySystemInformation(SystemProcessInformation)`: CPU, I/O, memory, hard faults of every process in one call | 1 syscall/s |
| Cores | `SystemProcessorPerformanceInformation`: busy, kernel, DPC and interrupt time | 1 syscall/s |
| CPU throttling | `CallNtPowerInformation(ProcessorInformation)` `MhzLimit / MaxMhz` | 1 call/s |
| GPU | NVML loaded dynamically (NVIDIA only in v0.1) | every second during a game or while the dashboard is open, every 5 s otherwise |
| Foreground | `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)`, out of context | event-driven |
| Memory and commit | `GlobalMemoryStatusEx` | 1 call/s |
| Network | `GetIfTable2`, physical interfaces only | only while the dashboard is open |
| Power plan, battery | `PowerGetActiveScheme`, `GetSystemPowerStatus` | every 30 s |

ETW sessions use QPC timestamps (`ClientContext = 1`) consumed raw
(`PROCESS_TRACE_MODE_RAW_TIMESTAMP`), 64 KB buffers and a 1 s flush. Consumers aggregate in
thread-local state and publish once per ETW buffer, so the shared lock is taken a few times per
second, not once per event.

Without administrator rights, Windows refuses ETW sessions: Blackbox then runs in **limited
mode** (snapshots, hang detection, throttling and memory pressure; no per-driver latency, no
frame times).

## Detection (crates/blackbox-core + crates/blackbox-engine)

- **Hitch / freeze**: the foreground app becomes "a game" once it presents ≥ 30 frames/s for
  3 seconds in a row (editors and terminals that render in bursts never qualify). Each frame is
  compared to the rolling median of the last 64 normal frames:
  `long if frame ≥ max(median × 2.5, median + 20 ms, 25 ms)` (normal sensitivity). A hitch is a
  *spike*: a long frame (or a short burst) after ≥ 30 steady frames, **confirmed by 3 normal
  frames after it**; the worst frame of the burst is reported. 8 long frames in a row are a new
  normal (30 fps menu, heavy scene), not hitches. ≥ 500 ms is a freeze. Nothing is judged during
  the first 90 frames / 3 s, for 1 s after the game regains focus, or across gaps > 3 s. Presents
  stay buffered 250 ms so late ETW buffers are put back in order. One incident per 5 s at most,
  unless a worse one comes.
- **Driver latency**: any single DPC or ISR ≥ 2 ms (1 ms high sensitivity, 4 ms low), one
  incident per driver per 2 minutes, and none when a hitch at the same moment already blames it.
- **Hang**: the foreground window is reported hung by Windows (`IsHungAppWindow`).
- **Throttling**: CPU frequency limit < 90 % under load, or a GPU thermal/slowdown reason,
  sustained 10 s.
- **Pressure**: memory load ≥ 95 % or average disk latency ≥ 100 ms, sustained 10 s.

Incidents wait until the kernel data covering their whole timeline has arrived (ETW flushes
every second), then `analyze()` runs once. Timelines are aligned on the 100 ms kernel buckets.

Pausing stops both ETW sessions (zero cost). Resuming takes fresh baselines so the first second
isn't polluted by counters accumulated during the pause. If a session is stopped from outside,
the engine notices within 5 s and restarts tracing.

## Play sessions (Games tab)

A session lasts while the game runs: alt-tabs don't split it (the time away is excluded), and it
ends after 5 minutes out of the game, when the game exits, or when Blackbox pauses or quits.
Sessions shorter than 60 s of play are not kept. Statistics use fixed memory however long you
play: frame times go into a histogram (0.1 ms bins up to 50 ms, 5 ms up to 1 s) for the
percentiles and the 1 % / 0.1 % lows, and the once-per-second readings (CPU, game CPU and RAM,
frequency limit, GPU load/temperature/power/clock/fan/VRAM/limits, worst DPC/ISR and disk
latency, fullscreen) into running sums and extremes. `games.json` keeps 20 sessions per game
(50 games), written once per finished session. "Last 3" is a duration-weighted average: maxima
stay maxima, counts are summed, rates are recomputed from the sums.

## Analysis

`analyze()` looks at the *focus window* (the long frame itself, slightly widened) against a 20 s
baseline, and scores every rule with an explicit confidence:

| Rule | Looks at | Cause ids |
| --- | --- | --- |
| Driver latency | worst DPC/ISR vs how much longer the frame took | `drv.*` by driver category |
| Disk | latency and throughput vs baseline, top I/O process | `io.background_disk`, `io.game_streaming`, `proc.*` |
| Memory | hard faults vs baseline, available memory | `mem.paging` |
| Process starts | processes started in the 6 s before, parent = Task Scheduler | `proc.*`, `proc.scheduled_task`, `proc.new_process` |
| Background CPU | non-game processes ≥ 8 % of the machine | `proc.*`, `proc.background_cpu` |
| CPU | saturation, frequency limit | `cpu.saturated`, `cpu.throttled` |
| GPU | NVML throttle reasons, VRAM | `gpu.thermal`, `gpu.power_limit`, `gpu.vram_full` |
| Nothing external | everything above came back clean | `game.internal` |

What each rule saw becomes `evidence`; what came back clean becomes `ruledOut`. The texts
(cause → consequence → fixes, EN/FR) live in `crates/blackbox-core/kb/signatures.json`, the
single source used by the UI and the native overlay. A unit test keeps that file and the
analyzer's cause ids in sync and checks that every French text has the same placeholders as the
English one.

## Storage

`%LOCALAPPDATA%\Blackbox\`:

- `settings.json`
- `games.json`: play sessions per game
- `incidents\index.json`: summaries, loaded at start
- `incidents\<id>.json`: one incident with its 6.5 s timeline, loaded on demand
- `blackbox.log`: reset past 1 MB
- `exports\`: incidents exported from the UI

Nothing is written while nothing happens. Writes use a temp file and a rename. Nothing leaves
the machine.

## IPC

The contract is `src/lib/contract.ts`, mirrored by `crates/blackbox-core/src/model.rs`.
Events: `bb:status` (1 Hz, only while the dashboard is visible), `bb:incident`, `bb:toast`,
`bb:settings`, `bb:session` (a play session was saved). Commands: see `Commands` in the contract and `src-tauri/src/main.rs`.

## Notifications

No Windows toasts. Three channels decided by the engine:

- **in-app** toasts and the notification center, when the dashboard is open;
- **native overlay**: a 372×86 GDI window drawn by Blackbox (no WebView), never takes focus,
  rounded by DWM, auto-hides after 6 s, click opens the dashboard;
- **smart** mode (default) holds overlay notifications while a fullscreen game has focus and
  shows one summary when the session ends.

## Tests

- `cargo test --workspace`: core rules (hitch detector, analysis scenarios, rings, KB
  consistency, health), store, Windows collectors (snapshots, memory, power, NVML when present,
  module map, ETW payload parsers).
- `cargo run --release -p blackbox-engine --example probe -- 30`: headless engine run printing
  status lines, incidents and the driver table; run it from an elevated terminal for full mode.
- `npm run build`: type-check and production bundle of the UI; `npm run dev` runs the UI in a
  browser with a realistic simulator (`src/lib/mock.ts`).
