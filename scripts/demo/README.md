# Real terminal demo recording

`record.sh` builds the release `ezm` binary and records `demo.tape` with VHS. The demo is intentionally real: `ezm` discovers five temporary Git worktrees, launches five idle OpenCode TUIs, waits for them to be ready, and then VHS records a fast reattach plus actual tmux mode and zoom transitions. The shipped per-slot themes are applied by `ezm`. Typed OpenCode tasks are left unsent, so recording does not make model requests.

## Record

```sh
scripts/demo/record.sh
```

The default output is `target/demo/ez-mux-real-terminal-demo.gif`. An explicit GIF path may be passed as the first argument. The recorder refuses to overwrite an existing GIF, source video, contact sheet, or evidence directory.

Requirements: Bash, Cargo, Git, VHS 0.11+, ttyd, ffmpeg/ffprobe, tmux 3.2+, OpenCode, and Neovim.

The script also writes:

- `*-source.mp4`: source recording used for frame inspection.
- `*-contact-sheet.png`: six explicitly timed key scenes in a readable two-column sheet.
- `*-evidence/`: tmux metadata/captures, sampled `window_zoomed_flag` transitions, exact generated OpenCode theme files, timed scene frames, worktree inventory, and ffprobe/frame-decode reports.

## Isolation

- The temporary repository and all five worktrees live below the isolated `HOME`, so OpenCode renders checkout paths as `~/worktrees/ezm-demo-N`.
- A wrapper forces every tmux call onto one exact private socket with `-S` and a private, sanitized config; the user tmux server is never addressed.
- VHS and every pane run under `env -i` with private `HOME`, XDG directories, temp directory, shell startup, Git identity, and per-slot OpenCode storage/config.
- OpenCode credentials and provider environment variables are absent. A short `unsent` label backed by a non-routable loopback-only model keeps the real prompt UI available without implying any request ran; prompts are never submitted. Sharing, auto-update, model fetching, default plugins, LSP downloads, Claude config loading, snapshots, file watching, and TUI animations are disabled for the recording.
- Cleanup kills only the exact private tmux server and any recorder-owned headless Chrome processes below the private run directory, then removes that directory. Candidate media and sanitized evidence remain at the requested output stem.

The approximately 25-second final sequence is: reattach to five just-launched themed OpenCode TUIs; focus and zoom slot 3; type an unsent task; zoom out; focus slot 2; switch to Neovim and open its README; switch to shell and print the real worktree/branch; restore the cached OpenCode TUI; focus and zoom slot 5; type another unsent task; zoom out to the five-pane workspace.
