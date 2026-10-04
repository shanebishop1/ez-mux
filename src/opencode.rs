//! `OpenCode` v2 session identity, independent of terminal lifetime.
use crate::app::AppError;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Deserialize, Serialize)]
struct Conversation {
    key: String,
    server: String,
    directory: String,
    session: String,
}

pub(crate) fn error(message: impl Into<String>) -> AppError {
    AppError::Runtime(message.into())
}

pub(crate) fn state_root() -> Result<PathBuf, AppError> {
    let root = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .ok_or_else(|| error("HOME or XDG_STATE_HOME is required"))?;
    Ok(root.join("ez-mux"))
}

pub(crate) fn stable_key(value: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn binary() -> std::ffi::OsString {
    std::env::var_os("EZM_OPENCODE_BIN").unwrap_or_else(|| "opencode".into())
}

pub(crate) fn api(
    server: &str,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Result<Value, AppError> {
    crate::config::validate_server_url(server, "OpenCode adapter")?;
    let mut command = Command::new(binary());
    command.args(["api", "--server", server, method, path]);
    if let Some(body) = body {
        command.args(["--data", &body.to_string()]);
    }
    // Authentication stays in the environment. Do not echo subprocess diagnostics:
    // provider/server errors can contain credentials or private response bodies.
    let output = crate::process::capture(&mut command)?;
    if !output.status.success() {
        return Err(error(
            "OpenCode API request failed; check server readiness and authentication (saved conversation retained)",
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|_| error("Invalid OpenCode API response"))
}

pub(crate) fn private_directory(path: &Path) -> Result<(), AppError> {
    fs::create_dir_all(path).map_err(|e| error(e.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|e| error(e.to_string()))?;
    }
    Ok(())
}

pub(crate) fn locked_file(path: &Path) -> Result<fs::File, AppError> {
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path).map_err(|e| error(e.to_string()))?;
    file.lock_exclusive().map_err(|e| error(e.to_string()))?;
    Ok(file)
}

pub(crate) fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), AppError> {
    use std::io::Write;
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp).map_err(|e| error(e.to_string()))?;
    file.write_all(contents)
        .and_then(|()| file.sync_all())
        .and_then(|()| fs::rename(&tmp, path))
        .map_err(|e| {
            let _ = fs::remove_file(&tmp);
            error(e.to_string())
        })
}

pub(crate) fn require_saved(server: &str, key: &str) -> Result<(), AppError> {
    let path = state_root()?
        .join("conversations")
        .join(format!("{}.json", stable_key(&format!("{server}\0{key}"))));
    if !path.is_file() {
        return Err(error(
            "Imported slot needs an explicit conversation: use ezm adopt --session ID or ezm new",
        ));
    }
    Ok(())
}

pub(crate) fn adopt(
    server: &str,
    directory: &str,
    key: &str,
    id: &str,
) -> Result<String, AppError> {
    validate_id(id)?;
    let root = state_root()?.join("conversations");
    private_directory(&root)?;
    let path = root.join(format!("{}.json", stable_key(&format!("{server}\0{key}"))));
    let _lock = locked_file(&path.with_extension("lock"))?;
    let response = api(server, "GET", &format!("/api/session/{id}"), None)?;
    let data = response.get("data").unwrap_or(&response);
    if data.get("id").and_then(Value::as_str) != Some(id)
        || data.pointer("/location/directory").and_then(Value::as_str) != Some(directory)
    {
        return Err(error(
            "Adopted conversation ID and directory must match the configured slot",
        ));
    }
    let saved = Conversation {
        key: key.into(),
        server: server.into(),
        directory: directory.into(),
        session: id.into(),
    };
    atomic_write(
        &path,
        &serde_json::to_vec(&saved).map_err(|e| error(e.to_string()))?,
    )?;
    Ok(id.into())
}

pub(crate) fn session(
    server: &str,
    directory: &str,
    key: &str,
    new: bool,
) -> Result<String, AppError> {
    let root = state_root()?.join("conversations");
    private_directory(&root)?;
    let identity = format!("{server}\0{key}");
    let path = root.join(format!("{}.json", stable_key(&identity)));
    let _lock = locked_file(&path.with_extension("lock"))?;
    if path.exists() && !new {
        let saved: Conversation =
            serde_json::from_slice(&fs::read(&path).map_err(|e| error(e.to_string()))?)
                .map_err(|_| error("Invalid saved conversation; refusing to replace it"))?;
        if saved.key != key || saved.server != server || saved.directory != directory {
            return Err(error(
                "Slot location changed; use new explicitly or restore its original mapping",
            ));
        }
        validate_id(&saved.session)?;
        let response = api(
            server,
            "GET",
            &format!("/api/session/{}", saved.session),
            None,
        )?;
        let data = response.get("data").unwrap_or(&response);
        if data.get("id").and_then(Value::as_str) != Some(saved.session.as_str()) {
            return Err(error(
                "Saved conversation was not returned by the server; use new explicitly",
            ));
        }
        if data
            .pointer("/location/directory")
            .and_then(Value::as_str)
            .is_some_and(|p| p != directory)
        {
            return Err(error(
                "Server conversation directory differs from slot; refusing to attach",
            ));
        }
        return Ok(saved.session);
    }
    let response = api(
        server,
        "POST",
        "/api/session",
        Some(&json!({"location":{"directory":directory}})),
    )?;
    let id = response
        .pointer("/data/id")
        .or_else(|| response.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(|| error("OpenCode API did not return a session ID"))?;
    validate_id(id)?;
    let saved = Conversation {
        key: key.into(),
        server: server.into(),
        directory: directory.into(),
        session: id.into(),
    };
    atomic_write(
        &path,
        &serde_json::to_vec(&saved).map_err(|e| error(e.to_string()))?,
    )?;
    Ok(id.into())
}

fn validate_id(id: &str) -> Result<(), AppError> {
    if !id.starts_with("ses")
        || id.len() > 200
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(error("Invalid OpenCode session ID"));
    }
    Ok(())
}

pub(crate) fn connect(
    server: &str,
    directory: &str,
    key: &str,
    new: bool,
) -> Result<String, AppError> {
    let id = session(server, directory, key, new)?;
    let status = Command::new(binary())
        .args(["--server", server, "--session", &id])
        .status()
        .map_err(|_| error("Could not launch OpenCode client"))?;
    if !status.success() {
        return Err(error(
            "OpenCode client exited unsuccessfully; conversation retained",
        ));
    }
    Ok(String::new())
}
