# Native agent launching and terminal workspace simplification

Status: Approved
Last updated: 2026-10-04
Owner/session: Codex /root, current user conversation
Scope: isolated refactor/native-opencode-workflow; custom command contract, generic manifest validation, runtime group identity and their tests/docs.
State: active

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
