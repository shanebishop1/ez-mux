//! Bounded capture for service/API diagnostics; no subprocess stderr disclosure.
use crate::{app::AppError, opencode::error};
use std::{
    io::{Read, Seek, SeekFrom},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};
pub(crate) fn capture(command: &mut Command) -> Result<Output, AppError> {
    let mut stdout = tempfile::tempfile().map_err(|e| error(e.to_string()))?;
    let stderr = tempfile::tempfile().map_err(|e| error(e.to_string()))?;
    command
        .stdin(Stdio::null())
        .stdout(stdout.try_clone().map_err(|e| error(e.to_string()))?)
        .stderr(stderr);
    let mut child = command
        .spawn()
        .map_err(|_| error("Could not launch API/diagnostic command"))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| error(e.to_string()))? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error(
                "API/diagnostic command timed out; existing state retained",
            ));
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    stdout
        .seek(SeekFrom::Start(0))
        .map_err(|e| error(e.to_string()))?;
    let mut bytes = Vec::new();
    stdout
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| error(e.to_string()))?;
    if bytes.len() > 1024 * 1024 {
        return Err(error("API response exceeded size limit"));
    }
    Ok(Output {
        status,
        stdout: bytes,
        stderr: Vec::new(),
    })
}
