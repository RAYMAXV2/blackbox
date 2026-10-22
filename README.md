# Blackbox

**A flight recorder for Windows performance.** Blackbox quietly records what your kernel,
drivers, disks, CPU, GPU and apps are doing, catches every stutter, freeze, driver latency spike
and hang, and tells you *why* it happened — with the evidence, what it ruled out, and how to fix
it. In English and French.

> v0.1 · Windows 10/11 x64 · Rust + Tauri 2 + Vue 3

## What it does

- **Live view, from the kernel up to your apps**: DPC/ISR rates and worst latencies, timer
  resolution, hard page faults → the drivers doing the most work right now → CPU (per core,
  frequency limit), memory and commit, GPU (load, temperature, power, clocks, VRAM, throttle
  reasons), disks (throughput, latency), network → the processes using CPU and I/O.
- **Game frame pacing**: FPS, frame-time graph, p99 and 1 % lows of the foreground game, read from
  ETW (no injection, no overlay hook: anti-cheat safe).
- **Per-game history**: every play session is saved per game (FPS, 1 % and 0.1 % lows,
  frame-time percentiles, hitches per hour, GPU load/temperature/power/clocks/VRAM, CPU and RAM,
  driver and disk latency, top causes), with the average of the last 3 sessions and the trend.
- **Incidents, explained**: stutters, freezes, driver latency spikes (audio crackle, input lag),
  apps not responding, sustained throttling, memory/disk pressure. Each incident has a verdict
  with a confidence, the evidence, the checks that came back clean, the fixes, and a synced
  6.5 s timeline (frames, DPC/ISR, disk, hard faults, CPU, GPU, process starts).
- **Cause → consequence → fix knowledge base**: 27 signatures (network/GPU/storage/USB/audio/ACPI
  drivers, background disk hogs, paging, Defender, Windows Update, telemetry tasks, indexing,
  Steam shader pre-caching, cloud sync, WMI polling, scheduled tasks, CPU saturation and
  throttling, GPU power/thermal limits, full VRAM, apps not responding, game-internal hitches).
- **Notifications without Windows toasts**: in-app toasts and a notification center, plus a tiny
  native overlay drawn by Blackbox itself that never steals focus. "Smart" mode holds alerts while
  you play fullscreen and sums the session up when you stop.
- **System checks**: elevation, tracing status, power plan, battery, Game Mode, HAGS, memory
  integrity, timer resolution, NVML.

## Built to be light

The recorder is designed to stay out of your games' way:

- ETW events are parsed at fixed offsets (no schema lookups), aggregated on the consumer
  thread, and published **once per ETW buffer**, not once per event.
- Frame events are filtered **inside the provider** (only `Present_Start`), and only the
  foreground app's frames are kept.
- All processes and all cores are read with **two syscalls per second**; GPU data comes from
  NVML; foreground changes are event-driven.
- Fixed-size in-memory rings: memory use doesn't grow with uptime.
- The engine thread runs at background priority with Windows 11 **EcoQoS**; ETW consumers sleep
  between buffers. GPU polling slows down to every 5 s when no game runs and the dashboard is
  closed; network counters are only read while the dashboard is open.
- **Nothing is written to disk** unless an incident happens.
- The dashboard is a WebView that only exists while it's open; closing it frees it entirely.
  While it's hidden, the engine doesn't even build the live status.
- The dashboard shows what Blackbox itself costs (CPU, memory, events/s, lost events).

Measured on a Ryzen 7 9800X3D + RTX 5090 (Windows 11 25H2), release build:

| Mode | CPU (whole machine) | CPU (one core) | Memory | ETW events |
| --- | --- | --- | --- | --- |
| Full tracing, 46.8 s, desktop use | 0.044 % | 0.70 % | 35.5 MB peak working set | 15–27 k/s, 0 lost |
| Full tracing, Overwatch at 360 fps | DPC/ISR + frames recorded | – | ~36 MB | ~35 k/s, 0 lost |
| Limited mode, tray only, 30 s | 0.013 % | 0.21 % | 25 MB private | – |

With the dashboard open, the WebView2 processes come on top and disappear when it's closed.

## Run it

Full tracing (per-driver latency, disk latency, frame times) needs administrator rights: Windows
only lets elevated processes open kernel ETW sessions. Without them, Blackbox runs in **limited
mode** (process/CPU/memory/GPU snapshots, hangs, throttling, pressure) and offers a
*Restart as admin* button. *Start with Windows* registers an elevated logon task so there is no
UAC prompt at each boot.

The dashboard closes to the tray; the recorder keeps running. Quit from the tray menu.

## Build

Requirements: Rust (stable, MSVC toolchain), Visual Studio Build Tools with C++, Node.js 20+,
WebView2 (preinstalled on Windows 11).

```bash
npm install
```

```bash
npx tauri dev
```

```bash
npx tauri build
```

The release build produces `target/release/blackbox.exe` and an NSIS installer under
`target/release/bundle/nsis/`. The installer is per-machine (Program Files): *Start with Windows*
runs Blackbox elevated, so its executable must not live in a folder any program can write to.

Other useful commands:

```bash
cargo test --workspace
```

```bash
cargo run --release -p blackbox-engine --example probe -- 30
```

```bash
npm run dev
```

`npm run dev` runs the dashboard in a normal browser on a realistic simulator (no Rust needed).

## Privacy

Everything stays on your PC, in `%LOCALAPPDATA%\Blackbox`: settings, incidents (one small JSON
file each), a log capped at 1 MB. No network calls, no telemetry.

## Project layout

```
crates/blackbox-core     model shared with the UI, time series, hitch detector, cause analysis, knowledge base
crates/blackbox-sys      Windows collectors: ETW sessions, NT snapshots, NVML, power, foreground hook
crates/blackbox-engine   the 1 Hz recorder thread, detectors, incident store, probe example
src-tauri                desktop app: commands, tray, native overlay notifications, autostart
src                      Vue 3 dashboard (EN/FR, dark/light), mock backend for browser development
docs/ARCHITECTURE.md     how it works, thresholds, analysis rules
```

To add a cause: add an entry to `crates/blackbox-core/kb/signatures.json` (EN + FR texts, same
`{placeholders}`), its id to `CAUSE_IDS` in `kb.rs`, and a rule in `analyze.rs`. Unit tests check
that the three stay in sync.

## Known limitations (v0.1)

- GPU telemetry is NVIDIA-only (NVML); AMD/Intel GPUs show no GPU card yet.
- Frame times come from DXGI (D3D10/11/12) and D3D9 presents. Vulkan and OpenGL games are only
  measured when the driver presents them through DXGI.
- No CPU temperature (needs a kernel driver); CPU throttling is read from Windows' frequency limit.
- If Blackbox is killed, its two ETW sessions keep running until the next start (which cleans
  them up) or a reboot.
- Tested on one machine so far (Windows 11 25H2, Ryzen 7 9800X3D, RTX 5090).

## Roadmap

- AMD and Intel GPU telemetry (D3DKMT / ADLX), CPU temperatures (PawnIO driver)
- Vulkan/OpenGL frame times via DxgKrnl present history
- "Deep mode" on demand: context switches and stacks for a few seconds around an incident
- Shareable incident reports, community signatures, comparisons between similar PCs
- Boot and shutdown analysis

## License

[MIT](LICENSE).

## En français

Blackbox est une boîte noire pour les performances de Windows : il enregistre en continu, pour
presque rien, ce que font le noyau, les pilotes, les disques, le CPU, le GPU et les applis. Il
détecte chaque saccade, gel, pic de latence pilote ou appli figée, puis explique la cause avec les
preuves, ce qui a été écarté et les solutions. L'interface est en anglais ou en français
(Paramètres → Langue). Le mode complet demande les droits administrateur ; sans eux, Blackbox
fonctionne en mode limité.
