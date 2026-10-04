# Named project workspaces

`remote-agents open PROJECT` is the uniform entrypoint for a project on this host or over
SSH. A project has subgroup windows; each group has one to five explicit worktree
slots. Perles is an optional project-wide window without agent slots.

## Quick runbook

```sh
remote-agents open shmovie                 # reconnect; do not create a new conversation
remote-agents open shmovie --group main --slot 2
remote-agents open shmovie --group review  # group must be defined in the registry
remote-agents status --all
remote-agents doctor shmovie
remote-agents start shmovie                # backend only
remote-agents attach shmovie --slot 2      # direct TUI on execution host (no tmux takeover)
remote-agents new shmovie --group main --slot 2
```

`new` creates and saves the next OpenCode conversation. Running terminals are
unchanged; close/reopen the group when you are ready to use that conversation.
Previous history and backend work are retained. Use `new` deliberately, not to reconnect.

For an existing slot, map its known existing conversation before opening:
`remote-agents adopt shmovie --group main --slot 2 --session ses_EXACT_ID`.
Adoption verifies the session's directory and never replaces a running client.
Existing OpenCode slots should set `require_existing = true`: ezm refuses to invent
a conversation when their mapping is missing. Choose `new` explicitly only when
there is no existing conversation to preserve.

Detach with tmux `prefix d` to leave work running. `remote-agents close PROJECT --group NAME`
closes only that group's terminal processes. `remote-agents close PROJECT` closes project
views while preserving backend and conversations. `remote-agents stop PROJECT` also stops
its configured service and can interrupt active backend work. Worktrees and saved
conversation history are never deleted by these commands.

`remote-agents open ... --no-attach` ensures the view without taking over the terminal.
All project operations route over SSH when a host is configured. Credentials are
loaded on the execution host. `--local` is the internal routing escape hatch.

## Registry and trust

The orchestration registry is `$XDG_CONFIG_HOME/remote-agents/projects.toml`, falling back to
`~/.config/remote-agents/projects.toml` on both Linux and macOS. `REMOTE_AGENTS_CONFIG`
selects a different reviewed file. This is separate from legacy `ez-mux.toml`.
Project and group names use letters, digits, hyphens and underscores.

```toml
[projects.example]
root = "/srv/example"
server = "http://127.0.0.1:4097"
service = "opencode-example.service"
credentials = "/home/me/.config/example/credentials.env"
opencode_binary = "/home/me/.local/bin/opencode"

[[projects.example.groups.main.slots]]
directory = "/srv/example-1"

[[projects.example.groups.main.slots]]
directory = "/srv/example-2"

[[projects.example.groups.review.slots]]
directory = "/srv/example-review"

[projects.example.perles]
directory = "/srv/example"
command = "perles --beads-dir /srv/example/.beads"
```

A slot's optional `command` runs another CLI instead of OpenCode. A project without
OpenCode slots can omit `server`, `credentials`, and `service`. Worktrees must
already exist. Slots within a group require distinct absolute directories;
assign separate worktrees to concurrent writers in different groups as well.

On a laptop, the matching project definition can set `host = "my-devbox"` and
`remote_binary = ".local/bin/remote-agents"`. Its remote host must have its own definition
with the same project/group names and valid remote paths. SSH transport uses the
user's existing SSH configuration. No passwords travel in arguments.

Internal group launchers substitute the stable `{slot}` number; conversation
identity never follows a shell's changed working directory.

Slot commands, Perles commands, and credential environment files are trusted
local executable configuration. Inspect unfamiliar registries before using them.
Credentials remain referenced in private files, never copied into TOML or
conversation records. Create the registry directly using the example above.

## Identity and persistence

Each group has an independent owner session with ezm's existing slot registry,
mode cache and popup processes. Its canonical window is linked into the visible
project session. The linked window's owner metadata routes ezm bindings to the
correct group. These are shared tmux windows, not nested terminal multiplexers.
Helper/owner sessions may appear in raw `tmux list-sessions`; `remote-agents status` reports
the project hierarchy instead.

Conversation records live under `$XDG_STATE_HOME/ez-mux/conversations`, falling
back to `~/.local/state/ez-mux/conversations`. Records contain the project/group/
slot key, server URL, directory and OpenCode session ID; no credentials. Exclusive
file locks serialize concurrent opens. Atomic replacement preserves state on
failure. A stale/deleted conversation, changed directory, or failed API request
fails visibly rather than silently creating another conversation. Use `new` to
explicitly choose a replacement.

A group can reference an existing owner session using its `owner` field. On first open, ezm
checks its slot worktrees before adopting its window. Adoption does not respawn
live panes. Existing agent processes retain their current conversation; future
launches use the unified adapter. Conversation records must be adopted from known
session IDs when rolling out to live clients; never infer the most recent chat.

Changing worktrees for an already-open group is rejected. Close only that group
when its work is finished, edit its configuration, and reopen. Opening one group
never rebuilds another group or restarts the backend. Service `start` is
idempotent; readiness checks use authenticated OpenCode v2 `/api/info`.

## Layout engine boundary

Bare `ezm`, `ezm repair`, `ezm preset`, and `ezm kill` retain their original cwd-
based semantics. Use remote-agents for named project lifecycle. The ezm parser does not accept
project lifecycle commands. The orchestrator calls a pinned sibling ezm for
layout operations; callbacks always target that layout binary. Native
shared-server agent launches now use the v2 API and `--server`/`--session`, with
stable legacy directory/slot conversation keys. Slot appearance uses
`OPENCODE_CLI_CONFIG_CONTENT`; it no longer overrides server config directories.

## Verification

`tests/opencode_v2.rs` covers conversation reuse, explicit replacement, API failure
and concurrent open. `tests/project_workspaces.rs` uses a private real tmux server
to cover independent groups, stable reopen and scoped close.

## Installation boundary

Install both native binaries from the same source revision. remote-agents owns
SSH, project definitions, service requests and saved conversations. ezm owns
layout/slots/modes and accepts a generic agent command; it does not depend on the
workspace module. Missing companion ezm fails before service or tmux mutations.
The project registry belongs to remote-agents; conversation state stays at the
existing ez-mux state path to preserve already-running sessions. Backend services
remain independently supervised by systemd.
