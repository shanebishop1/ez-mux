# Local workspace composition

Ezm manages terminal groups and worktree slots. Project aliases, machine routing,
backend administration, credentials and agent conversation identity belong to the
caller. A launcher may generate the manifest below; ezm has no project registry.

```sh
ezm workspace --file /absolute/layout.json open --group build
ezm workspace --file /absolute/layout.json status
ezm workspace --file /absolute/layout.json close --group build
```

```json
{
  "name": "demo",
  "root": "/projects/demo",
  "groups": {
    "build": {
      "slots": [
        {"directory": "/projects/demo-1", "command": "my-agent-wrapper"},
        {"directory": "/projects/demo-2"}
      ]
    },
    "review": {"panes": 3}
  }
}
```

Open creates or reconnects to the selected group's window in a project tmux
session. Each group has one to five slots with independent persistent agent,
shell, Neovim and Lazygit views. Reopening preserves panes and live worktree
assignments. `--slot N` selects a slot; `--no-attach` leaves the client detached.
Close tears down only the selected group's views, or the whole workspace when
no group is selected. Detach instead to leave work running.

An omitted group list means one `main` group. Omitted slots use the normal Git
worktree discovery and pane settings in `ez-mux.toml`. Explicit slots override
worktree selection. Each group can have a `command` default; each slot can override
it. Otherwise normal `agent_command` or default agent behavior applies. Tools,
themes and transport settings continue to use normal ezm configuration.

Agent shell commands receive `EZM_SLOT` containing a validated number from 1 to 5.
The existing literal `{slot}` placeholder also expands to that number; do not use
it when literal braces are intended. Commands are trusted user configuration.
Paths, names and credentials are not interpolated into that placeholder.

Optional `perles` contains `directory` and a trusted `command` for an auxiliary
window without slots. `owner` on a group may explicitly identify a preexisting ezm
session for adoption; ownership and all worktree assignments are checked before
mutation. Unknown manifest fields, including `host`, `server` and `credentials`,
are rejected. Manifests use absolute root and slot paths.

Bare `ezm` remains a useful registry-free entrypoint for the current directory.
Its existing generic SSH/mosh/tssh and OpenCode integrations remain available.
The OpenCode shared-server integration launches the native v2 client with the
server and directory; stable slot-to-conversation mapping can be supplied by an
external `agent_command`. Ezm does not call conversation APIs or save session IDs.
