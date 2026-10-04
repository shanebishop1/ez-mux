use super::config::Project;
use crate::{app::AppError, opencode::error};
use std::{collections::BTreeMap, process::Command};
pub(super) fn credentials(project: &Project) -> Result<BTreeMap<String, String>, AppError> {
    let mut values = BTreeMap::new();
    // Mask inherited credentials from another project, even for unauthenticated hosts.
    values.insert("OPENCODE_SERVER_PASSWORD".into(), String::new());
    values.insert("OPENCODE_SERVER_USERNAME".into(), "opencode".into());
    if let Some(path) = &project.credentials {
        let out = Command::new("bash")
            .env("OPENCODE_SERVER_PASSWORD", "")
            .env("OPENCODE_SERVER_USERNAME", "opencode")
            .args([
                "-c",
                "set -a; source \"$1\" || exit; env -0",
                "ezm-credentials",
            ])
            .arg(path)
            .output()
            .map_err(|_| error("Cannot read credential environment"))?;
        if !out.status.success() {
            return Err(error("Credential environment could not be loaded"));
        }
        for item in out.stdout.split(|b| *b == 0) {
            let item = String::from_utf8_lossy(item);
            if let Some((key, value)) = item.split_once('=') {
                if matches!(key, "OPENCODE_SERVER_PASSWORD" | "OPENCODE_SERVER_USERNAME") {
                    values.insert(key.into(), value.into());
                }
            }
        }
    }
    Ok(values)
}
fn opencode_command(project: &Project) -> Result<Command, AppError> {
    let mut cmd = Command::new(
        project
            .opencode_binary
            .as_deref()
            .unwrap_or(std::path::Path::new("opencode")),
    );
    cmd.envs(credentials(project)?);
    Ok(cmd)
}
pub(super) fn health(project: &Project) -> Result<String, AppError> {
    let Some(server) = &project.server else {
        return Ok("not configured (custom agents)".into());
    };
    let mut command = opencode_command(project)?;
    command.args(["api", "--server", server, "GET", "/api/info"]);
    let out = crate::process::capture(&mut command)?;
    if !out.status.success() {
        return Err(error("OpenCode API unavailable or authentication failed"));
    }
    let data: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|_| error("Invalid API health response"))?;
    let version = data
        .get("version")
        .or_else(|| data.pointer("/data/version"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if !version.starts_with("2.") {
        return Err(error("Backend is not OpenCode v2"));
    }
    Ok(version.into())
}
// Servers are externally supervised; workspace operations never start or stop them.
pub(super) fn check(project: &Project) -> Result<(), AppError> {
    health(project).map(|_| ())
}
