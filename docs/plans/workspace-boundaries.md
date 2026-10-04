# Workspace responsibility boundaries

Status: Superseded by [external adapters](external-adapters.md)
Last updated: 2026-10-03
Owner: Codex /root, user conversation

## Goal
Keep ezm a portable worktree/terminal workspace manager. Move machine backend
administration to the user's dotfiles. Preserve running work and conversation IDs.
The user approved the preceding architecture audit and this boundary.

## Scope / Out of Scope
Ezm owns named workspace routing, groups, stable worktree slots, views and optional
agent adapters. Dotfiles owns installation, credentials policy, systemd units,
server environment and administrative start/stop. Backend consolidation and
conversation-switch UI refinements are separate follow-ups, not prerequisites.

## Dependencies and Constraints
Do not restart services, respawn active panes, create chats or submit prompts in
rollout. Preserve dirty consumers and remote branches. No compatibility aliases
for removed administration commands. Existing credential references are adapter
inputs; provider credentials remain the machine's responsibility.

## Execution Breakdown
1. Remove service fields and start/stop verbs from ezm; endpoint checks remain
   read-only. Dotfiles supplies opencode-backend administration and server entrypoint.
2. Make remote project entries destination-only. Resolve normal ez-mux.toml on
   the execution host. Keep explicit groups as overrides; restore discovered
   worktrees/default pane count when slots are omitted. Preserve live assignments.
3. Unify named-route transport selection with existing SSH/mosh/tssh settings.
4. Update docs, consumers and registry/service definitions without restarting
   services. Publish/merge reviewed source, install pinned builds on both hosts.

## Validation Evidence Expectations
Behavior tests prove removed service controls, no implicit systemctl calls,
minimal remote registry, inherited agent/theme config and automatic worktree
selection. Real tmux tests prove reopen preserves PIDs and independent groups.
Run format, clippy and full Rust suite; focused dotfiles tests and smoke checks.
Snapshot live process IDs and conversation records before/after deployment.

## Ownership
2026-10-03: /root owns /private/tmp/ez-mux-workspace-boundaries on
refactor/workspace-boundaries and /private/tmp/dotfiles-workspace-boundaries on
the same branch name in its own repository. Shared checkout edits are excluded.
State: active. No cleanup authorization.

## Related Docs
../workspaces.md; ../configuration.md; unified-workspaces.md (earlier design,
service ownership superseded by this plan).

## Verification progress

Regression tests first reproduced rejected destination-only routing, lost inherited
theme configuration, missing default discovery, ignored tssh selection, literal
slot placeholders, and accepted unsafe explicit-pane configuration. All focused
regressions now pass. Strict all-target clippy passes. Independent review found
two edge cases, both reproduced and fixed. Full Rust suite is running.

Full `cargo test --locked --workspace --all-targets` passed (401 library tests
and all integration/isolated tmux targets). Strict clippy and format checks pass;
runtime size audit has no hard failures; all 23 release-helper tests pass.
Second independent review found no remaining scoped blockers. Rollout pending.
