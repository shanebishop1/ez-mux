# Configuration reference

[Back to the README](../README.md) · [Worktree setup](worktrees.md)

## Config file and precedence

The file name is `ez-mux.toml`. Config **path** selection is:

1. `EZM_CONFIG`, when non-empty (explicit path override).
2. `./ez-mux.toml`, when it exists in the current directory.
3. The OS default path:
   - Linux: `$XDG_CONFIG_HOME/ez-mux/ez-mux.toml`, otherwise `~/.config/ez-mux/ez-mux.toml`.
   - macOS: `~/Library/Application Support/ez-mux/ez-mux.toml`.

For settings that have environment variables, value precedence is **environment > config file > built-in default**. Empty values are treated as unset. The startup pane count is the exception: **`--panes` (or the positional `1..5` shortcut) > `panes` in the file > `5`**.

Environment-overridable settings are:

- `EZM_REMOTE_PATH` and `EZM_REMOTE_SERVER_URL`.
- `EZM_USE_TSSH` and `EZM_USE_MOSH` (`1`, `true`, `yes`, and `on` enable a switch; `0`, `false`, `no`, and `off` disable it; other non-empty values enable it).
- `PERLES_DIR` / legacy `BEADS_DIR`, and `PERLES_DB` / legacy `BEADS_DB`.
- `OPENCODE_SERVER_URL` and `OPENCODE_SERVER_PASSWORD`, overriding the file keys `opencode_server_url` and `opencode_server_password`.

`agent_command`, `opencode_slot_themes_enabled`, and `[opencode_slot_themes]` are file settings. `EZM_BIN` is an internal integration-wrapper override, not a general runtime setting.

The exported library convenience APIs `ensure_current_project_session()` and `ensure_project_session()` retain their shipped compatibility contract and read `EZM_REMOTE_PATH`, `EZM_REMOTE_SERVER_URL`, `EZM_USE_TSSH`, and `EZM_USE_MOSH` from the process environment. They do not load the CLI config file; applications that already resolve configuration should pass the resolved runtime context API instead. The CLI itself has one authoritative `environment > config file > default` resolution path.

## Example

This example does not include credentials:

```toml
panes = 5

# Optional remote routing; both values are required to activate it.
ezm_remote_path = "/srv/remotes"
ezm_remote_server_url = "https://remote.example:7443"
ezm_use_tssh = false
ezm_use_mosh = false

# Optional work-tracking locations.
perles_dir = ".perles"
perles_db = "/path/to/perles.db"

# Optional shared OpenCode server, used by the attach flow when remote routing is active.
opencode_server_url = "http://127.0.0.1:4096"

# This is executable shell code; see the trust boundary below.
agent_command = 'exec codex || exec "${SHELL:-/bin/sh}" -l'

opencode_slot_themes_enabled = true
[opencode_slot_themes]
"1" = "nightowl"
"2" = "orng"
"3" = "osaka-jade"
"4" = "catppuccin"
"5" = "monokai"
```

## Session-scoped runtime behavior

On creation, ezm resolves the runtime context once and stores the non-secret project values in tmux **session options**. Those values include remote mapping and transport selection, perles settings, OpenCode attach URL, agent command, and slot themes. Internal mode, popup, auxiliary, and repair actions read that session context rather than reinterpreting another project's process environment.

- A session with an existing context marker keeps that context when another invocation supplies different config or environment values. This prevents project A and project B from contaminating one another.
- Popup helper sessions delegate context lookup to their recorded parent session.
- A pre-existing session with no ez-mux context marker is never initialized from the current invocation's environment or config. If its own session environment contains positively owned legacy ez-mux settings, ezm recovers the non-secret values into the session context and scrubs those legacy variables.
- Markerless sessions with no recoverable session-owned settings are ambiguous (global environment and another invocation cannot be attributed safely). They fail closed; kill the owning session and relaunch it to create a fresh context: `ezm kill`, then `ezm`.
- A legacy `OPENCODE_SERVER_URL` containing URL userinfo is not used as a credential. When its host portion can be recovered safely, migration strips the userinfo before persisting the URL; the old URL and any legacy password variable are scrubbed, and credentials must be supplied separately with `OPENCODE_SERVER_PASSWORD`. An unparseable credential-bearing URL is rejected instead and requires the same kill/relaunch reconciliation.
- A later config change does not silently rewrite a live session. Kill and recreate the project session when you intentionally want a fresh context: `ezm kill`, then `ezm`.
- The OpenCode password is not stored in the persisted context options. It is targeted to the project session environment and is only reused for a matching persisted server URL; it is never used as a global project setting. New mode-cache processes inherit the owning session's credential, with an explicit empty mask when none is configured so stale global tmux credentials cannot leak in.

## Remote routing versus OpenCode attach

Remote routing activates only when both `ezm_remote_path` / `EZM_REMOTE_PATH` and `ezm_remote_server_url` / `EZM_REMOTE_SERVER_URL` resolve to non-empty values. The local repository path is remapped under the remote base, preserving the repository basename and relative subdirectory. Shell, Neovim, Lazygit, popup, and auxiliary flows use SSH by default, or the selected `mosh`/`tssh` transport.

OpenCode shared-server attach is a separate agent-mode behavior. When remote routing is active and `opencode_server_url` / `OPENCODE_SERVER_URL` is configured, agent mode launches `opencode attach` with the remapped directory. That URL is not itself an SSH transport. A configured `agent_command` takes precedence over the built-in OpenCode launch and attach paths.

## Executable-code trust boundary

`agent_command` is not a binary name or a declarative adapter. It is a shell command string that ezm places into an agent-mode pane and executes with the configured shell. A repository-local `ez-mux.toml` can therefore cause arbitrary commands to run when you enter that checkout. Review and trust the config before running ezm in an unfamiliar repository; use `EZM_CONFIG` to point at a reviewed file or remove `agent_command` to use the built-in OpenCode behavior. ezm does not provide a trust prompt or sandbox for this setting.

## Environment variables

| Variable | Purpose |
| --- | --- |
| `EZM_CONFIG` | Explicit config file path. |
| `EZM_REMOTE_PATH` | Remote path base used for remapping. |
| `EZM_REMOTE_SERVER_URL` | Remote SSH authority/URL used with the path base. |
| `EZM_USE_TSSH` | Select `tssh` for remote launches. |
| `EZM_USE_MOSH` | Select `mosh` for remote launches when `tssh` is not selected. |
| `PERLES_DIR`, `PERLES_DB` | Perles locations; legacy `BEADS_DIR`, `BEADS_DB` are accepted as fallbacks. |
| `OPENCODE_SERVER_URL` | Shared-server URL for the OpenCode attach flow. |
| `OPENCODE_SERVER_PASSWORD` | Password delivered to the targeted session environment, not persisted in session options. |
| `EZM_BIN` | Binary override used by internal integration wrappers. |

## Logging

ezm creates one log file per launch. Default locations are `$XDG_STATE_HOME/ez-mux/logs` (fallback `~/.local/state/ez-mux/logs`) on Linux and `~/Library/Logs/ez-mux` on macOS.

```bash
ezm logs open-latest
```
