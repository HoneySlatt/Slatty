//! The session supervisor on Windows (docs/windows-port.md). The supervisor puts itself in a job
//! object before it starts the game: every process the game starts is then in the job too, a
//! launcher that starts the game and exits included, and the session ends once the supervisor is
//! the job's last process. Windows has no signals: the launcher asks for a stop through an event
//! named after the supervisor. A first stop asks the game's windows to close, a second one ends
//! its processes. A program of GOG's setup that needs administrator rights is started through
//! Windows' prompt for them, outside the job, which cannot hold it.

use std::collections::HashSet;
use std::process::Stdio;
use std::time::Duration;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ELEVATION_REQUIRED, HANDLE, HWND, LPARAM, WAIT_OBJECT_0,
};
use windows_sys::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
    JobObjectBasicAccountingInformation, JobObjectBasicProcessIdList, QueryInformationJobObject,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, EVENT_MODIFY_STATE, GetCurrentProcess, GetExitCodeProcess, GetProcessId,
    OpenEventW, OpenProcess, PROCESS_TERMINATE, SetEvent, TerminateProcess, WaitForSingleObject,
};
use windows_sys::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    ShellExecuteExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, SW_SHOWNORMAL, WM_CLOSE,
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

/// The session's first process: started by the supervisor, in its job, or started with
/// administrator rights through Windows' prompt for them, which no job of the supervisor's can
/// hold.
enum Main {
    Child(std::process::Child),
    Elevated(Owned),
}

impl Main {
    fn id(&self) -> u32 {
        match self {
            Main::Child(child) => child.id(),
            // SAFETY: the handle is open.
            Main::Elevated(process) => unsafe { GetProcessId(process.0) },
        }
    }

    /// Its exit code once it ended; `Some(None)` when it ended without one.
    fn ended(&mut self) -> Option<Option<i32>> {
        match self {
            Main::Child(child) => child.try_wait().ok().flatten().map(|s| s.code()),
            // SAFETY: the handle is open; the code is written to a valid u32.
            Main::Elevated(process) => unsafe {
                if WaitForSingleObject(process.0, 0) != WAIT_OBJECT_0 {
                    return None;
                }
                let mut code = 0;
                Some((GetExitCodeProcess(process.0, &mut code) != 0).then_some(code as i32))
            },
        }
    }
}

/// Starts `spec`; with `elevate`, again through Windows' prompt for administrator rights when it
/// needs them (its manifest asks for them, `ERROR_ELEVATION_REQUIRED`).
fn start(spec: &LaunchSpec, elevate: bool, stdout: Stdio, stderr: Stdio) -> std::io::Result<Main> {
    let started = std::process::Command::new(&spec.program)
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .envs(spec.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn();
    match started {
        Err(e) if elevate && e.raw_os_error() == Some(ERROR_ELEVATION_REQUIRED as i32) => {
            start_elevated(spec).map(Main::Elevated)
        }
        started => started.map(Main::Child),
    }
}

/// Starts `spec` with administrator rights, Windows asking the person first (UAC). Refused there,
/// it fails with `ERROR_CANCELLED`.
fn start_elevated(spec: &LaunchSpec) -> std::io::Result<Owned> {
    let wide_path = |p: &std::path::Path| -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        p.as_os_str().encode_wide().chain([0]).collect()
    };
    let (verb, file, dir) = (
        wide("runas"),
        wide_path(&spec.program),
        wide_path(&spec.cwd),
    );
    let parameters = wide(&command_line(&spec.args));
    // SAFETY: every string ends with a nul and outlives the call; the structure is as large as
    // told. COM is set up for the shell, as `ShellExecuteExW`'s documentation asks.
    unsafe {
        CoInitializeEx(
            std::ptr::null(),
            (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
        );
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = parameters.as_ptr();
        info.lpDirectory = dir.as_ptr();
        info.nShow = SW_SHOWNORMAL;
        let started = ShellExecuteExW(&mut info);
        let error = last_error();
        // Done with COM: with `SEE_MASK_NOASYNC` the shell finished before returning.
        CoUninitialize();
        if started == 0 {
            return Err(error);
        }
        if info.hProcess.is_null() {
            return Err(std::io::Error::other("Windows started no process"));
        }
        Ok(Owned(info.hProcess))
    }
}

/// `args` as one command line, quoted as Windows programs read it back (the C runtime's rules,
/// which the standard library also follows to start a process).
fn command_line(args: &[String]) -> String {
    let mut line = String::new();
    for arg in args {
        if !line.is_empty() {
            line.push(' ');
        }
        if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '\u{b}', '"']) {
            line.push_str(arg);
            continue;
        }
        line.push('"');
        let mut backslashes = 0;
        for c in arg.chars() {
            match c {
                '\\' => backslashes += 1,
                '"' => {
                    line.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                    line.push('"');
                    backslashes = 0;
                }
                c => {
                    line.extend(std::iter::repeat_n('\\', backslashes));
                    line.push(c);
                    backslashes = 0;
                }
            }
        }
        line.extend(std::iter::repeat_n('\\', backslashes * 2));
        line.push('"');
    }
    line
}

pub(super) fn supervise(
    spec: &LaunchSpec,
    elevate: bool,
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
    let mut main = match start(spec, elevate, stdout, stderr) {
        Ok(main) => main,
        Err(e) => {
            emit(&SupervisorEvent::Failed {
                message: format!("starting {}: {e}", spec.program.display()),
            });
            return 1;
        }
    };
    emit(&SupervisorEvent::Started { pid: main.id() });
    let mut main_code = None;
    let mut main_running = true;
    let mut stops = 0;
    loop {
        if main_running && let Some(code) = main.ended() {
            main_running = false;
            main_code = code;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Arguments reach an elevated program as Windows' own parser reads them back
    /// (`CommandLineToArgvW`, the C runtime's rules): a game folder with spaces stays one argument,
    /// quotes and backslashes are kept.
    #[test]
    fn arguments_are_read_back_as_they_were_given() {
        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::UI::Shell::CommandLineToArgvW;
        let args = [
            r"/DIR=C:\Games\[FAKE] Game",
            "/VERYSILENT",
            r#"say "hi""#,
            "",
            r"C:\My Dir\",
            r"x\",
            r#"a\"b"#,
            "tab\there",
        ]
        .map(String::from);
        // The first word is the program's name, which Windows reads by other rules.
        let line = wide(&format!("program {}", command_line(&args)));
        let mut count = 0;
        // SAFETY: the line ends with a nul; Windows allocates the list, freed once read.
        let read: Vec<String> = unsafe {
            let list = CommandLineToArgvW(line.as_ptr(), &mut count);
            assert!(!list.is_null());
            let read = (1..count as usize)
                .map(|i| {
                    let word = *list.add(i);
                    let len = (0..).take_while(|&j| *word.add(j) != 0).count();
                    String::from_utf16(std::slice::from_raw_parts(word, len)).unwrap()
                })
                .collect();
            LocalFree(list.cast());
            read
        };
        assert_eq!(read, args);
    }
}
