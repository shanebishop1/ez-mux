# External project and agent adapters

Status: Complete
Owner/session: Codex /root, current user conversation, 2026-10-04
Scope: isolated refactor/external-project-adapters; workspace manifest and generic
commands, removal of project registry/routing and conversation API/state ownership.
State: released; implementation and non-disruptive rollout verified

The user approved extracting machine policy into dotfiles. Ezm retains group/slot
composition, ordinary directory startup, generic remote transport and native v2
client launch/theme integration. It accepts an explicit local JSON manifest and
never resolves project aliases or credentials. External agent commands own stable
conversation identity. Existing slot placeholders remain numeric-only so running
callbacks can survive activation; new commands also receive EZM_SLOT.

Verify locked Rust tests including isolated tmux lifecycle, strict Clippy, format,
runtime-size audit and release helpers. External API/host routing coverage moves
with its implementation into dotfiles. Preserve all production panes and backend
processes during rollout; retain prior executable builds for existing callbacks.

Verification completed 2026-10-04: all hosted CI gates passed at source
50dde046c7ad6041f0f234aea83126ab57aeaefe, including the complete locked test surface,
Linux/macOS terminal integration, MSRV, strict Clippy and format. Independent review
findings were corrected. Dotfiles owns the extracted adapters and private registry.

Deployed on the Mac and Shane after the source and consumer PRs merged. All four
saved conversation identities passed read-only v2 validation. Before/after checks
confirmed unchanged live pane IDs/PIDs, backend PIDs, active focus and conversation
file hashes. Existing executable builds remain available for live processes.
All inventoried named-command consumers were migrated with migration-only commits;
unrelated staged and unstaged edits were preserved.
