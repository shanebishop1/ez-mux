# Unified project workspaces

Status: Superseded by [external adapters](external-adapters.md)
Last updated: 2026-10-03
Owner: Codex /root, user conversation

## Goal

One project interface for local or remote projects, subgroup windows, stable slots,
OpenCode v2 conversations, and optional Perles. Preserve active sessions and edits.

## Scope

Native v2 create/resume; explicit project registry and host overlays; group-scoped
slot lifecycles; open/new/status/doctor/close/stop; register the three existing projects. Keep legacy ezm entrypoints working. No automatic worktree
creation/deletion, prompts, backend restarts, or killing unrelated sessions.

## Design

A project contains named groups and optional auxiliary windows. Each group owns
its slot registry, worktree assignments and tool processes. Group windows are
presented together in one project tmux session; implementation may retain hidden
owner sessions for compatibility with existing mode/popup machinery. Bindings
must route to the selected window's owner, never a different group's registry.

OpenCode conversations persist separately from terminals. Store non-secret
backend/directory/session-ID mappings under the user's state directory, protected
by a process lock and atomic replacement. Reopen validates and resumes the stored
conversation. New explicitly replaces that mapping without deleting old history.
Network/auth failures must not silently create replacement conversations.

Registry files describe roots, groups, explicit worktrees, optional Perles, host
and service identities. Credentials are referenced from private environment files,
never serialized into workspace state or command arguments. Existing project
services remain supervised by systemd. Remote operations use SSH and the same remote-agents
interface on the execution host. Status distinguishes service/API/UI state.

## Execution breakdown

1. Native OpenCode v2 adapter and durable conversation identity; regression tests.
2. Project registry, explicit slots, group window ownership and routing; private
   tmux end-to-end tests for two groups, mode switching, reopen and teardown.
3. Uniform lifecycle and remote routing, registry configuration and diagnostics.
4. Canonical docs/runbook, full repository verification, independent review,
   commit/publication and additive installation on Mac/devbox. Preserve live panes;
   adopt only verified identities.

## Validation evidence expectations

Focused red/green tests for session reuse, failures, concurrency and argument
safety; isolated real tmux tests proving group isolation and idempotent reopen;
format/clippy/tests/runtime-size audit/release tests. Read-only live diagnostics
on all three projects plus disposable smoke workspaces. No model prompts.

## Ownership

2026-10-03: /root owns this new isolated checkout and all changes in
/private/tmp/ez-mux-unified-workspaces, branch feat/unified-workspaces. Existing
shared dotfiles portable cleanup and all live project edits are excluded.
State: active. Handoff: none; no cleanup of other checkouts authorized.

## Implementation verification (2026-10-03)

Full locked Cargo suite passes on macOS, including 401 library tests and all
integration/real-tmux targets. Formatting, strict all-target/all-feature clippy,
runtime-size audit (no failures), and 23 Python release-helper tests pass.
Independent review completed; all ownership, credential isolation, adoption,
Perles environment, mode-race and stable-slot findings resolved.

Two pre-existing harness failures were reproduced on unchanged upstream:
GNU coreutils sleep appears as gsleep in tmux. Fixture recognition now accepts
both names; the full suite passes with that portability correction.

Operational choice: new/adopt update the saved next conversation without killing
a live client. Imported OpenCode slots require explicit known-ID adoption or new.
Deployment/publication and live process-preservation verification remain pending.

## Final approved interface (2026-10-03)

The user selects one public tool, ezm. Keep workspace orchestration, generic
layout operations, agent adapters and backend lifecycle as separate internal
components. Remove the temporary alternate binary and profile importer, with no
compatibility alias. One registry lives at ~/.config/ez-mux/projects.toml;
conversation IDs and their existing state paths remain unchanged. /root owns
final verification, publication and non-disruptive deployment on both hosts.
