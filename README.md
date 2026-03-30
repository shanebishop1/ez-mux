# ez-mux
<img width="704" height="384" alt="stylized_ez_mux_image" src="https://github.com/user-attachments/assets/18385b30-042a-4996-b14e-18053f4b2ba6" />

`ez-mux` is a Rust CLI that turns a Git project into a ready-to-work tmux workspace.

The command is `ezm`.

## What you get

- Deterministic per-project tmux sessions (`ezm-<project>-<hash>`), so each repo reopens in its own workspace.
- Automatic 5-slot pane bootstrap with deterministic worktree assignment (slot 1..5).
- Slot modes built in: `agent` (OpenCode), `shell`, `neovim`, and `lazygit`.
- Runtime keybinds for focus/swap/mode switching/popup, installed automatically on startup.
- Automatic `beads-viewer` auxiliary window when `bv` is available (local or remote).
- Built-in remote routing: path remap + SSH launch for shell/neovim/lazygit/popup flows.
- Built-in OpenCode shared-server attach flow (`opencode attach ...`) for agent mode when remote routing is enabled.

## Requirements

- Linux or macOS
- `tmux` on `PATH`
- Rust `1.85+` (if building/installing from source)

Quick checks:

```bash
tmux -V
rustc --version
```

## Install

Install from this repository:

```bash
cargo install --path .
```

Or build a release binary:

```bash
cargo build --release
./target/release/ezm --version
```

## Quick start

From the project directory you want to manage:

```bash
ezm
```

That will create (or reattach) the session, set up slots/modes/keybinds, and open the auxiliary viewer when available.

CLI commands:

```bash
ezm --panes 3
ezm preset --preset three-pane
ezm logs open-latest
```

Help:

```bash
ezm --help
```

## Keybinds

`prefix` means your tmux prefix key (usually `C-b`).

| Key | Action |
| --- | --- |
| `prefix f` then `1..5` | Move selected slot pane to center (swap-to-center) and focus it |
| `prefix u` | Toggle current slot mode (`agent` <-> `shell`) |
| `prefix a` | Set current slot to `agent` mode |
| `prefix S` | Set current slot to `shell` mode |
| `prefix N` | Set current slot to `neovim` mode |
| `prefix G` | Set current slot to `lazygit` mode |
| `prefix P` | Toggle slot popup shell |
| `prefix d` | Detach (or hard-close when in popup context) |
| `prefix h/j/k/l` | Pane navigation with slot-aware border refresh |
| `prefix M-3` | Toggle `three-pane` preset |

## Configuration

Config file name: `ez-mux.toml`

Lookup order:

1. `EZM_CONFIG` (explicit path override)
2. `./ez-mux.toml` (current working directory)
3. OS default location

Default global config paths:

- Linux: `$XDG_CONFIG_HOME/ez-mux/ez-mux.toml` (fallback `~/.config/ez-mux/ez-mux.toml`)
- macOS: `~/Library/Application Support/ez-mux/ez-mux.toml`

Example config:

```toml
# Startup panes for `ezm` (1..=5)
panes = 5

# Optional remote routing settings (enable SSH-backed remote launches)
ezm_remote_path = "/srv/remotes"
ezm_remote_server_url = "https://remote.example:7443"

# Optional OpenCode shared-server attach settings (agent mode)
opencode_server_url = "http://127.0.0.1:4096"
opencode_server_password = "replace-me"

# Optional agent mode override command
agent_command = 'exec opencode || exec "${SHELL:-/bin/sh}" -l'

# Optional per-slot OpenCode theming
opencode_slot_themes_enabled = true
[opencode_slot_themes]
"1" = "nightowl"
"2" = "orng"
"3" = "osaka-jade"
"4" = "catppuccin"
"5" = "monokai"
```

## Remote and OpenCode behavior

- Remote routing turns on only when both `EZM_REMOTE_PATH` and `EZM_REMOTE_SERVER_URL` are set (or equivalent config values).
- When remote routing is active, shell/neovim/lazygit and popup flows run through SSH against the mapped remote directory.
- With remote routing enabled, agent mode can attach to a shared OpenCode server via `OPENCODE_SERVER_URL` and optional `OPENCODE_SERVER_PASSWORD`.

## Environment variables

- `EZM_CONFIG`: override config file path.
- `EZM_REMOTE_PATH`: remote path base used for path remapping.
- `EZM_REMOTE_SERVER_URL`: remote server URL used with `EZM_REMOTE_PATH`.
- `OPENCODE_SERVER_URL`: optional shared-server URL.
- `OPENCODE_SERVER_PASSWORD`: optional shared-server password.
- `EZM_BIN`: binary override used by integration wrappers (not a CLI flag).

For startup pane count, precedence is `--panes` (CLI) > config file > default (`5`).

## Logging

`ezm` creates one log file per launch.

Default log locations:

- Linux: `$XDG_STATE_HOME/ez-mux/logs` (fallback `~/.local/state/ez-mux/logs`)
- macOS: `~/Library/Logs/ez-mux`

Open the latest log with:

```bash
ezm logs open-latest
```
