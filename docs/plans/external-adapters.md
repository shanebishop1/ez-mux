# External project and agent adapters

Status: Approved
Owner/session: Codex /root, current user conversation, 2026-10-04
Scope: isolated refactor/external-project-adapters; workspace manifest and generic
commands, removal of project registry/routing and conversation API/state ownership.
State: active

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

Verification so far: 401 library tests, original core/foundation/geometry suites,
strict Clippy, format and runtime-size audit pass. 23 release-helper tests pass.
The obsolete in-ezm API timeout test moved to the passing dotfiles adapter suite;
remaining Rust integration targets are running separately. Independent review's
partial-command fallback and callback-transition findings were corrected and
verified. No production changes have been made.
