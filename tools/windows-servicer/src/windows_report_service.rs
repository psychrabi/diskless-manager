use anyhow::{anyhow, Context, Result};
use std::path::PathBuf;
use std::sync::OnceLock;
use windows_sys::Win32::Foundation::{GetLastError, ERROR_SERVICE_SPECIFIC_ERROR, NO_ERROR};
use windows_sys::Win32::System::EventLog::{
    DeregisterEventSource, RegisterEventSourceW, ReportEventW, EVENTLOG_ERROR_TYPE,
    EVENTLOG_INFORMATION_TYPE,
};
use windows_sys::Win32::System::Services::{
    RegisterServiceCtrlHandlerExW, SetServiceStatus, StartServiceCtrlDispatcherW,
    LPHANDLER_FUNCTION_EX, SERVICE_RUNNING, SERVICE_START_PENDING, SERVICE_STATUS, SERVICE_STOPPED,
    SERVICE_TABLE_ENTRYW, SERVICE_WIN32_OWN_PROCESS,
};

const SERVICE_NAME: &str = "DisklessManagerReporter";
static CONFIG_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn run(config_path: PathBuf) -> Result<()> {
    CONFIG_PATH
        .set(config_path)
        .map_err(|_| anyhow!("report service was already initialized"))?;
    let name = wide(SERVICE_NAME);
    let table = [
        SERVICE_TABLE_ENTRYW {
            lpServiceName: name.as_ptr().cast_mut(),
            lpServiceProc: Some(service_main),
        },
        SERVICE_TABLE_ENTRYW::default(),
    ];
    // The Windows service dispatcher invokes service_main on this process thread.
    // SAFETY: the null-terminated name and table live through this blocking call.
    let started = unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) };
    if started == 0 {
        return Err(anyhow!(
            "service dispatcher failed with Windows error {}",
            unsafe { GetLastError() }
        ));
    }
    Ok(())
}

unsafe extern "system" fn service_main(_argc: u32, _argv: *mut windows_sys::core::PWSTR) {
    let name = wide(SERVICE_NAME);
    // SAFETY: the service name is null-terminated and the handler has the SCM ABI.
    let status_handle = unsafe {
        RegisterServiceCtrlHandlerExW(name.as_ptr(), Some(service_control), std::ptr::null())
    };
    if status_handle.is_null() {
        return;
    }
    let mut status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: SERVICE_START_PENDING,
        dwControlsAccepted: 0,
        dwWin32ExitCode: NO_ERROR,
        dwServiceSpecificExitCode: 0,
        dwCheckPoint: 0,
        dwWaitHint: 30_000,
    };
    // SAFETY: the SCM returned this handle and status remains alive for each call.
    unsafe { SetServiceStatus(status_handle, &status) };
    status.dwCurrentState = SERVICE_RUNNING;
    status.dwWaitHint = 0;
    // SAFETY: the SCM returned this handle and status remains alive for each call.
    unsafe { SetServiceStatus(status_handle, &status) };

    write_event(
        EVENTLOG_INFORMATION_TYPE,
        "Diskless Manager client report service started",
    );
    let result = CONFIG_PATH
        .get()
        .context("service config path was not initialized")
        .and_then(|path| {
            crate::windows_client_report::run_report_client_with_retry(
                path,
                crate::windows_client_report::REPORT_ATTEMPTS,
                crate::windows_client_report::REPORT_RETRY_DELAY,
            )
        });
    match &result {
        Ok(()) => write_event(
            EVENTLOG_INFORMATION_TYPE,
            "Diskless Manager client report uploaded",
        ),
        Err(error) => write_event(
            EVENTLOG_ERROR_TYPE,
            &format!("Diskless Manager client report failed: {error:#}"),
        ),
    }
    status.dwCurrentState = SERVICE_STOPPED;
    status.dwWaitHint = 0;
    if result.is_err() {
        status.dwWin32ExitCode = ERROR_SERVICE_SPECIFIC_ERROR;
        status.dwServiceSpecificExitCode = 1;
    }
    // SAFETY: the SCM returned this handle and status remains alive for each call.
    unsafe { SetServiceStatus(status_handle, &status) };
}

/// Best-effort Application event log entry. `install-client.ps1` registers the
/// source so events render their message text; without registration the event
/// still appears with the raw text in the event detail view.
fn write_event(event_type: windows_sys::Win32::System::EventLog::REPORT_EVENT_TYPE, message: &str) {
    let name = wide(SERVICE_NAME);
    let text = wide(message);
    let strings = [text.as_ptr()];
    // SAFETY: the name and one-element string array remain alive for each call.
    unsafe {
        let handle = RegisterEventSourceW(std::ptr::null(), name.as_ptr());
        if !handle.is_null() {
            ReportEventW(
                handle,
                event_type,
                0,
                1,
                std::ptr::null_mut(),
                1,
                0,
                strings.as_ptr(),
                std::ptr::null(),
            );
            DeregisterEventSource(handle);
        }
    }
}

/// Control handler for the one-shot reporter service. The service never
/// advertises accepted controls (`dwControlsAccepted` stays 0), so the SCM
/// does not deliver stop or pause requests: the process runs one bounded
/// report and exits on its own. Any control that arrives anyway is
/// acknowledged with NO_ERROR so the SCM records a handled control instead
/// of tearing the service down with an unhandled-control error; no stop
/// state machine is needed for a process that finishes within seconds.
unsafe extern "system" fn service_control(
    _control: u32,
    _event_type: u32,
    _event_data: *mut core::ffi::c_void,
    _context: *mut core::ffi::c_void,
) -> u32 {
    NO_ERROR
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

const _: LPHANDLER_FUNCTION_EX = Some(service_control);
