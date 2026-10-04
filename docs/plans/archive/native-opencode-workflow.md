# Native agent launching and terminal workspace simplification

Status: Complete
Last updated: 2026-10-04
Owner/session: Codex /root, current user conversation
Scope: isolated refactor/native-opencode-workflow; custom command contract, generic manifest validation, runtime group identity and their tests/docs.
State: closed; rollout verified, session ownership released after merge

The user approved the audit's simplification pass and continued OpenCode v2
integration. Ezm retains generic subgroup/slot composition. External project
configuration launches native clients without permanent conversation mappings.

Restore custom agent command text unchanged and expose only numeric EZM_SLOT.
Allow repeated explicit directories and additional empty visible panes. Remove
legacy owner/adoption configuration; reconnect existing ezm-owned group windows
through runtime metadata, preserving their processes and backing sessions.

Keep the existing layout manifest interface rather than replacing the entire
session engine without evidence. Group ownership checks and teardown remain
necessary terminal safety. Native v2 command/theme support stays in scope.

Verify focused Rust tests and isolated real-tmux lifecycle, then locked full
surface, strict Clippy, formatting and release gates. Deploy only after reviewed
source merges. Preserve active pane/backend PIDs, focus and conversation history;
retain old executable builds for running clients. Dotfiles owns the native client
probe, machine configuration, credential loading and consumer rollout.


## Completion evidence

2026-10-04: reviewed implementation merged (ez-mux #6, dotfiles #32) and
installed on Mac and Shane. Consumer runbooks and task descriptions merged in
Goblinham #160/#161 and Shmovie #47/#48, then applied as migration-only commits.
All unrelated staged/unstaged changes were preserved by captured-file and
alternate-index checks. Newly discovered legacy worktrees also received the
reviewed v2 pins, plugin hooks and bootstrap consumer patch.

Verification: complete locked Rust surface, strict all-target Clippy, formatting,
runtime structure audit and 23 release checks passed; isolated real-tmux tests
cover preserved owner identity/PIDs, literal commands, repeated directories,
extra empty panes and four-pane logical worktree order. Default dotfiles suite:
1,001 passed, six skipped. Source CI passed on macOS/Linux. Independent review
found no remaining implementation blocker.

Native disposable server checks verified exact-directory continuation across
sibling worktrees, explicit existing IDs and two simultaneous clients with no
model prompt submissions. Fresh interactive shells on both machines select the
pinned v2 CLI. All three remote backends resolve the four saved conversation IDs.
The final snapshots retain five unique remote panes with identical PIDs/focus,
backend PIDs and old mapping-file hashes. Old adapter files/links were removed
only after future callbacks were refreshed; old builds and mapping evidence remain
available for rollback. No service restart, pane respawn or model prompt occurred.

The final audit covers six valid Mac and 47 valid remote project locations.
Remote runtime manifests (44) contain no old v1 CLI or npm ezm pin. Runtime and
consumer records are under ~/.local/state/project-runtime/native-v2-rollout-20261004
on each host, including per-branch commit inventories and before/after snapshots.
Temporary checkouts are released after their publication/evidence is preserved.

## Remaining deployment boundary

This phase removes the bespoke conversation adapter and uses native OpenCode v2.
Consolidating the three still-active services is deferred to an idle cutover.
They share conversation storage, but endpoint authentication and Exaskill's
explicit configuration overlay must be reconciled first. Do not restart or stop
active services to perform that change. Current operating guidance is in the
project-session runbook; this plan is historical evidence.
