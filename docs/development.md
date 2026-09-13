# Development and verification

[Back to the README](../README.md)

## Prerequisites

- Rust 1.85 or newer (`Cargo.toml` declares `rust-version = "1.85"`). `mise install` uses the repository's pinned Rust toolchain and installs lefthook.
- tmux 3.2 or newer, Git, and a login-capable shell on `PATH`.
- Python 3 for the runtime-size audit and release helper checks.
- Node.js 18 or newer only when exercising the generated npm package launcher.

The tmux floor is based on the feature used by the real popup workflow: tmux's [3.2 change log](https://github.com/tmux/tmux/blob/3.2/CHANGES) adds per-client transient popups and `display-popup`. ezm also probes zoom-flag support and can fall back for older command capabilities, but popup support is a required part of the supported interactive surface. Local and CI E2E runs record their actual `tmux -V`; the CI workflow currently uses the versions supplied by its Linux and macOS runners rather than a pinned 3.2 job.

## Verification commands

Run formatting, strict linting, locked tests, and the source-structure audit from the repository root:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --locked --lib
cargo test --locked
python3 scripts/audit_runtime_file_sizes.py
python3 -m unittest discover -s scripts/release -p 'test_*.py'
```

The CI workflow also runs the real tmux suites. Each suite starts its own private tmux server and never uses the user's default tmux server:

```bash
cargo test --locked --test foundation_e2e -- --nocapture --test-threads 1
cargo test --locked --test core_session_e2e -- --nocapture
EZM_SMOKE_PLATFORM=linux EZM_SMOKE_MAX_CARGO_JOBS=2 EZM_SMOKE_TEST_THREADS=1 \
  cargo test --locked --test smoke_e2e -- --nocapture --test-threads 1
cargo test --locked --test focus_reduced_layout_anchor -- --nocapture
cargo test --locked --test zoomed_mode_switch_e2e -- --nocapture
cargo test --locked --test fresh_mode_cache_geometry_e2e -- --nocapture --test-threads 1
cargo test --locked --test runtime_auth_cache_regression -- --nocapture --test-threads 1
```

Use `EZM_SMOKE_PLATFORM=macos` for the macOS smoke profile. The E2E harness requires `tmux` and Git; OpenCode, perles, Neovim, Lazygit, SSH, mosh, and tssh are not prerequisites for the local harness.

For a release-style locked build:

```bash
cargo metadata --no-deps --format-version 1
cargo build --release --locked --bin ezm
```

## Evidence paths

Integration tests write machine-readable evidence under:

```text
target/e2e-evidence/<suite>/<run-id>/
```

The normal suite names are `foundation`, `core-session-orchestration`, `cross-platform-smoke`, `focus-reduced-layout`, `focus-reduced-layout-socket`, and `zoomed-mode-switch`. Core and smoke runs include `summary.json`; individual case evidence is under `cases/`. CI attempts to upload available E2E evidence after each suite, whether it passes or fails, including release-platform runs. Release verification records and assembled release evidence are produced under `dist/` by the release workflow and are not checked into the repository.

## Architecture map

```text
src/main.rs
  └─ lib.rs / cli.rs                 process entrypoint and argument parsing
       └─ app.rs                     orchestration and command dispatch
            ├─ config.rs + load.rs   config path and value resolution
            ├─ logging/              per-launch logs and log opening
            └─ session/
                 ├─ runtime.rs       resolved context, session create/attach
                 ├─ resolver.rs      canonical project/session identity
                 ├─ repair.rs        damage analysis and selective recovery
                 └─ tmux/
                      ├─ command.rs  process boundary and diagnostics
                      ├─ layout/     pane topology, presets, geometry
                      ├─ keybinds.rs runtime routing and mode keys
                      ├─ mode_runtime/ persistent mode backing panes and launch
                      ├─ popup/      popup helper sessions and cleanup hooks
                      ├─ auxiliary.rs perles window and remote viewer launch
                      └─ remote_*    authority parsing, path remap, transports
```

`app` resolves configuration once. `session::runtime` owns the project-session lifecycle and context reconciliation. The tmux modules translate that context into targeted tmux commands; lower layers should not reread process environment to reinterpret an already-resolved project.

## Why zoomed mode has a separate entrypoint

`tests/zoomed_mode_switch_e2e.rs` intentionally remains separate from `tests/core_session_e2e.rs`. It owns one isolated `zoomed-mode-switch` harness and focuses on the `E2E-20` transition: enable tmux zoom, send the real `prefix+N` route, and verify that the selected slot and zoom state survive the mode switch. Keeping this timing- and geometry-sensitive scenario as its own Cargo test target makes the broad core suite easier to diagnose while ensuring CI still invokes the zoomed workflow explicitly.

The reduced-layout entrypoint is likewise explicit because it checks two- and four-pane geometry, focus promotion, and the short private socket path used by macOS/Linux harnesses.
