# ez-mux

[![CI quality gate](https://github.com/shanebishop1/ez-mux/actions/workflows/ci-quality-gate.yml/badge.svg)](https://github.com/shanebishop1/ez-mux/actions/workflows/ci-quality-gate.yml) [![Latest release](https://img.shields.io/github/v/release/shanebishop1/ez-mux?sort=semver)](https://github.com/shanebishop1/ez-mux/releases/latest)

**Multiple agents. Separate worktrees. One tmux workspace.**

`ez-mux` (`ezm`) keeps parallel development in one keyboard-driven workspace. Give each worktree a stable slot, switch between its agent, shell, Neovim, and Lazygit without restarting those tools, and bring the task that needs you into focus.

<img width="960" height="498" alt="ezm agents and worktree tool switching between Neovim, a shell, Lazygit, and a popup shell" src="docs/assets/ezm-terminal-demo.gif" />

[Watch the demo](docs/assets/ezm-terminal-demo.mp4)

**Navigate:** [Install](#install) | [Quick start](#quick-start) | [Worktrees](docs/worktrees.md) | [Keybinds](#keybinds) | [Configuration](docs/configuration.md) | [Development](docs/development.md)

## How it works

Run `ezm` in your project. It discovers your Git worktrees, assigns them to up to five numbered slots, and opens your workspace. Use tmux keybinds to focus a slot, switch tools, pop open a shell, or change layouts. Detach when you're done; run `ezm` again to reattach.

Additional worktree directories must end in `-1` through `-5`; see [setup and assignment rules](docs/worktrees.md), or use `--no-worktrees` to share the current directory across slots.

## Why ez-mux?

- **Keep your place.** Slot identities stay stable as you rearrange panes or switch layouts; tools keep running when you switch modes.
- **Manage the workspace, not the plumbing.** Worktree assignment, tool switching, popup shells, keybinds, and repair are built in.
- **Use your tools, locally or remotely.** OpenCode by default, or your own agent command; optional shared-server attach, SSH/mosh/tssh routing, and a `perles` work-tracking window.

Use it when you want to move between several live tasks without rebuilding your terminal setup. If you only need to launch a fixed layout, a session manager such as tmuxinator may be enough. `ezm` manages the workspace after launch, too.

## Install

Install on **Linux or macOS**, on x86-64 or arm64:

```bash
npm install --global ez-mux
ezm --version
```

Requires **Node.js 18+**, **tmux 3.2+**, and **Git**. Install tmux and Git with your system package manager; npm does not install them. The package includes prebuilt native binaries. Native Windows is not supported.

To upgrade:

```bash
npm install --global ez-mux@latest
```

## Requirements and optional integrations

Supported host operating systems are Linux and macOS. The feature-based minimum is **tmux 3.2 or newer**: the popup workflow depends on `display-popup`, introduced in tmux 3.2. The repository's CI records the tmux version supplied by each Linux/macOS runner; compatibility with the exact 3.2 lower bound has not been independently verified by a pinned runner job.

### Required for the normal workflow

| Dependency | Why it is required | If unavailable |
| --- | --- | --- |
| `tmux` 3.2+ | Creates the project session, panes, keybinds, popups, and mode backing panes. | Exits with an error explaining that tmux must be installed and on `PATH`. |
| A login-capable shell | Pane and remote wrappers use `$SHELL -l`, falling back to `/bin/sh -l`. | Pane launches and remote fallback shells fail if the selected shell cannot run. |
| Git | Default startup calls `git worktree list --porcelain` to discover slot worktrees. | Fresh worktree startup exits with an install/`PATH` error. Use `--no-worktrees` to run without Git. A non-Git directory still works, with a warning that slots will share the project directory. |

`ezm` must also be run from a directory it can canonicalize. Git worktrees are the intended project input, but `--no-worktrees` deliberately reuses the current directory for every slot.

### Optional tools and integrations

| Tool or integration | Used for | Missing-tool behavior |
| --- | --- | --- |
| OpenCode | Default `agent` mode and shared-server attach. | Warns that `opencode` is missing from `PATH` and opens a login shell. A failed attach is reported and also falls back to a shell. |
| `agent_command` integrations (Codex, Claude Code, or another CLI) | Replaces the default agent command. | `ezm` executes the configured command as written; its shell semantics determine failure or fallback. |
| `perles` | The auxiliary work-tracking window. | A missing local executable skips that window. A missing remote executable prints a warning and leaves the remote shell available. |
| `neovim` / `nvim` | `neovim` slot mode. | Warns that `nvim` is missing from `PATH` and opens a login shell; a non-zero tool exit is reported. |
| `lazygit` | `lazygit` slot mode. | Warns that `lazygit` is missing from `PATH` and opens a login shell; a failed invocation is reported and also returns to a shell. |
| `ssh` | Default transport when remote routing is active. | Remote launch reports the transport failure and falls back to a local login shell; the remote operation is unavailable. |
| `mosh` | Remote transport when `ezm_use_mosh` is enabled. | The selected remote launch reports failure and falls back to a local login shell. |
| `tssh` | Remote transport when `ezm_use_tssh` is enabled. | The selected remote launch reports failure and falls back to a local login shell. |

`mosh` and `tssh` are not needed for local sessions. If both switches are enabled, `tssh` takes precedence. SSH credentials, keys, and host configuration remain the responsibility of the transport tool.

## Quick start

From the project directory:

```bash
ezm
```

The first run creates a session named like `ezm-<project>-<hash>`, bootstraps the requested slots (five by default), assigns discovered worktrees, installs runtime keybinds, and attaches to the session. A later run reattaches to that project session.

OpenCode is not required for startup. Without `opencode`, populated agent slots become login shells until an agent tool is installed or `agent_command` is configured.

Useful variants:

```bash
ezm --panes 3              # start with three visible slots
ezm --no-worktrees         # reuse the current directory in every slot
ezm preset --preset three-pane
ezm repair
ezm logs open-latest
ezm --help
```

## Keybinds

`prefix` means the tmux prefix key, normally `C-b`.

| Key | Action |
| --- | --- |
| `prefix f` then `1..5` | Move the selected slot pane to the main focus position and focus it |
| `prefix u` | Toggle the current slot between `agent` and `shell` |
| `prefix a` | Set the current slot to `agent` mode |
| `prefix S` | Set the current slot to `shell` mode |
| `prefix N` | Set the current slot to `neovim` mode |
| `prefix G` | Set the current slot to `lazygit` mode |
| `prefix P` | Toggle the slot popup shell |
| `prefix d` | Detach, or hard-close when inside a popup context |
| `prefix h/j/k/l` | Pane navigation with slot-aware border refresh |
| `prefix M-3` | Toggle the `three-pane` preset |

## Configuration

See the [configuration reference](docs/configuration.md) for config paths, precedence, a complete example, session persistence, remote routing, executable config trust, environment variables, and logging.

## Development

See [development and verification](docs/development.md) for prerequisites, commands, E2E evidence paths, and the architecture map.
