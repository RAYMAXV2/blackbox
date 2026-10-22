//! Single instance: a named mutex, plus a named event a second launch uses to ask the running
//! instance to show its window. Both are created with a DACL that lets the elevated and the
//! non-elevated process of the same user talk to each other.

use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, LocalFree, ERROR_ALREADY_EXISTS, HANDLE};
use windows_sys::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows_sys::Win32::System::Threading::{CreateEventW, CreateMutexW, OpenEventW, SetEvent, WaitForSingleObject, EVENT_MODIFY_STATE, INFINITE};

const MUTEX_NAME: &str = "Local\\Blackbox.Instance";
const SHOW_EVENT: &str = "Local\\Blackbox.Show";
/// Authenticated users and SYSTEM, low integrity label so a medium process can signal.
const SDDL: &str = "D:(A;;GA;;;AU)(A;;GA;;;SY)S:(ML;;NW;;;LW)";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

struct Sa {
    sd: PSECURITY_DESCRIPTOR,
    attrs: SECURITY_ATTRIBUTES,
}

impl Sa {
    fn new() -> Option<Sa> {
        let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(wide(SDDL).as_ptr(), SDDL_REVISION_1, &mut sd, std::ptr::null_mut())
        };
        if ok == 0 {
            return None;
        }
        Some(Sa {
            sd,
            attrs: SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: sd,
                bInheritHandle: 0,
            },
        })
    }
    fn ptr(&self) -> *const SECURITY_ATTRIBUTES {
        &self.attrs
    }
}

impl Drop for Sa {
    fn drop(&mut self) {
        unsafe { LocalFree(self.sd) };
    }
}

pub struct Instance(HANDLE);

unsafe impl Send for Instance {}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

/// Becomes the single instance. With `wait`, keeps trying for a few seconds (used right after
/// an elevated restart, while the previous instance is exiting).
pub fn acquire(wait: bool) -> Option<Instance> {
    let sa = Sa::new();
    let sa_ptr = sa.as_ref().map_or(std::ptr::null(), Sa::ptr);
    let name = wide(MUTEX_NAME);
    let deadline = Instant::now() + Duration::from_secs(if wait { 10 } else { 0 });
    loop {
        let h = unsafe { CreateMutexW(sa_ptr, 0, name.as_ptr()) };
        let exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        if !h.is_null() && !exists {
            return Some(Instance(h));
        }
        if !h.is_null() {
            unsafe { CloseHandle(h) };
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Asks the running instance to show its dashboard.
pub fn signal_show() {
    unsafe {
        let h = OpenEventW(EVENT_MODIFY_STATE, 0, wide(SHOW_EVENT).as_ptr());
        if !h.is_null() {
            SetEvent(h);
            CloseHandle(h);
        }
    }
}

/// Calls `on_show` each time another launch asks for the window. The thread sleeps in the wait.
pub fn listen_show(on_show: impl Fn() + Send + 'static) {
    let sa = Sa::new();
    let sa_ptr = sa.as_ref().map_or(std::ptr::null(), Sa::ptr);
    let h = unsafe { CreateEventW(sa_ptr, 0, 0, wide(SHOW_EVENT).as_ptr()) };
    drop(sa);
    if h.is_null() {
        return;
    }
    let h = h as usize;
    let _ = std::thread::Builder::new().name("bb-instance".into()).spawn(move || loop {
        if unsafe { WaitForSingleObject(h as HANDLE, INFINITE) } != 0 {
            break;
        }
        on_show();
    });
}
