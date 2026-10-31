# PawnIO modules

Two unmodified modules from [PawnIO.Modules](https://github.com/namazso/PawnIO.Modules)
release **0.2.11**, by namazso, licensed under the **GNU LGPL 2.1 or later** (full text in
[`COPYING`](COPYING); sources: `AMDFamily17.p` and `IntelMSR.p` in that repository).

| File | Used for |
| --- | --- |
| `AMDFamily17.bin` | AMD family 17h–1Ah (Zen 1–5): reads the THM_TCON_CUR_TMP SMN register (CPU Tctl) |
| `IntelMSR.bin` | Intel: reads IA32_TEMPERATURE_TARGET and IA32_PACKAGE_THERM_STATUS (package temperature) |

They are loaded at runtime into the [PawnIO](https://pawnio.eu) driver, which Blackbox does not
ship: it must already be installed (FanControl and LibreHardwareMonitor install it). Blackbox only
calls the read functions; the modules themselves restrict which registers can be accessed.

To update: download the new release zip from the repository above, replace the two `.bin` files
and `COPYING`, and update the version here.
