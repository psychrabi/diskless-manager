use std::process::{Command, Output};

/// Run a blocking `Command` on the blocking thread pool, keeping the
/// async runtime responsive. Returns the same `Output` as `.output()`.
pub async fn run_command(cmd: &mut Command) -> std::io::Result<Output> {
    let args: Vec<std::ffi::OsString> = cmd
        .get_args()
        .map(|a| a.to_os_string())
        .collect();
    let prog = cmd.get_program().to_os_string();

    tokio::task::spawn_blocking(move || {
        let mut c = Command::new(&prog);
        c.args(&args);
        c.output()
    })
    .await
    .map_err(std::io::Error::other)?
}