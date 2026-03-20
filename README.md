# ez-mux
<img width="704" height="384" alt="stylized_ez_mux_image" src="https://github.com/user-attachments/assets/18385b30-042a-4996-b14e-18053f4b2ba6" />

`ez-mux` is a standalone Rust CLI project for deterministic tmux workspace orchestration.

Primary command:

- `ezm`

## Current Status

- Repository foundation and planning docs are in place.
- Epic gates and E2E ownership are defined in `docs/epics/INDEX.md`.
- A full read-only Focus5/NTM reference snapshot is imported under `reference/ntm-focus5/`.
- Runtime implementation has not started yet.

## Project Goals

- Deliver an installable, reliable `ezm` CLI for Linux and macOS.
- Keep a clean-room rewrite approach (behavioral parity, not direct code porting).
- Validate behavior with real tmux E2E tests in isolated tmux server namespaces.
- Keep `ez-mux` decoupled from host-repo internals; integrate through stable CLI/env/config contracts.

## Key Docs

- Canonical plan: `docs/plan.md`
- Implementation staging: `docs/implementation-plan.md`
- Epic gate order and dependencies: `docs/epics/INDEX.md`
- Reference corpus provenance: `reference/ntm-focus5/PROVENANCE.md`

## Near-Term Cutover Direction

- Continue developing and validating `ez-mux` as an independent project surface.
- Use host integration via `EZM_BIN` override during transition.
- Move this project to its own top-level repository as the cutover target state.

## Notes

- The Focus5/NTM snapshot is reference-only and must not be directly ported into runtime modules.
