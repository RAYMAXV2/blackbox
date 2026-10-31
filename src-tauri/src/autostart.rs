//! "Start with Windows": a logon scheduled task that runs Blackbox elevated without a UAC
//! prompt. Defined from XML so it also runs on battery and never times out (schtasks'
//! command-line defaults would stop it after 72 h).

use std::os::windows::process::CommandExt;
use std::process::Command;

const TASK: &str = "Blackbox";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn task_xml(exe: &str, user: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>Blackbox performance recorder</Description></RegistrationInfo>
  <Triggers>
    <LogonTrigger><Enabled>true</Enabled><UserId>{user}</UserId><Delay>PT15S</Delay></LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>HighestAvailable</RunLevel></Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings><StopOnIdleEnd>false</StopOnIdleEnd><RestartOnIdle>false</RestartOnIdle></IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>5</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec><Command>{exe}</Command><Arguments>--background</Arguments></Exec>
  </Actions>
</Task>
"#,
        user = xml_escape(user),
        exe = xml_escape(exe)
    )
}

fn schtasks(args: &[&str]) -> Result<(), String> {
    let out =
        Command::new("schtasks.exe").args(args).creation_flags(CREATE_NO_WINDOW).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

pub fn set(enabled: bool) -> Result<(), String> {
    // The task runs with the highest privileges: only an elevated Blackbox may change it.
    if !blackbox_sys::util::is_elevated() {
        return if !enabled && !is_enabled() { Ok(()) } else { Err("admin".into()) };
    }
    if !enabled {
        return match schtasks(&["/Delete", "/F", "/TN", TASK]) {
            Ok(()) => Ok(()),
            Err(_) if !is_enabled() => Ok(()),
            Err(e) => Err(e),
        };
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?.to_string_lossy().into_owned();
    let user = format!(
        "{}\\{}",
        std::env::var("USERDOMAIN").unwrap_or_default(),
        std::env::var("USERNAME").unwrap_or_default()
    );
    let xml = task_xml(&exe, &user);
    // schtasks expects UTF-16 LE with a BOM for XML definitions.
    let mut bytes = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    let path = std::env::temp_dir().join(format!("blackbox-task-{}.xml", std::process::id()));
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    let result = schtasks(&["/Create", "/F", "/TN", TASK, "/XML", &path.to_string_lossy()]);
    let _ = std::fs::remove_file(&path);
    result?;
    // The XML went through a user-writable temp file: check what was actually registered.
    if !registered_command_is(&exe) {
        let _ = schtasks(&["/Delete", "/F", "/TN", TASK]);
        return Err("task verification failed".into());
    }
    Ok(())
}

/// The registered task runs exactly `exe --background`, elevated, and nothing else.
fn registered_command_is(exe: &str) -> bool {
    let Ok(out) =
        Command::new("schtasks.exe").args(["/Query", "/TN", TASK, "/XML"]).creation_flags(CREATE_NO_WINDOW).output()
    else {
        return false;
    };
    let xml = String::from_utf8_lossy(&out.stdout);
    let one_action = xml.matches("<Exec>").count() == 1;
    out.status.success()
        && one_action
        && xml.contains(&format!("<Command>{}</Command>", xml_escape(exe)))
        && xml.contains("<Arguments>--background</Arguments>")
}

pub fn is_enabled() -> bool {
    schtasks(&["/Query", "/TN", TASK]).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_is_escaped() {
        let x = task_xml(r"C:\A&B\blackbox.exe", r"PC\me");
        assert!(x.contains(r"C:\A&amp;B\blackbox.exe"));
        assert!(x.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
    }
}
