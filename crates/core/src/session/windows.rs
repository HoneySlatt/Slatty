//! The session supervisor on Windows (docs/windows-port.md). The supervisor puts itself in a job
//! object before it starts the game: every process the game starts is then in the job too, a
//! launcher that starts the game and exits included, and the session ends once the supervisor is
//! the job's last process. Windows has no signals: the launcher asks for a stop through an event
//! named after the supervisor. A first stop asks the game's windows to close, a second one ends
//! its processes.

use std::collections::HashSet;
use std::process::Stdio;
use std::time::Duration;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WAIT_OBJECT_0};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
    JobObjectBasicAccountingInformation, JobObjectBasicProcessIdList, QueryInformationJobObject,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, EVENT_MODIFY_STATE, GetCurrentProcess, OpenEventW, OpenProcess,
    PROCESS_TERMINATE, SetEvent, TerminateProcess, WaitForSingleObject,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CLOSE,
};
use windows_sys::core::BOOL;

use super::SupervisorEvent;
use crate::runner::LaunchSpec;

/// How often the game's processes are counted while nothing else happens.
const LOOK_EVERY: Duration = Duration::from_millis(250);

/// A handle closed when dropped.
struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by this program and is closed once.
        unsafe { CloseHandle(self.0) };
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

/// The event a supervisor waits on for a stop, named after its process.
fn stop_event_name(supervisor: u32) -> Vec<u16> {
    wide(&format!("Local\\slatty-stop-{supervisor}"))
}

/// Asks the supervisor `pid` to stop its game.
pub(super) fn request_stop(pid: u32) {
    // SAFETY: the name ends with a nul; the handle is closed by `Owned`.
    unsafe {
        let event = OpenEventW(EVENT_MODIFY_STATE, 0, stop_event_name(pid).as_ptr());
        if !event.is_null() {
            let event = Owned(event);
            SetEvent(event.0);
        }
    }
}

pub(super) fn supervise(
    spec: &LaunchSpec,
    stdout: Stdio,
    stderr: Stdio,
    mut emit: impl FnMut(&SupervisorEvent),
) -> i32 {
    let mut fail = |message: String| {
        emit(&SupervisorEvent::Failed { message });
        2
    };
    // SAFETY: plain calls; each handle is closed by `Owned`.
    let (stop, job) = unsafe {
        let stop = CreateEventW(
            std::ptr::null(),
            0,
            0,
            stop_event_name(std::process::id()).as_ptr(),
        );
        if stop.is_null() {
            return fail(format!("making the stop event: {}", last_error()));
        }
        let stop = Owned(stop);
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return fail(format!("making a job object: {}", last_error()));
        }
        let job = Owned(job);
        if AssignProcessToJobObject(job.0, GetCurrentProcess()) == 0 {
            return fail(format!("joining the job object: {}", last_error()));
        }
        (stop, job)
    };
    let mut child = match std::process::Command::new(&spec.program)
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .envs(spec.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            emit(&SupervisorEvent::Failed {
                message: format!("starting {}: {e}", spec.program.display()),
            });
            return 1;
        }
    };
    emit(&SupervisorEvent::Started { pid: child.id() });
    let mut main_code = None;
    let mut main_running = true;
    let mut stops = 0;
    loop {
        if main_running && let Ok(Some(status)) = child.try_wait() {
            main_running = false;
            main_code = status.code();
            emit(&SupervisorEvent::MainExited { code: main_code });
        }
        if !main_running && active_processes(&job) <= 1 {
            break;
        }
        // SAFETY: the event's handle is open.
        if unsafe { WaitForSingleObject(stop.0, LOOK_EVERY.as_millis() as u32) } == WAIT_OBJECT_0 {
            stops += 1;
            let game = game_processes(&job);
            match stops {
                1 => close_windows(&game),
                _ => end_processes(&game),
            }
        }
    }
    emit(&SupervisorEvent::Ended { main_code });
    0
}

fn last_error() -> std::io::Error {
    std::io::Error::last_os_error()
}

/// The processes in the job, this supervisor included; the count a failed query gives keeps the
/// session going.
fn active_processes(job: &Owned) -> u32 {
    // SAFETY: the structure is as large as told.
    unsafe {
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = std::mem::zeroed();
        let ok = QueryInformationJobObject(
            job.0,
            JobObjectBasicAccountingInformation,
            (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
            size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
            std::ptr::null_mut(),
        );
        if ok == 0 {
            u32::MAX
        } else {
            info.ActiveProcesses
        }
    }
}

/// The ids of the job's processes but this supervisor.
fn game_processes(job: &Owned) -> HashSet<u32> {
    /// `JOBOBJECT_BASIC_PROCESS_ID_LIST` with room for many processes.
    #[repr(C)]
    struct List {
        assigned: u32,
        listed: u32,
        ids: [usize; 4096],
    }
    let mut list = Box::new(List {
        assigned: 0,
        listed: 0,
        ids: [0; 4096],
    });
    // SAFETY: the structure is as large as told, and laid out as Windows expects.
    let ok = unsafe {
        QueryInformationJobObject(
            job.0,
            JobObjectBasicProcessIdList,
            (&mut *list as *mut List).cast(),
            size_of::<List>() as u32,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return HashSet::new();
    }
    let me = std::process::id();
    list.ids[..(list.listed as usize).min(list.ids.len())]
        .iter()
        .map(|&id| id as u32)
        .filter(|&id| id != me)
        .collect()
}

/// Asks every visible window of `processes` to close, as its close button would.
fn close_windows(processes: &HashSet<u32>) {
    unsafe extern "system" fn each(window: HWND, processes: LPARAM) -> BOOL {
        // SAFETY: `processes` is the set `close_windows` passed, alive for the enumeration.
        unsafe {
            let processes = &*(processes as *const HashSet<u32>);
            let mut pid = 0;
            GetWindowThreadProcessId(window, &mut pid);
            if processes.contains(&pid) && IsWindowVisible(window) != 0 {
                PostMessageW(window, WM_CLOSE, 0, 0);
            }
        }
        1
    }
    // SAFETY: the callback reads the set only while EnumWindows runs.
    unsafe { EnumWindows(Some(each), processes as *const HashSet<u32> as LPARAM) };
}

/// Ends `processes` at once.
fn end_processes(processes: &HashSet<u32>) {
    for &pid in processes {
        // SAFETY: the handle is closed by `Owned`.
        unsafe {
            let process = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if !process.is_null() {
                let process = Owned(process);
                TerminateProcess(process.0, 1);
            }
        }
    }
}
