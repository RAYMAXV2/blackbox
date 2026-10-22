//! Knowledge base: cause signatures (cause -> consequence -> fix) plus what we know about
//! common drivers and background processes.

use crate::model::{DriverCategory, KnowledgeEntry, Lang, ParamValue, Params, ProcTag};
use std::collections::HashMap;

const SIGNATURES_JSON: &str = include_str!("../kb/signatures.json");

/// Every cause id the analyzer can emit. Kept in sync with `kb/signatures.json` by a test.
pub const CAUSE_IDS: &[&str] = &[
    "drv.network",
    "drv.gpu",
    "drv.storage",
    "drv.acpi",
    "drv.usb",
    "drv.audio",
    "drv.other",
    "io.background_disk",
    "io.game_streaming",
    "mem.paging",
    "proc.defender",
    "proc.windows_update",
    "proc.telemetry",
    "proc.search_indexer",
    "proc.shader_precache",
    "proc.cloud_sync",
    "proc.wmi",
    "proc.scheduled_task",
    "proc.background_cpu",
    "proc.new_process",
    "cpu.saturated",
    "cpu.throttled",
    "gpu.power_limit",
    "gpu.thermal",
    "gpu.vram_full",
    "app.not_responding",
    "game.internal",
];

#[derive(Debug)]
pub struct Kb {
    entries: Vec<KnowledgeEntry>,
    index: HashMap<String, usize>,
}

impl Kb {
    /// Parses the embedded signature file. The file is validated by unit tests, so a failure
    /// here is a build defect, not a runtime condition.
    pub fn load() -> Kb {
        let entries: Vec<KnowledgeEntry> =
            serde_json::from_str(SIGNATURES_JSON).expect("kb/signatures.json is valid (checked by tests)");
        let index = entries.iter().enumerate().map(|(i, e)| (e.id.clone(), i)).collect();
        Kb { entries, index }
    }

    pub fn entries(&self) -> &[KnowledgeEntry] {
        &self.entries
    }

    pub fn get(&self, id: &str) -> Option<&KnowledgeEntry> {
        self.index.get(id).map(|&i| &self.entries[i])
    }

    /// Localized, filled-in title of a cause. Falls back to the id for unknown causes.
    pub fn title(&self, id: &str, lang: Lang, params: &Params) -> String {
        match self.get(id) {
            Some(e) => fill(e.title.get(lang), params),
            None => id.to_owned(),
        }
    }
}

/// Replaces `{name}` placeholders with values from `params`. Unknown placeholders are kept.
pub fn fill(template: &str, params: &Params) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let key = &after[..close];
                match params.get(key) {
                    Some(ParamValue::Text(s)) => out.push_str(s),
                    Some(ParamValue::Num(n)) => out.push_str(&format_num(*n)),
                    None => {
                        out.push('{');
                        out.push_str(key);
                        out.push('}');
                    }
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push_str(&rest[open..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn format_num(n: f64) -> String {
    if (n - n.round()).abs() < 1e-9 {
        format!("{}", n.round() as i64)
    } else {
        format!("{n:.1}")
    }
}

/// Human name and category of a kernel module, from its file name (any case).
pub fn driver_info(file_name: &str) -> (Option<&'static str>, DriverCategory) {
    let n = file_name.to_ascii_lowercase();
    if let Some(&(_, friendly, cat)) = EXACT_DRIVERS.iter().find(|(name, _, _)| *name == n) {
        return (Some(friendly), cat);
    }
    if let Some(&(_, friendly, cat)) = PREFIX_DRIVERS.iter().find(|(prefix, _, _)| n.starts_with(prefix)) {
        return (Some(friendly), cat);
    }
    // Vendor drivers we don't know by name usually say what they are.
    let cat = KEYWORD_DRIVERS.iter().find(|(kw, _)| n.contains(kw)).map_or(DriverCategory::Other, |&(_, c)| c);
    (None, cat)
}

const KEYWORD_DRIVERS: &[(&str, DriverCategory)] = &[
    ("audio", D::Audio),
    ("wlan", D::Network),
    ("wifi", D::Network),
    ("ethernet", D::Network),
    ("nvme", D::Storage),
    ("sata", D::Storage),
    ("raid", D::Storage),
    ("usb", D::Usb),
    ("hid", D::Input),
];

use DriverCategory as D;

const EXACT_DRIVERS: &[(&str, &str, DriverCategory)] = &[
    // graphics
    ("nvlddmkm.sys", "NVIDIA graphics driver", D::Gpu),
    ("amdkmdag.sys", "AMD graphics driver", D::Gpu),
    ("atikmdag.sys", "AMD graphics driver", D::Gpu),
    ("atikmpag.sys", "AMD graphics miniport", D::Gpu),
    ("dxgkrnl.sys", "DirectX graphics kernel", D::Gpu),
    ("dxgmms2.sys", "DirectX GPU scheduler", D::Gpu),
    ("basicdisplay.sys", "Microsoft basic display", D::Gpu),
    ("basicrender.sys", "Microsoft basic render", D::Gpu),
    ("watchdog.sys", "Display watchdog", D::Gpu),
    // network
    ("ndis.sys", "Network driver interface (NDIS)", D::Network),
    ("tcpip.sys", "TCP/IP stack", D::Network),
    ("netio.sys", "Network I/O subsystem", D::Network),
    ("afd.sys", "Winsock (AFD)", D::Network),
    ("nwifi.sys", "Native Wi-Fi", D::Network),
    ("vwifibus.sys", "Virtual Wi-Fi bus", D::Network),
    ("wfplwfs.sys", "Windows Filtering Platform", D::Network),
    ("pacer.sys", "QoS packet scheduler", D::Network),
    ("netbt.sys", "NetBIOS over TCP/IP", D::Network),
    ("mslldp.sys", "LLDP protocol", D::Network),
    ("vmswitch.sys", "Hyper-V virtual switch", D::Network),
    ("wintun.sys", "WireGuard/Wintun VPN adapter", D::Network),
    ("ndisuio.sys", "NDIS user-mode I/O", D::Network),
    // storage
    ("storport.sys", "Storage port driver", D::Storage),
    ("stornvme.sys", "Microsoft NVMe driver", D::Storage),
    ("storahci.sys", "Microsoft SATA AHCI driver", D::Storage),
    ("secnvme.sys", "Samsung NVMe driver", D::Storage),
    ("disk.sys", "Disk class driver", D::Storage),
    ("classpnp.sys", "Storage class driver", D::Storage),
    ("partmgr.sys", "Partition manager", D::Storage),
    ("volmgr.sys", "Volume manager", D::Storage),
    ("volmgrx.sys", "Dynamic volume manager", D::Storage),
    ("spaceport.sys", "Storage Spaces", D::Storage),
    ("ataport.sys", "ATA port driver", D::Storage),
    ("amdsata.sys", "AMD SATA driver", D::Storage),
    ("amdxata.sys", "AMD SATA filter", D::Storage),
    ("rcraid.sys", "AMD RAID driver", D::Storage),
    ("fvevol.sys", "BitLocker drive encryption", D::Storage),
    ("volsnap.sys", "Volume shadow copy", D::Storage),
    // file systems and filters
    ("ntfs.sys", "NTFS file system", D::Filesystem),
    ("refs.sys", "ReFS file system", D::Filesystem),
    ("fltmgr.sys", "File system filter manager", D::Filesystem),
    ("cldflt.sys", "Cloud files filter (OneDrive)", D::Filesystem),
    ("wof.sys", "Windows overlay filter", D::Filesystem),
    ("bindflt.sys", "Bind filter", D::Filesystem),
    ("wcifs.sys", "Container isolation filter", D::Filesystem),
    ("mup.sys", "Multiple UNC provider", D::Filesystem),
    ("exfat.sys", "exFAT file system", D::Filesystem),
    ("fastfat.sys", "FAT file system", D::Filesystem),
    // security
    ("wdfilter.sys", "Microsoft Defender file filter", D::Security),
    ("wdnisdrv.sys", "Microsoft Defender network inspection", D::Security),
    ("wdboot.sys", "Microsoft Defender boot driver", D::Security),
    ("mpsdrv.sys", "Windows Firewall", D::Security),
    ("ksecdd.sys", "Kernel security support", D::Security),
    ("cng.sys", "Kernel cryptography", D::Security),
    ("vgk.sys", "Riot Vanguard anti-cheat", D::Security),
    ("easyanticheat.sys", "Easy Anti-Cheat", D::Security),
    ("easyanticheat_eos.sys", "Easy Anti-Cheat (EOS)", D::Security),
    ("bedaisy.sys", "BattlEye anti-cheat", D::Security),
    ("faceit.sys", "FACEIT anti-cheat", D::Security),
    // usb
    ("usbxhci.sys", "USB 3 host controller", D::Usb),
    ("usbport.sys", "USB host controller port", D::Usb),
    ("usbehci.sys", "USB 2 host controller", D::Usb),
    ("usbhub.sys", "USB hub", D::Usb),
    ("usbhub3.sys", "USB 3 hub", D::Usb),
    ("usbccgp.sys", "USB composite device", D::Usb),
    ("ucx01000.sys", "USB host controller extension", D::Usb),
    ("usbstor.sys", "USB mass storage", D::Usb),
    ("uaspstor.sys", "USB attached SCSI storage", D::Usb),
    // audio
    ("hdaudbus.sys", "HD Audio bus", D::Audio),
    ("hdaudio.sys", "HD Audio class driver", D::Audio),
    ("portcls.sys", "Audio port class", D::Audio),
    ("ks.sys", "Kernel streaming", D::Audio),
    ("ksthunk.sys", "Kernel streaming thunk", D::Audio),
    ("rtkvhd64.sys", "Realtek audio driver", D::Audio),
    ("nvhda64v.sys", "NVIDIA HDMI audio", D::Audio),
    ("atihdwt6.sys", "AMD HDMI audio", D::Audio),
    ("usbaudio.sys", "USB audio", D::Audio),
    ("usbaudio2.sys", "USB audio 2.0", D::Audio),
    ("drmk.sys", "Kernel audio DRM", D::Audio),
    // power / ACPI
    ("acpi.sys", "ACPI power management", D::Acpi),
    ("acpiex.sys", "ACPI extensions", D::Acpi),
    ("cmbatt.sys", "ACPI battery", D::Acpi),
    ("intelppm.sys", "Intel processor power management", D::Acpi),
    ("amdppm.sys", "AMD processor power management", D::Acpi),
    ("processr.sys", "Processor power management", D::Acpi),
    ("intelpep.sys", "Intel power engine", D::Acpi),
    ("amdpep.sys", "AMD power engine", D::Acpi),
    ("pdc.sys", "Power dependency coordinator", D::Acpi),
    // input
    ("kbdclass.sys", "Keyboard class", D::Input),
    ("mouclass.sys", "Mouse class", D::Input),
    ("hidclass.sys", "HID class", D::Input),
    ("hidusb.sys", "USB HID", D::Input),
    ("kbdhid.sys", "HID keyboard", D::Input),
    ("mouhid.sys", "HID mouse", D::Input),
    ("i8042prt.sys", "PS/2 keyboard and mouse", D::Input),
    ("xboxgip.sys", "Xbox controller", D::Input),
    // kernel
    ("ntoskrnl.exe", "Windows kernel", D::Kernel),
    ("hal.dll", "Hardware abstraction layer", D::Kernel),
    ("wdf01000.sys", "Kernel driver framework (KMDF)", D::Kernel),
    ("wpprecorder.sys", "WPP trace recorder", D::Kernel),
    ("pci.sys", "PCI bus driver", D::Kernel),
    ("win32k.sys", "Window manager (kernel)", D::Kernel),
    ("win32kbase.sys", "Window manager (kernel)", D::Kernel),
    ("win32kfull.sys", "Window manager (kernel)", D::Kernel),
    ("clfs.sys", "Common log file system", D::Kernel),
    ("msrpc.sys", "Kernel RPC", D::Kernel),
    ("vmbus.sys", "Hyper-V VMBus", D::Kernel),
    ("hvsocket.sys", "Hyper-V sockets", D::Kernel),
    ("cimfs.sys", "Composite image file system", D::Kernel),
];

const PREFIX_DRIVERS: &[(&str, &str, DriverCategory)] = &[
    ("igdkmd", "Intel graphics driver", D::Gpu),
    ("netwtw", "Intel Wi-Fi driver", D::Network),
    ("netwbw", "Intel Wi-Fi driver", D::Network),
    ("netwns", "Intel Wi-Fi driver", D::Network),
    ("rt640", "Realtek Ethernet driver", D::Network),
    ("rt68", "Realtek Ethernet driver", D::Network),
    ("rtcx", "Realtek Ethernet driver", D::Network),
    ("rtux", "Realtek USB Ethernet driver", D::Network),
    ("rtwlan", "Realtek Wi-Fi driver", D::Network),
    ("e1d", "Intel Ethernet driver", D::Network),
    ("e1i", "Intel Ethernet driver", D::Network),
    ("e1r", "Intel Ethernet driver", D::Network),
    ("e2f", "Intel Ethernet driver", D::Network),
    ("e2xw", "Killer Ethernet driver", D::Network),
    ("athw", "Qualcomm Atheros Wi-Fi driver", D::Network),
    ("qcamain", "Qualcomm Wi-Fi driver", D::Network),
    ("mtkwl", "MediaTek Wi-Fi driver", D::Network),
    ("mtkwecx", "MediaTek Wi-Fi driver", D::Network),
    ("bcmwl", "Broadcom Wi-Fi driver", D::Network),
    ("mrvl", "Marvell network driver", D::Network),
    ("aqnic", "Marvell AQtion Ethernet driver", D::Network),
    ("tap0901", "OpenVPN TAP adapter", D::Network),
    ("iastor", "Intel Rapid Storage driver", D::Storage),
    ("nvstor", "NVIDIA storage driver", D::Storage),
    ("amdpsp", "AMD security processor", D::Security),
    ("rtkvhd", "Realtek audio driver", D::Audio),
    ("intcaud", "Intel audio driver", D::Audio),
    ("intcdaud", "Intel display audio", D::Audio),
    ("intcsst", "Intel smart sound", D::Audio),
    ("amdacpbus", "AMD audio co-processor", D::Audio),
    ("amdi2c", "AMD I2C controller", D::Input),
    ("ialpss", "Intel serial I/O", D::Input),
    ("amdgpio", "AMD GPIO controller", D::Input),
];

/// Known background processes worth naming in a verdict. Name compare is case-insensitive.
pub fn process_tag(exe_name: &str) -> Option<ProcTag> {
    let n = exe_name.to_ascii_lowercase();
    PROCESS_TAGS.iter().find(|(name, _)| *name == n).map(|&(_, tag)| tag)
}

const PROCESS_TAGS: &[(&str, ProcTag)] = &[
    ("msmpeng.exe", ProcTag::Defender),
    ("mpcmdrun.exe", ProcTag::Defender),
    ("nissrv.exe", ProcTag::Defender),
    ("mpdefendercoreservice.exe", ProcTag::Defender),
    ("tiworker.exe", ProcTag::WindowsUpdate),
    ("trustedinstaller.exe", ProcTag::WindowsUpdate),
    ("mousocoreworker.exe", ProcTag::WindowsUpdate),
    ("usocoreworker.exe", ProcTag::WindowsUpdate),
    ("usoclient.exe", ProcTag::WindowsUpdate),
    ("wuauclt.exe", ProcTag::WindowsUpdate),
    ("dismhost.exe", ProcTag::WindowsUpdate),
    ("sihclient.exe", ProcTag::WindowsUpdate),
    ("waasmedicagent.exe", ProcTag::WindowsUpdate),
    ("compattelrunner.exe", ProcTag::Telemetry),
    ("devicecensus.exe", ProcTag::Telemetry),
    ("wsqmcons.exe", ProcTag::Telemetry),
    ("searchindexer.exe", ProcTag::SearchIndexer),
    ("searchprotocolhost.exe", ProcTag::SearchIndexer),
    ("searchfilterhost.exe", ProcTag::SearchIndexer),
    ("fossilize_replay.exe", ProcTag::ShaderPrecache),
    ("onedrive.exe", ProcTag::CloudSync),
    ("onedrive.sync.service.exe", ProcTag::CloudSync),
    ("dropbox.exe", ProcTag::CloudSync),
    ("googledrivefs.exe", ProcTag::CloudSync),
    ("iclouddrive.exe", ProcTag::CloudSync),
    ("megasync.exe", ProcTag::CloudSync),
    ("nextcloud.exe", ProcTag::CloudSync),
    ("wmiprvse.exe", ProcTag::Wmi),
    ("blackbox.exe", ProcTag::Blackbox),
];

/// Processes that are part of the OS core and never a "background app" verdict on their own.
pub fn is_system_pseudo_process(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "idle" | "system" | "registry" | "secure system" | "memcompression" | "interrupts" | "vmmem" | "vmmemws"
    )
}

/// Parents that mean "launched by the Task Scheduler".
pub fn is_scheduler_host(name: &str) -> bool {
    name.eq_ignore_ascii_case("taskhostw.exe") || name.eq_ignore_ascii_case("taskeng.exe")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params;
    use std::collections::BTreeSet;

    fn placeholders(s: &str) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut rest = s;
        while let Some(o) = rest.find('{') {
            let after = &rest[o + 1..];
            if let Some(c) = after.find('}') {
                out.insert(after[..c].to_string());
                rest = &after[c + 1..];
            } else {
                break;
            }
        }
        out
    }

    #[test]
    fn signatures_parse_and_match_cause_ids() {
        let kb = Kb::load();
        let json_ids: BTreeSet<_> = kb.entries().iter().map(|e| e.id.as_str()).collect();
        let code_ids: BTreeSet<_> = CAUSE_IDS.iter().copied().collect();
        assert_eq!(json_ids, code_ids, "kb/signatures.json and CAUSE_IDS disagree");
        assert_eq!(kb.entries().len(), CAUSE_IDS.len(), "duplicate ids in signatures.json");
    }

    #[test]
    fn translations_are_complete_and_consistent() {
        for e in Kb::load().entries() {
            for (label, l) in [("title", &e.title), ("explain", &e.explain)] {
                assert!(!l.en.trim().is_empty() && !l.fr.trim().is_empty(), "{} {label} empty", e.id);
                assert_eq!(placeholders(&l.en), placeholders(&l.fr), "{} {label} placeholders differ", e.id);
            }
            assert!(!e.fixes.is_empty(), "{} has no fix", e.id);
            for f in &e.fixes {
                assert_eq!(placeholders(&f.en), placeholders(&f.fr), "{} fix placeholders differ", e.id);
            }
            assert!(!e.consequences.is_empty(), "{} has no consequence", e.id);
        }
    }

    #[test]
    fn fill_replaces_known_and_keeps_unknown() {
        let p = params! {"driver" => "ndis.sys", "us" => 1250u32, "x" => 1.26f64};
        assert_eq!(fill("{driver} ran {us} µs ({x}) {missing}", &p), "ndis.sys ran 1250 µs (1.3) {missing}");
        assert_eq!(fill("no braces", &p), "no braces");
        assert_eq!(fill("dangling {brace", &p), "dangling {brace");
    }

    #[test]
    fn driver_and_process_lookup() {
        assert_eq!(driver_info("NVLDDMKM.SYS").1, DriverCategory::Gpu);
        assert_eq!(driver_info("Netwtw14.sys").1, DriverCategory::Network);
        assert_eq!(driver_info("unknown_thing.sys"), (None, DriverCategory::Other));
        assert_eq!(driver_info("ElgatoUsbAudio.sys"), (None, DriverCategory::Audio));
        assert_eq!(driver_info("iFiUsbAudio.sys"), (None, DriverCategory::Audio));
        assert_eq!(process_tag("CompatTelRunner.exe"), Some(ProcTag::Telemetry));
        assert_eq!(process_tag("game.exe"), None);
    }
}
