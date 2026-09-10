use std::process::Child;
use std::thread;
use std::time::{Duration, Instant};

use super::EzmBackgroundProcess;

impl EzmBackgroundProcess {
    pub fn wait_for_exit(&mut self, timeout: Duration) -> Result<i32, String> {
        if self.child.is_none() {
            return Err(format!("{} process was already reaped", self.context));
        }
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self
                .child
                .as_mut()
                .expect("child checked above")
                .try_wait()
                .map_err(|error| {
                    format!("failed checking {} process status: {error}", self.context)
                })?
            {
                let _ = self.child.take();
                return Ok(status.code().unwrap_or(-1));
            }
            if Instant::now() >= deadline {
                if let Some(mut child) = self.child.take() {
                    terminate_background_child(&mut child, &self.context);
                }
                return Err(format!(
                    "timed out waiting for {} process to exit after {} ms",
                    self.context,
                    timeout.as_millis()
                ));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for EzmBackgroundProcess {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        terminate_background_child(&mut child, &self.context);
    }
}

pub(super) fn terminate_background_child(child: &mut Child, context: &str) {
    let _ = child.kill();
    let deadline = Instant::now() + super::PTY_TEARDOWN_TIMEOUT;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                eprintln!("foundation harness: failed checking {context} during cleanup: {error}");
                return;
            }
        }
    }
    eprintln!(
        "foundation harness: timed out cleaning up {context} (pid={})",
        child.id()
    );
}
