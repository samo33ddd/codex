use super::DAEMON_PROCESS_CREATION_FLAGS;
use super::Process;
use pretty_assertions::assert_eq;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::process::Stdio;
use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;
use windows_sys::Win32::System::Threading::CREATE_BREAKAWAY_FROM_JOB;
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
use windows_sys::Win32::System::Threading::TerminateProcess;

#[link(name = "kernel32")]
unsafe extern "system" {
    #[link_name = "GetConsoleWindow"]
    fn get_console_window() -> *mut std::ffi::c_void;
}

#[test]
fn detached_launch_preflight_rejects_restrictive_job() {
    const CHILD: &str = "CODEX_TEST_RESTRICTIVE_LAUNCH_JOB";
    let executable = std::env::current_exe().expect("test executable");
    if std::env::var_os(CHILD).is_some() {
        let job = unsafe { super::CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        assert_ne!(job, 0);
        let job = unsafe {
            <std::os::windows::io::OwnedHandle as std::os::windows::io::FromRawHandle>::from_raw_handle(job as _)
        };
        assert_ne!(
            unsafe {
                super::AssignProcessToJobObject(
                    job.as_raw_handle() as _,
                    super::GetCurrentProcess(),
                )
            },
            0
        );
        // A new job does not permit breakaway. Reject before any lifecycle mutation.
        assert!(super::ensure_detached_launch(&executable).is_err());
        return;
    }
    let output = std::process::Command::new(executable)
        .args([
            "--exact",
            "backend::windows::tests::detached_launch_preflight_rejects_restrictive_job",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .output()
        .expect("isolated job test");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}

#[test]
fn daemon_default_children_do_not_open_console_windows() {
    const PARENT_REPORT: &str = "CODEX_TEST_DAEMON_CONSOLE_PARENT_REPORT";
    const CHILD_REPORT: &str = "CODEX_TEST_DAEMON_CONSOLE_CHILD_REPORT";
    const TEST_NAME: &str =
        "backend::windows::tests::daemon_default_children_do_not_open_console_windows";
    let executable = std::env::current_exe().expect("test executable");

    if let Some(path) = std::env::var_os(CHILD_REPORT) {
        let console_window = unsafe { get_console_window() } as usize;
        std::fs::write(path, console_window.to_string()).expect("write console window handle");
        return;
    }

    if let Some(path) = std::env::var_os(PARENT_REPORT) {
        let report_path = std::path::PathBuf::from(path);
        let child_report_path = report_path.with_extension("grandchild");
        let status = std::process::Command::new(executable)
            .args(["--exact", TEST_NAME, "--nocapture"])
            .env(CHILD_REPORT, &child_report_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("spawn default daemon child");
        assert!(status.success(), "default daemon child test failed");
        let console_window = std::fs::read_to_string(child_report_path)
            .expect("read console window handle")
            .parse::<usize>()
            .expect("parse console window handle");
        std::fs::write(&report_path, console_window.to_string()).expect("write parent report");
        return;
    }

    let report_dir = tempfile::tempdir().expect("temporary report directory");
    let report_path = report_dir.path().join("console-window.txt");
    let output = std::process::Command::new(executable)
        .args(["--exact", TEST_NAME, "--nocapture"])
        .env(PARENT_REPORT, &report_path)
        .creation_flags(DAEMON_PROCESS_CREATION_FLAGS & !CREATE_BREAKAWAY_FROM_JOB)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn daemon process");
    assert!(
        output.status.success(),
        "daemon process test failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let console_window = std::fs::read_to_string(report_path)
        .expect("read console window handle")
        .parse::<usize>()
        .expect("parse console window handle");
    assert_eq!(
        console_window, 0,
        "default child of daemon process received console window {console_window:#x}"
    );
}

#[tokio::test]
async fn identity_queries_do_not_require_termination_access() {
    let mut child = tokio::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Start-Sleep 60",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .kill_on_drop(true)
        .spawn()
        .expect("child");
    let process = Process::open(child.id().expect("pid"))
        .expect("query handle")
        .expect("live process");
    assert!(!process.start_time().expect("creation time").is_empty());
    assert!(process.is_running().expect("liveness"));
    // Check the rights on the actual query handle, independent of privileges
    // that could let the caller reopen the process with termination access.
    assert_eq!(
        unsafe {
            TerminateProcess(process.0.as_raw_handle() as _, /*uexitcode*/ 1)
        },
        0
    );
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(ERROR_ACCESS_DENIED as i32)
    );
    assert!(
        process
            .is_running()
            .expect("query must not terminate child")
    );
    child
        .kill()
        .await
        .expect("cleanup through original spawn handle");
}
