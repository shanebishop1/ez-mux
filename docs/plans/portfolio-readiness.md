# Portfolio Readiness: Hardening and Final Cleanup Plan

Status: Implemented; local validation complete. External CI execution is not claimed.

Based on: September 2026 read-only audit of ez-mux.

## 1. Goal

Make ez-mux a project that is safe to run, predictable across multiple projects, easy to understand, and backed by meaningful automated verification. Preserve the existing product and module structure. This is a focused hardening pass, not a rewrite.

The work is complete when command construction is safe, project settings cannot bleed between sessions, layout recovery respects user intent, CI exercises the real application, and the public documentation accurately describes installation, behavior, and development.

## 2. Execution Rules

- All implementation must be performed by `engineer` subagents, not `general` agents. The coordinating agent reviews, assigns work, validates results, and maintains this plan.
- Do not start with broad refactoring. Add a regression that demonstrates each defect, make the smallest correct fix, and then remove duplication exposed by that fix.
- Never use the user's default tmux server for testing. Every test and manual reproduction must use a dedicated socket and clean up only resources it owns.
- Preserve unrelated worktree changes. Do not commit or push unless explicitly requested.
- Do not introduce a plugin framework, dependency-injection framework, or generic command DSL.
- Do not split files solely to meet a line-count target. Extract behavior when it establishes a useful boundary or removes repeated decisions.
- Keep compatibility only where there is a concrete shipped behavior or persisted tmux state to preserve. Define migration/reconciliation for live sessions explicitly.
- Never put actual credentials into fixtures, evidence artifacts, task reports, or example configuration.
- Before implementing a proposed transport flag, tmux format modifier, or OpenCode secret-delivery mechanism, verify it against the supported tool versions. The audit's external documentation lookup failed; isolated tmux reproduction was used for the confirmed injection finding.

## 3. Baseline and Evidence

The audit established the following baseline, which should be rechecked before implementation because the worktree may have changed:

| Check | Audit result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Passed |
| `cargo +1.85.1 check --locked` | Passed |
| Library tests | 324 passed |
| Core tmux E2E | 10 scenarios passed, 5 failed |
| Reduced-layout E2E | Default macOS temp path caused socket-path errors; independent short-path run passed 4/4 |
| Runtime source-size audit | Passed with nine warnings, no hard-limit violations |

Core failing scenarios were E2E-01, E2E-04, E2E-06, E2E-07, and E2E-12. Investigation identified incompatible single-key `list-keys` predicates and popup visibility assertions without an attached client. Do not assume every future failure has the same cause.

The global popup cleanup hook injection was reproduced on tmux 3.7b using an isolated server and a crafted session name. This is a confirmed defect, not merely a hypothetical quoting concern.

## 4. Delivery Sequence and Ownership

| Phase | Engineer task | Depends on | Main ownership |
| --- | --- | --- | --- |
| A | Establish reliable isolated test harness | None | `tests/support`, E2E scenario predicates |
| B1 | Fix dynamic command quoting and popup guards | A for integration verification | `tmux/popup/hooks.rs`, `tmux/keybinds.rs` |
| B2 | Validate remote authorities and redact diagnostics | None; coordinate command helpers with B1 | `remote_authority.rs`, `remote_transport.rs`, diagnostic rendering |
| C | Scope runtime configuration and secrets to sessions | B1/B2 command boundaries | `config`, `app`, runtime environment and launch paths |
| D | Fix canonical window, layout recovery, and failure handling | A; coordinate runtime changes with C | session runtime, repair, presets, attach |
| E | Enforce tests and release consistency in CI | A-D | `.github`, release scripts, toolchain checks |
| F | Targeted deduplication and public documentation | B-E | identified helpers, README, package metadata |
| G | Final independent verification and review | All previous phases | no feature expansion |

Parallelize only tasks with non-overlapping file ownership. In particular, keybindings, `app.rs`, runtime launch context, and shared command helpers are conflict hotspots. Split tasks or serialize them rather than allowing multiple engineers to edit those files concurrently.

Each engineer handoff must report changed files, behavior changes, test commands and outcomes, migration implications, and any unresolved assumptions. Mark a task complete only after its acceptance criteria have been verified.

## 5. A: Reliable E2E Harness

### A1. Bound tmux socket path length

Primary files: `tests/support/foundation_harness.rs`, `tests/focus_reduced_layout_anchor.rs`.

1. Inspect every tmux harness, including standalone PTY tests, for socket creation and environment setup. Identify whether they share the foundation harness or independently repeat its logic.
2. Separate the evidence directory from the socket directory. Evidence can remain under `target/e2e-evidence`; socket paths must remain short on macOS as well as Linux.
3. Create an exclusive, private temporary directory beneath a short platform-appropriate root. Use the existing temporary-directory dependency rather than predictable shared names. Account for platform path canonicalization when calculating effective length.
4. Prefer an explicit socket path if it removes tmux's additional path components; otherwise calculate the complete path including tmux's UID directory and socket name. Set and test a conservative bound compatible with supported platforms.
5. Preserve the directory's lifetime for the entire suite. Teardown must address the exact test socket and must run on failure through owned cleanup state.
6. Remove any accidental dependence on the user's tmux configuration, plugins, default server, or remote routing environment.

Acceptance: the focused tests and core suite start with the normal macOS `TMPDIR`, with a deliberately long temp environment, and on Linux. No manual `TMPDIR=/tmp` workaround is required. Two suites can run without contacting or deleting each other's resources.

### A2. Make keybinding assertions version-resilient

Primary files: `tests/core_session_e2e/scenario_e2e_01.rs`, `scenario_e2e_04.rs`, `scenario_e2e_06.rs`, `scenario_e2e_07.rs`, `scenario_e2e_12.rs`, and `core_support.rs`.

1. Replace duplicated single-key `list-keys` probes with one test helper that lists the requested table and finds the exact table/key entry.
2. Parse enough structure to avoid substring false positives, such as matching `P` inside another key or matching a command belonging to a different binding.
3. Assert the meaningful command routing and guard behavior rather than tmux's incidental whitespace or quoting presentation.
4. On failure, include the requested table/key, tmux version, and captured table output in the evidence.
5. Add small parser tests for missing bindings, similarly named keys, and observed supported-version output.

Acceptance: the existing binding scenarios pass on tmux 3.7b and the selected supported CI versions; intentionally removing or changing a binding makes the corresponding assertion fail.

### A3. Exercise real popup behavior

Primary files: `tests/core_session_e2e/scenario_e2e_07.rs`, existing PTY support in `tests/zoomed_mode_switch_e2e.rs` and related scenario helpers.

1. Reuse the existing `portable-pty` approach to attach a real client to the isolated session.
2. Set a deterministic terminal type and dimensions, and wait for client readiness using a bounded state predicate rather than a fixed sleep.
3. Invoke popup open, close, and reopen against that explicit client. Verify visibility, helper-session persistence, and close/reopen semantics separately.
4. Keep a distinct non-interactive test for the intentional no-client behavior. Do not treat a successful no-op as evidence that a popup was displayed.
5. Capture useful failure diagnostics before cleanup: clients, panes, relevant options, and terminal output with secrets excluded.

Acceptance: visible popup behavior is actually exercised in CI. The test does not merely skip assertions when no client exists, and a broken display command is detected.

### A4. Improve test failure reporting

Print failed scenario IDs and the summary artifact path directly in the core suite failure. Preserve per-case artifacts. Avoid one opaque assertion that forces contributors to discover artifact locations manually. Do not rewrite all scenarios as separate tests unless required for isolation or useful filtering.

## 6. B1: Safe tmux-to-Shell Boundaries

Primary files: `src/session/tmux/popup/hooks.rs`, `src/session/tmux/keybinds.rs`, relevant command-construction tests.

### Implementation

1. Inventory every `run-shell`, hook, and keybinding command containing tmux formats. Record each expansion boundary: Rust formatting, tmux parsing, tmux format expansion, and shell parsing.
2. Treat dynamic format results as data, not shell source. Use a verified tmux shell-quoting format modifier where appropriate, ensuring it is used in the correct quoting context. Do not double-quote an already shell-quoted expansion without checking semantics.
3. Where a safe identifier can replace a name, prefer it. Avoid concatenating arbitrary names into shell programs when the operation can use a stable tmux object ID.
4. Guard the global cleanup hook so unrelated sessions do not trigger ezm cleanup. Use both ownership/context validation and safe quoting; the guard is not a substitute for quoting.
5. Add the ordinary ezm-context guard to popup-open routing. Preserve the existing special handling when already inside a popup helper session.
6. Reconcile previously installed ezm hooks and keybindings on startup. Remove or replace only positively identified ezm-owned entries; preserve user hooks and unrelated bindings.
7. If a shared quoting helper is needed, keep its contract narrow and name the specific parsing layer it handles. Do not use one generic escape function for incompatible shell/tmux contexts.

### Regression tests

- Crafted session names containing command substitutions, backticks, quotes, backslashes, spaces, and shell metacharacters cannot cause a side effect when the session closes.
- Use an isolated tmux option or private sentinel as the side effect; never execute a destructive payload.
- Cleanup still removes the correct helper sessions for legitimate ezm parents.
- Closing an unrelated session leaves its panes, sessions, and processes alone.
- Popup keybinding in an ordinary session does not invoke ezm runtime operations.
- A legitimate popup still closes and reopens normally after the guard change.

Acceptance: the exact class of injection demonstrated during the audit fails to execute, without disabling legitimate cleanup or popup behavior.

## 7. B2: Remote Inputs and Safe Diagnostics

Primary files: `src/session/tmux/remote_authority.rs`, `remote_transport.rs`, `command.rs`, `src/app.rs`, mode launch builders, popup remote launch builders.

### B2.1. Authority parsing and option boundaries

1. Define supported authority forms explicitly: host, host with port, user-prefixed host, URL authority, and bracketed IPv6 as already supported.
2. Reject option-like destinations beginning with `-`, malformed usernames, control characters, and unsupported userinfo instead of passing them through to a transport.
3. Distinguish an SSH username from URL password userinfo. Unless a documented behavior requires password-bearing SSH authorities, reject them with a redacted error rather than forwarding `user:password@host` as a destination.
4. Add an end-of-options delimiter where supported by each actual transport invocation. Verify SSH, tssh, and mosh separately; their option and nested-command boundaries are not interchangeable.
5. Keep transport arguments structured for as long as practical. Serialize to shell text only at the boundary that actually requires it.
6. Validate resolved remote authority early enough that invalid settings cannot create a partially initialized session first.

Tests: reject leading-option cases such as `-F...` and `-oProxyCommand=...`; retain valid hostname, username, port, IPv6, and remote directory quoting behavior across the supported transports. No test should contact a real remote host.

### B2.2. Central redaction

1. Establish a small safe-rendering boundary for URLs and command diagnostics. Reuse the existing authority redaction behavior where correct rather than building a second inconsistent parser.
2. Remove raw credential-bearing URLs from verbose summaries. Render the non-secret destination or redact userinfo while retaining useful host/port information.
3. Review command failures, startup tracing, launch logs, and error reasons for password or userinfo leakage, including nested attach commands.
4. Prefer logging structured operation details to logging entire shell scripts. Preserve useful exit status and stderr while redacting known secret fields and values.
5. Keep executable command data separate from redacted diagnostic data; never accidentally execute a redacted command or log a raw command through a fallback path.

Tests: unique fake secret values must be absent from stdout, stderr, verbose output, tmux command diagnostics, and launch artifacts for both successful and failed launches. Include malformed URLs and errors raised before normal launch.

Acceptance: supported remote routing remains intact, option-like inputs fail early, and no tested diagnostic path exposes configured credentials.

## 8. C: Session-Scoped Runtime Configuration

Primary files: `src/config.rs`, `src/app.rs`, `src/session/runtime.rs`, `src/session/tmux/remote_env.rs`, `keybinds.rs`, `auxiliary.rs`, mode runtime and popup context modules.

### C1. Establish one resolved runtime boundary

1. Trace startup and every internal command to identify which values come from CLI, config files, process environment, session metadata, or global tmux environment.
2. Document the current precedence rules before changing them. Preserve documented user-facing precedence while preventing another project's inherited environment from becoming an unintended override.
3. Reuse or minimally extend the existing resolved runtime/context types. Include remote path, parsed destination or validated authority, transport selection, and shared-server settings at an appropriate boundary.
4. Pass the resolved context into tmux operations. Lower-level modules must not reread process environment to reinterpret values that were already resolved.
5. Persist only the information required by later internal commands and background launches. Choose per-session options/environment or explicit invocation context based on actual tmux inheritance behavior; test that behavior rather than assuming `run-shell` inherits the desired environment.
6. Ensure keybinding and background commands explicitly resolve the parent project session's context, including when invoked inside helper sessions.
7. Replace server-global `set-environment -g` writes for project-specific state. An unconfigured project must not unset another project's values.

### C2. Credential delivery and existing sessions

1. Verify which secret-delivery methods the supported OpenCode attach command accepts. Prefer a narrowly scoped child-process environment or another supported non-argv mechanism.
2. Do not replace a global tmux secret with a secret embedded in a keybinding, pane option, command-line argument, or loggable launch script.
3. If the supported upstream interface forces a tradeoff, document it and obtain a decision before introducing an insecure fallback. A private file mechanism requires explicit lifetime, permissions, cleanup, and crash-recovery rules.
4. Define how sessions created by the old version obtain scoped context on first use. Avoid silently changing a live project's destination when another project launches.
5. Do not indiscriminately clear globally named variables that may be user-managed. Remove legacy ezm-owned state only when ownership/provenance is known, or document a deliberate migration step.

### C3. Fix auxiliary routing

Pass the resolved remote context into auxiliary viewer startup and internal auxiliary commands. Remove the direct `std::env::var` reads of remote path/server from `auxiliary.rs`. Preserve local executable discovery when routing is inactive, and preserve existing behavior when a viewer window is already open.

### Regression matrix

| Scenario | Expected result |
| --- | --- |
| Projects A and B have distinct remote settings | New panes and internal commands use their own project's destination |
| B has no remote configuration | A remains unchanged |
| Config-only remote setup | Shell, mode, popup, and auxiliary flows agree |
| Explicit environment override for one launch | Documented precedence applies only to the intended project context |
| Existing session reopened after another project starts | Original scoped configuration is preserved or deliberately reconciled |
| Popup or mode helper invokes an internal action | Parent project's context is used |
| Shared-server password configured | Credential is not stored in global tmux environment or diagnostics |

Acceptance: one authoritative resolution path feeds all launch modes, and isolated two-project tests prove there is no cross-project contamination.

## 9. D: Canonical State, Layouts, and Failure Recovery

### D1. Canonical window ownership

Primary files: `src/session/tmux/command.rs`, repair metadata/geometry, layout and styling call sites.

1. Replace the active-window interpretation of `tmux_primary_window_target` with an explicit canonical-workspace resolver.
2. Prefer an owned persisted window ID, or recover by locating canonical panes. Do not rely exclusively on slot 1 if repairing a missing slot 1 is a supported case.
3. Validate that a persisted window ID belongs to the intended session and represents the managed workspace. Define behavior for deleted windows and stale metadata.
4. Distinguish callers that truly want the active window from callers that want the canonical window. Rename helpers to make the distinction unambiguous.
5. Review the legacy window-zero fallback: it must not silently redirect a canonical operation to an arbitrary active auxiliary window.

Tests: select the auxiliary window and an ordinary extra window before repair, styling, and preset changes; only the managed workspace is modified. Include nonzero base-index and stale canonical metadata cases.

### D2. Suspended versus damaged slots

Primary files: `src/session/repair.rs`, `src/session/tmux/repair/*`, `src/session/tmux/layout/pane_mode.rs`, `layout/preset.rs`, slot registry validation.

1. Model intentional suspension as part of damage analysis input, using the existing layout mode and suspension metadata.
2. Validate suspension against the declared layout. Do not let a stray suspended flag hide an actually damaged required pane.
3. Ordinary repair must restore only required damaged slots and preserve the user's active pane count and layout intent.
4. Explicit preset restoration may revive suspended panes. Use a clear operation distinction, preferably existing types or a narrow policy parameter, rather than conflating all restoration with damage repair.
5. Before measuring or resizing a three-pane layout, recreate the slots the target layout requires. Handle existing suspension metadata idempotently.
6. Confirm the semantics of the three-pane toggle from each starting layout. Retain documented three-to-five behavior unless deliberately changed and documented.
7. Validate pane-level mode metadata against canonical session mode where invariant validation is expected. Do not claim metadata validation proves which executable is actually running.
8. Preserve healthy pane identity, worktree assignment, and running processes during selective repair.

Tests: for starting pane counts 1 through 5, repair preserves healthy layout; damage to an active pane is repaired without reviving intentionally suspended slots; preset transitions end with consistent counts and metadata; repeated repair is a no-op; pane/session mode disagreement is reported or reconciled deliberately.

### D3. Bootstrap rollback

Primary files: `src/session/runtime.rs`, `src/session/tmux/layout.rs`, runtime fake-client tests.

1. Track whether the current invocation created the session. Never roll back a pre-existing user's session after an attach or validation error.
2. If bootstrap fails after creation, remove the newly created session and any positively identified resources created as part of that bootstrap.
3. Preserve the original bootstrap error. If cleanup also fails, include cleanup context without hiding the original cause.
4. Verify a subsequent launch can start cleanly after successful rollback.

Tests: injected failure at bootstrap stages removes only newly created resources; cleanup failure reports both errors; pre-existing sessions are never deleted by this failure path.

### D4. Single attach attempt

Primary file: `src/session/tmux/attach.rs`.

Remove the second `attach-session` execution used to gather diagnostics. Either capture the original child's diagnostics in a way compatible with interactive terminal behavior or report its original exit status and already-visible stderr. Preserve interruption behavior and child cleanup.

Tests: fake tmux records exactly one attach invocation on nonzero exit; success and interruption retain expected exit behavior; a real PTY attach remains functional.

## 10. E: CI and Release Integrity

Primary files: `.github/workflows/ci-quality-gate.yml`, `.github/workflows/release.yml`, release scripts, `mise.toml`, `Cargo.toml`, release notes template.

### E1. Required verification jobs

1. Keep formatting, strict Clippy, and the source-structure audit.
2. Add a fast locked library test job and real integration/E2E jobs with tmux installed explicitly.
3. Exercise Linux and macOS. Record Rust and tmux versions in the job output and evidence metadata.
4. Run the core suite rather than excluding `core_session_e2e_suite`. Add explicit jobs or test commands so one early failure does not prevent all other suites from being exercised.
5. Upload E2E artifacts on failure, excluding temporary executable secrets or private credential files. Print the artifact path in failures.
6. Remove the broad retry-on-any-test-failure pattern. If a platform has a specific transient external problem, isolate and justify that handling instead of masking product/test failures.
7. Add a locked MSRV check using the declared minimum toolchain, preferably including the intended test-target compatibility scope. The local audit verified 1.85.1; verify the exact declared minimum before claiming coverage.
8. Use CI concurrency cancellation for superseded PR runs if appropriate, without cancelling release publication halfway through.

Acceptance: PRs cannot pass with failing required tests, and releases depend on the same meaningful verification rather than a weaker skipped suite.

### E2. Release version and artifact checks

1. Resolve release inputs as data through environment variables rather than interpolating untrusted values directly into shell source.
2. Validate the tag format and compare its version to the ez-mux package version from `cargo metadata --no-deps`. Verify the checked-out revision is the intended release ref.
3. Reuse a single validated version value for archive names, checksums, verification metadata, and npm package generation.
4. Inspect the existing smoke/install wrappers before wiring them in. Run compatible artifacts natively on Linux/macOS; do not pretend a cross-compiled artifact was executed on an incompatible runner.
5. Test archive contents, executable permissions, `ezm --version`, installation to a temporary prefix, and uninstall/cleanup where supported.
6. Review the disconnected release evidence assembler. Either make it part of the release's actual enforced dependency graph or document it as a manual tool. Remove obsolete requirements only after confirming nothing consumes them.
7. Include the uploaded verification JSON in the release notes template and ensure documented artifact names match publication.

Acceptance: a mismatched tag is rejected before publication; native smoke checks run; release notes, archives, npm metadata, and verification artifacts identify the same version.

## 11. F: Modularity and Portfolio Presentation

### F1. Targeted code cleanup

1. Consolidate executable-hint normalization repeated in `session/runtime.rs`, `tmux/layout.rs`, and `tmux/keybinds.rs`. First compare semantics and tests; preserve intentionally different behavior rather than forcing superficially similar functions together.
2. Consolidate zoom-capability detection and retry eligibility shared by slot swapping and persistent mode switching. Keep the actual operations separate if their state transitions differ.
3. Remove empty forwarding/message helpers only when they add no boundary and do not support a useful test seam. Avoid replacing simple calls with more abstractions.
4. Review obsolete legacy handling against persisted or shipped behavior. Remove only branches with no concrete remaining consumer; document retained migration paths.
5. Correct stale references in scripts, including the structure audit's reference to a missing planning document. Ensure the audit describes what it actually measures, including inline test code if counted.
6. Re-run tests immediately after each extraction. Do not combine mechanical changes with unrelated behavior fixes.

Acceptance: fewer duplicated decisions, clearer runtime/configuration ownership, and no arbitrary proliferation of modules or helpers. File-size warnings are review prompts, not mandatory split instructions.

### F2. README and package metadata

Primary files: `README.md`, `Cargo.toml`; add a small focused development document only if the README would become unwieldy.

1. Keep the product's agent/worktree differentiation, but remove unnecessary repetition between the opening sections.
2. Add a real terminal screenshot or short demonstration showing launch, focus/mode switching, and a reduced layout. Use sanitized project names and no visible credentials. Prefer repository-owned assets with useful alt text; do not fabricate a runtime screenshot.
   Implemented asset: `docs/assets/ezm-terminal-demo.gif`.
3. Link to actual releases and list supported archive platforms. Confirm any npm distribution is actually published before documenting an install command.
4. Provide a concrete quick-start path, including prerequisites and expected default behavior when OpenCode is unavailable.
5. Separate mandatory tools from optional integrations: tmux, Git as required by actual code paths, configured shell behavior, agent tools, perles, neovim, lazygit, SSH, mosh, and tssh. State which missing tools are graceful and which features fail.
6. State the minimum supported tmux version selected during compatibility verification. Do not guess it based only on the installed version.
7. Document configuration precedence, session-scoped runtime behavior, remote routing versus OpenCode shared-server attachment, and how changes affect already-running sessions.
8. Document that repository-local `agent_command` is executable code. Make the trust boundary explicit before users run ezm in an unfamiliar checkout. A prompt/trust database is not automatically part of this pass; ask for a product decision if stronger trust enforcement is needed.
9. Add development setup and exact verification commands, test prerequisites, E2E evidence locations, and a brief map of app/config/session/tmux boundaries.
10. Add verified repository metadata to `Cargo.toml`; avoid unnecessary keywords, badges, or generic governance files. A badge should reflect a real enforced workflow.
11. Explain the separate zoomed-mode scenario/test entrypoint if it remains intentionally separate from the core suite.

Acceptance: a new contributor can install or build, understand optional tools, run the same meaningful checks as CI, and locate the main runtime boundaries without prior project history.

## 12. Follow-up Decisions, Not Automatic Scope Expansion

The audit raised additional candidates that should not be mislabeled as confirmed high-severity defects or silently change product behavior:

- Persistent popup cwd: determine whether reopening should preserve the helper shell's state or follow the parent pane's latest cwd. Do not destroy an active persistent shell merely to refresh metadata. Document the chosen contract and add a test; ask if the intended behavior is unclear.
- Repository-local executable config: clear documentation is included above. An explicit trust prompt/store requires a separate UX decision, especially for non-interactive use.
- Log privacy: review whether private Unix file modes are warranted by the actual contents and fallback locations. If changed, create files privately without blindly changing permissions on user-owned parent directories.
- Lexical remote path traversal: establish whether inputs can legitimately contain parent components and whether the mapping is a convenience or a containment boundary. Add normalization/validation only with a defined contract.
- Hidden internal commands: validate ownership to prevent accidental operations on unrelated sessions, but do not treat same-user CLI visibility as a separate privilege boundary or add a token system without a concrete threat model.

These checks may result in small follow-ups, but must not delay the confirmed safety, correctness, and CI fixes through speculative redesign.

## 13. Final Verification and Definition of Done

Run the following from a cleanly understood worktree after all implementation packages land:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --locked --lib
cargo test --locked
python3 scripts/audit_runtime_file_sizes.py
```

Also run the explicit MSRV job, platform E2E matrix, isolated adversarial command tests, and release smoke/install checks added during this plan. A source-size warning is acceptable when reviewed and justified; a failing behavior test is not.

Final review checklist:

- [x] Dynamic tmux values cannot become shell code through hooks or keybindings.
- [x] Remote authority inputs cannot become local transport options.
- [x] Passwords and URL credentials are absent from diagnostic artifacts and global tmux state created by ezm.
- [x] Two project sessions retain independent runtime configuration.
- [x] Config-file-only remote behavior is consistent across modes, popup, and auxiliary viewer.
- [x] Canonical operations target the managed workspace regardless of the selected window.
- [x] Reduced layouts survive ordinary repair and support defined preset transitions.
- [x] Bootstrap failures roll back only newly created resources.
- [x] Failed attach is not executed a second time for diagnostics.
- [ ] E2E tests run on normal macOS and Linux environments with isolated sockets.
- [x] Popup tests exercise an actual attached client.
- [ ] Required CI and release jobs execute the core suite without broad skip/retry masking.
- [ ] Release version, package metadata, archives, and verification evidence agree.
- [x] Shared helpers remove real duplication without increasing architectural complexity.
- [x] README installation, requirements, trust boundary, and development commands are accurate.
- [x] Final engineer review reports no unresolved high-priority findings; remaining limitations are explicit.

`[ ]` items are implemented/configured but require platform or external CI execution that was not performed locally. The local certification below is the source of truth for completed checks; it does not substitute for the configured CI matrix.

The final implementation report should summarize outcomes by these criteria, list tests actually executed, identify any platform checks performed only in CI, and link to remaining follow-up issues. Do not claim the project is fully verified merely because formatting and Clippy pass.

## 14. Implementation Report

### Phase A — Harness and evidence

- Short, owned tmux socket paths and failure-safe cleanup were added to the E2E harnesses.
- Keybinding predicates now parse the requested table/key instead of relying on fragile single-key output matching, and popup scenarios use an attached client for visibility checks.
- Prior certification recorded core evidence **17/17**, foundation **6/6**, and smoke **4/4**. The final harness run left **zero owned sockets and processes**.

### Phase B — tmux boundaries and remote safety

- Dynamic tmux values are quoted and ownership/context guards prevent unrelated sessions from triggering ezm cleanup or popup actions.
- Remote authorities reject option-like or malformed inputs, transport argument boundaries are explicit, and credential-bearing diagnostics are redacted.
- Hook, keybinding, authority, transport, and diagnostic regression coverage was added without contacting real remote hosts.

### Phase C — Session-scoped runtime state

- Resolved runtime context now follows the project session through mode, popup, auxiliary, and background launch paths.
- Project-specific remote settings no longer bleed through server-global environment state, and shared-server credentials are not persisted in global tmux state.

### Phase D — Canonical state and recovery

- Canonical workspace selection is independent of the currently selected auxiliary window.
- Repair distinguishes intentional suspension from damage, preserves healthy pane identity, and handles layout/preset transitions consistently.
- Bootstrap rollback is limited to resources created by the failed invocation, and failed attach diagnostics no longer execute a second attach.

### Phase E — CI and release integrity

- CI and release workflows now define locked test, MSRV, E2E, native verification, evidence, version, and publication gates with independent results and no broad retry masking.
- Release scripts validate refs and versions, verify publishable archives, assemble evidence, and test workflow contracts. These workflows remain configured for CI; their external execution is not claimed here.

### Phase F — Documentation and presentation

- README installation, prerequisites, optional integrations, configuration precedence, trust boundary, development commands, E2E evidence, and platform support were updated.
- The real repository-owned demo asset is `docs/assets/ezm-terminal-demo.gif`.
- Repository metadata and the runtime source audit reference were corrected; targeted duplication cleanup was completed where behavior and test seams permitted.

### Phase G — Final local certification

- Environment: **macOS arm64**, **Rust/Cargo 1.85.1**, **tmux 3.6a**.
- `cargo fmt --all -- --check`: pass.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: pass.
- Library tests: **387 passed**; final `cargo test --locked`: **467 passed**.
- Full locked MSRV tests on Rust/Cargo 1.85.1: pass.
- `python3 scripts/audit_runtime_file_sizes.py`: pass with warnings and no hard stops.
- Release/workflow tests: **12 passed**.
- Platform-only gaps remain: **Linux**, **macOS x86**, the **tmux floor**, **tmux 3.7b**, **mosh**, and **native cross-release** checks are CI-configured but were not locally executed. No external CI run is represented as complete.

### Dependency advisory resolution

`RUSTSEC-2026-0009` is fully resolved by removing the `time` dependency. `cargo tree -i time` confirms that no package depends on it. Rust **1.85.1** remains the MSRV; it was not raised to adopt the first patched `time` release, **0.3.47**, which requires Rust **1.88**.

No commits or pushes were made.
