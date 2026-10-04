# Named project workspaces

`ezm open PROJECT` is the uniform entrypoint for a project on this host or over
SSH. A project has subgroup windows; groups use normal Git worktree discovery unless explicit slots are configured.
The ordinary five-pane default and ez-mux.toml settings apply. Perles is an optional project-wide window without agent slots.

## Quick runbook

```sh
ezm open shmovie                 # reconnect; do not create a new conversation
ezm open shmovie --group main --slot 2
ezm open shmovie --group review  # group must be defined in the registry
ezm status --all
ezm doctor shmovie
ezm attach shmovie --slot 2      # direct TUI on execution host (no tmux takeover)
ezm new shmovie --group main --slot 2
```

`new` creates and saves the next OpenCode conversation. Running terminals are
unchanged; close/reopen the group when you are ready to use that conversation.
Previous history and backend work are retained. Use `new` deliberately, not to reconnect.

For an existing slot, map its known existing conversation before opening:
`ezm adopt shmovie --group main --slot 2 --session ses_EXACT_ID`.
Adoption verifies the session's directory and never replaces a running client.
Existing OpenCode slots should set `require_existing = true`: ezm refuses to invent
a conversation when their mapping is missing. Choose `new` explicitly only when
there is no existing conversation to preserve.

Detach with tmux `prefix d` to leave work running. `ezm close PROJECT --group NAME`
closes only that group's terminal processes. `ezm close PROJECT` closes project
views while preserving backend and conversations. Backend administration belongs
to OpenCode or the machine supervisor; ezm has no service start/stop commands. Worktrees and saved
conversation history are never deleted by these commands.

`ezm open ... --no-attach` ensures the view without taking over the terminal.
All project operations route over SSH when a host is configured. Credentials are
loaded on the execution host. `--local` is the internal routing escape hatch.

## Registry and trust

The orchestration registry is `$XDG_CONFIG_HOME/ez-mux/projects.toml`, falling back to
`~/.config/ez-mux/projects.toml` on both Linux and macOS. `EZM_PROJECTS_CONFIG`
selects a different reviewed file. It names workspaces; normal tool/layout settings still come from `ez-mux.toml`
on the execution host, using the usual environment/file precedence.
Project and group names use letters, digits, hyphens and underscores.

```toml
[projects.example]
root = "/srv/example"
server = "http://127.0.0.1:4097"
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
OpenCode slots can omit `server` and `credentials`. Worktrees must
already exist. Slots within a group require distinct absolute directories;
assign separate worktrees to concurrent writers in different groups as well.

On a laptop, the matching project definition can set `host = "my-devbox"` and
`remote_binary = ".local/bin/ezm"`. Its remote host must have its own definition
with the project name and valid execution-host paths. The laptop needs only
`[projects.example]` and `host = "my-devbox"`; do not duplicate groups or paths. Transport selection honors the normal tssh/mosh settings. Mosh is used for
interactive open/attach; noninteractive operations use SSH (or tssh). No passwords travel in arguments.

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
Helper/owner sessions may appear in raw `tmux list-sessions`; `ezm status` reports
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
never rebuilds another group or restarts the backend. Configured endpoints are
checked with authenticated OpenCode v2 `/api/info`; unavailable servers fail
without modifying services. Start them with their external supervisor.

## Internal boundaries

One installed executable contains separate workspace orchestration, layout,
routing and agent-adapter modules. The workspace layer chooses project/group/slot
identity and execution host. The layout layer accepts explicit worktrees and a
generic agent command; it does not manage backend services. The OpenCode adapter
owns stable conversation mapping. OpenCode or the machine supervisor owns backend processes. Ezm never installs,
starts or stops system services and does not require one server per project.

Bare ezm, repair, preset and kill retain their cwd-based layout semantics. Use
named workspace commands for managed projects. Layout subprocesses and callbacks
use the exact current executable, so unrelated PATH entries cannot change builds.

## Verification

The CLI boundary test checks that workspace and layout operations share one
entrypoint. OpenCode tests cover stable reuse, explicit replacement, API failure
and concurrent opens. Real tmux tests cover group isolation, mode switching,
scoped close, ownership conflicts and pane preservation during reopen.

## Minimal local project

```toml
[projects.example]
root = "/srv/example"
```

With no groups, `main` is implicit. With no slots, ezm discovers worktrees using
its normal rules and honors `panes` in ez-mux.toml (five by default). Existing
owners retain their saved assignments on reopen; discovery does not add or
replace live agents. Explicit slots override discovery for selected groups.
Backend endpoints and credentials are optional adapter inputs, not ownership
claims over those servers. Ordinary configured agent commands remain supported.
