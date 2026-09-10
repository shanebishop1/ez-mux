#!/usr/bin/env bash

set -euo pipefail

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd -P)"
readonly OUTPUT_PATH="${1:-$REPO_ROOT/target/demo/ez-mux-real-terminal-demo.gif}"

case "$OUTPUT_PATH" in
  *.gif) ;;
  *)
    printf 'error: output path must end in .gif: %s\n' "$OUTPUT_PATH" >&2
    exit 2
    ;;
esac

readonly OUTPUT_DIR="$(dirname -- "$OUTPUT_PATH")"
readonly OUTPUT_STEM="${OUTPUT_PATH%.gif}"
readonly SOURCE_VIDEO="${OUTPUT_STEM}-source.mp4"
readonly CONTACT_SHEET="${OUTPUT_STEM}-contact-sheet.png"
readonly EVIDENCE_DIR="${OUTPUT_STEM}-evidence"
readonly ZOOM_EVIDENCE="$EVIDENCE_DIR/tmux-zoom-transitions.txt"

require_program() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf 'error: required program not found: %s\n' "$1" >&2
    exit 127
  fi
}

resolve_program() {
  local program="$1"
  local resolved
  resolved="$(command -v "$program")"
  if [[ "$resolved" == */mise/shims/* ]] && command -v mise >/dev/null 2>&1; then
    local managed
    if managed="$(mise which "$program" 2>/dev/null)"; then
      resolved="$managed"
    elif managed="$(PATH=/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin command -v "$program")"; then
      resolved="$managed"
    else
      printf 'error: could not resolve real executable behind shim: %s\n' "$program" >&2
      exit 127
    fi
  fi
  printf '%s\n' "$resolved"
}

for program in bash cargo ffmpeg ffprobe git nvim opencode tmux ttyd vhs; do
  require_program "$program"
done

for artifact in "$OUTPUT_PATH" "$SOURCE_VIDEO" "$CONTACT_SHEET" "$EVIDENCE_DIR"; do
  if [[ -e "$artifact" || -L "$artifact" ]]; then
    printf 'error: refusing to overwrite existing demo artifact: %s\n' "$artifact" >&2
    exit 2
  fi
done

mkdir -p -- "$OUTPUT_DIR"
mkdir -p -- "$EVIDENCE_DIR"

readonly REAL_TMUX="$(resolve_program tmux)"
readonly REAL_VHS="$(resolve_program vhs)"
readonly REAL_TTYD="$(resolve_program ttyd)"
readonly REAL_FFMPEG="$(resolve_program ffmpeg)"
readonly REAL_FFPROBE="$(resolve_program ffprobe)"
readonly REAL_OPENCODE="$(resolve_program opencode)"
readonly REAL_NVIM="$(resolve_program nvim)"
readonly REAL_GIT="$(resolve_program git)"
readonly REAL_BASH="$(resolve_program bash)"

printf 'Building the release binary...\n'
cargo build --release --locked --bin ezm --manifest-path "$REPO_ROOT/Cargo.toml"

readonly RUN_ROOT="$(mktemp -d "$OUTPUT_DIR/ez-mux-demo.XXXXXX")"
readonly PRIVATE_BIN="$RUN_ROOT/bin"
readonly PRIVATE_HOME="$RUN_ROOT/home"
readonly PRIVATE_TMP="$RUN_ROOT/tmp"
readonly VHS_CAPTURE="$RUN_ROOT/vhs-capture.mp4"
readonly PRIVATE_CONFIG="$RUN_ROOT/config"
readonly PRIVATE_DATA="$RUN_ROOT/data"
readonly PRIVATE_STATE="$RUN_ROOT/state"
readonly WORKTREE_ROOT="$PRIVATE_HOME/worktrees"
readonly PROJECT_DIR="$WORKTREE_ROOT/ezm-demo-5"
readonly TMUX_DIR="$RUN_ROOT/tmux"
readonly TMUX_SOCKET="$TMUX_DIR/demo.sock"

cleanup() {
  local exit_code=$?
  if [[ -S "$TMUX_SOCKET" ]]; then
    EZM_DEMO_REAL_TMUX="$REAL_TMUX" EZM_DEMO_TMUX_CONFIG="$TMUX_DIR/tmux.conf" \
      EZM_DEMO_TMUX_SOCKET="$TMUX_SOCKET" \
      "$PRIVATE_BIN/tmux" kill-server >/dev/null 2>&1 || true
  fi
  pkill -TERM -f "$PRIVATE_TMP/rod/user-data/" >/dev/null 2>&1 || true
  sleep 0.1
  pkill -KILL -f "$PRIVATE_TMP/rod/user-data/" >/dev/null 2>&1 || true
  for _ in 1 2 3 4 5; do
    rm -rf -- "$RUN_ROOT" 2>/dev/null || true
    [[ ! -e "$RUN_ROOT" ]] && break
    sleep 0.25
  done
  if [[ -e "$RUN_ROOT" ]]; then
    printf 'warning: private run directory resisted cleanup: %s\n' "$RUN_ROOT" >&2
  fi
  exit "$exit_code"
}
trap cleanup EXIT INT TERM

mkdir -p -- \
  "$PRIVATE_BIN" "$PRIVATE_HOME" "$PRIVATE_TMP" "$PRIVATE_CONFIG" \
  "$PRIVATE_DATA" "$PRIVATE_STATE" "$WORKTREE_ROOT" "$TMUX_DIR"
chmod 700 "$RUN_ROOT" "$PRIVATE_HOME" "$PRIVATE_TMP" "$PRIVATE_CONFIG" \
  "$PRIVATE_DATA" "$PRIVATE_STATE" "$TMUX_DIR"

install -m 755 "$REPO_ROOT/target/release/ezm" "$PRIVATE_BIN/ezm"
ln -s "$REAL_VHS" "$PRIVATE_BIN/vhs"
ln -s "$REAL_TTYD" "$PRIVATE_BIN/ttyd"
ln -s "$REAL_FFMPEG" "$PRIVATE_BIN/ffmpeg"
ln -s "$REAL_NVIM" "$PRIVATE_BIN/nvim"
ln -s "$REAL_GIT" "$PRIVATE_BIN/git"
ln -s /usr/bin/basename "$PRIVATE_BIN/basename"
ln -s /usr/bin/clear "$PRIVATE_BIN/clear"
ln -s "$REAL_BASH" "$PRIVATE_BIN/bash"
ln -s /bin/sh "$PRIVATE_BIN/sh"

cat >"$PRIVATE_BIN/tmux" <<'EOF'
#!/bin/sh
set -eu
: "${EZM_DEMO_REAL_TMUX:?missing private tmux executable}"
: "${EZM_DEMO_TMUX_SOCKET:?missing private tmux socket}"
: "${EZM_DEMO_TMUX_CONFIG:?missing private tmux config}"
exec "$EZM_DEMO_REAL_TMUX" -S "$EZM_DEMO_TMUX_SOCKET" -f "$EZM_DEMO_TMUX_CONFIG" "$@"
EOF
chmod 755 "$PRIVATE_BIN/tmux"

cat >"$TMUX_DIR/tmux.conf" <<'EOF'
set-option -g status-left ' ez-mux '
set-option -g status-right ' five worktrees · real TUIs '
set-option -g status-interval 0
set-option -g set-titles off
set-option -g default-size 200x60
set-window-option -g automatic-rename off
set-window-option -g allow-rename off
EOF
chmod 600 "$TMUX_DIR/tmux.conf"

cat >"$PRIVATE_BIN/opencode" <<'EOF'
#!/bin/sh
set -eu
: "${EZM_DEMO_REAL_OPENCODE:?missing real OpenCode executable}"
: "${EZM_DEMO_OPENCODE_ROOT:?missing isolated OpenCode root}"
slot="${OPENCODE_CONFIG_DIR##*/}"
case "$slot" in
  slot-[1-5]) ;;
  *) slot="shared" ;;
esac
export XDG_CACHE_HOME="$EZM_DEMO_OPENCODE_ROOT/$slot/cache"
export XDG_DATA_HOME="$EZM_DEMO_OPENCODE_ROOT/$slot/data"
export XDG_STATE_HOME="$EZM_DEMO_OPENCODE_ROOT/$slot/state"
export SHELL=/bin/sh
exec "$EZM_DEMO_REAL_OPENCODE" "$@"
EOF
chmod 755 "$PRIVATE_BIN/opencode"

cat >"$PRIVATE_BIN/demo-login-shell" <<'EOF'
#!/bin/sh
exec /bin/sh "$@"
EOF
chmod 755 "$PRIVATE_BIN/demo-login-shell"

cat >"$PRIVATE_HOME/.profile" <<'EOF'
PS1='demo $ '
export PS1
printf '\033]2;ez-mux demo\033\\'
EOF
chmod 600 "$PRIVATE_HOME/.profile"

cat >"$PRIVATE_HOME/.bashrc" <<'EOF'
cd -- "${EZM_DEMO_PROJECT:?missing demo project}"
PS1='demo $ '
export PS1
EOF
chmod 600 "$PRIVATE_HOME/.bashrc"

cat >"$PRIVATE_CONFIG/opencode.json" <<'EOF'
{
  "$schema": "https://opencode.ai/config.json",
  "autoupdate": false,
  "share": "disabled",
  "snapshot": false,
  "enabled_providers": ["offline-demo"],
  "model": "offline-demo/no-requests",
  "small_model": "offline-demo/no-requests",
  "provider": {
    "offline-demo": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "offline",
      "options": {
        "baseURL": "http://127.0.0.1:1/v1",
        "apiKey": "not-a-credential"
      },
      "models": {
        "no-requests": {
          "name": "unsent"
        }
      }
    }
  },
  "mcp": {},
  "plugin": []
}
EOF
chmod 600 "$PRIVATE_CONFIG/opencode.json"

"$REAL_GIT" init --initial-branch=slot-5 "$PROJECT_DIR" >/dev/null
"$REAL_GIT" -C "$PROJECT_DIR" config user.name "Ezm Demo"
"$REAL_GIT" -C "$PROJECT_DIR" config user.email "demo@example.invalid"
cat >"$PROJECT_DIR/README.md" <<'EOF'
# ez-mux demo — slot 5

This harmless sample checkout exists only for the terminal recording.

- Worktree: ezm-demo-5
- Branch: slot-5
- Task: compare release notes
EOF
cat >"$PROJECT_DIR/ez-mux.toml" <<'EOF'
panes = 5
opencode_slot_themes_enabled = true

[opencode_slot_themes]
"1" = "nightowl"
"2" = "orng"
"3" = "osaka-jade"
"4" = "catppuccin"
"5" = "monokai"
EOF
"$REAL_GIT" -C "$PROJECT_DIR" add README.md ez-mux.toml
"$REAL_GIT" -C "$PROJECT_DIR" commit -m "Seed isolated demo" >/dev/null

for slot in 1 2 3 4; do
  worktree="$WORKTREE_ROOT/ezm-demo-$slot"
  "$REAL_GIT" -C "$PROJECT_DIR" worktree add -b "slot-$slot" "$worktree" >/dev/null
  cat >"$worktree/README.md" <<EOF
# ez-mux demo — slot $slot

This harmless sample checkout exists only for the terminal recording.

- Worktree: ezm-demo-$slot
- Branch: slot-$slot
- Task: isolated task stream $slot
EOF
  "$REAL_GIT" -C "$worktree" add README.md
  "$REAL_GIT" -C "$worktree" commit -m "Identify slot $slot" >/dev/null
done

"$REAL_GIT" -C "$PROJECT_DIR" worktree list --porcelain >"$EVIDENCE_DIR/git-worktrees.txt"

readonly OPENCODE_SAFE_CONFIG="$(<"$PRIVATE_CONFIG/opencode.json")"

for slot in 1 2 3 4 5; do
  mkdir -p -- \
    "$RUN_ROOT/opencode/slot-$slot/cache" \
    "$RUN_ROOT/opencode/slot-$slot/data" \
    "$RUN_ROOT/opencode/slot-$slot/state/opencode"
  printf '%s\n' '{"animations_enabled":false}' \
    >"$RUN_ROOT/opencode/slot-$slot/state/opencode/kv.json"
done

tmux_demo() {
  EZM_DEMO_REAL_TMUX="$REAL_TMUX" EZM_DEMO_TMUX_CONFIG="$TMUX_DIR/tmux.conf" \
    EZM_DEMO_TMUX_SOCKET="$TMUX_SOCKET" \
    "$PRIVATE_BIN/tmux" "$@"
}

readonly SAFE_PATH="$PRIVATE_BIN:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
DEMO_ENV=(
  "HOME=$PRIVATE_HOME"
  "PATH=$SAFE_PATH"
  "SHELL=$PRIVATE_BIN/demo-login-shell"
  "TERM=xterm-256color"
  "TMPDIR=$PRIVATE_TMP"
  "TMUX_TMPDIR=$TMUX_DIR"
  "USER=ezm-demo"
  "LOGNAME=ezm-demo"
  "XDG_CACHE_HOME=$RUN_ROOT/cache"
  "XDG_CONFIG_HOME=$PRIVATE_CONFIG"
  "XDG_DATA_HOME=$PRIVATE_DATA"
  "XDG_STATE_HOME=$PRIVATE_STATE"
  "EZM_CONFIG=$PROJECT_DIR/ez-mux.toml"
  "EZM_BIN=$PRIVATE_BIN/ezm"
  "EZM_DEMO_PROJECT=$PROJECT_DIR"
  "EZM_DEMO_OPENCODE_ROOT=$RUN_ROOT/opencode"
  "EZM_DEMO_REAL_OPENCODE=$REAL_OPENCODE"
  "EZM_DEMO_REAL_TMUX=$REAL_TMUX"
  "EZM_DEMO_TMUX_CONFIG=$TMUX_DIR/tmux.conf"
  "EZM_DEMO_TMUX_SOCKET=$TMUX_SOCKET"
  "OPENCODE_CONFIG=$PRIVATE_CONFIG/opencode.json"
  "OPENCODE_CONFIG_CONTENT=$OPENCODE_SAFE_CONFIG"
  "OPENCODE_AUTO_SHARE=false"
  "OPENCODE_DISABLE_AUTOUPDATE=true"
  "OPENCODE_DISABLE_CLAUDE_CODE=true"
  "OPENCODE_DISABLE_DEFAULT_PLUGINS=true"
  "OPENCODE_DISABLE_LSP_DOWNLOAD=true"
  "OPENCODE_DISABLE_MODELS_FETCH=true"
  "OPENCODE_DISABLE_TERMINAL_TITLE=true"
  "OPENCODE_EXPERIMENTAL_DISABLE_FILEWATCHER=true"
)

printf 'Launching five real isolated OpenCode TUIs...\n'
(
  cd -- "$PROJECT_DIR"
  env -i "${DEMO_ENV[@]}" "$PRIVATE_BIN/ezm"
)

ready=0
for _ in $(seq 1 80); do
  live_tuis=0
  while IFS='|' read -r pane mode session; do
    if [[ "$mode" == "agent" && "$session" != *__mode_cache ]]; then
      if tmux_demo capture-pane -p -t "$pane" | command grep -Fq 'unsent'; then
        live_tuis=$((live_tuis + 1))
      fi
    fi
  done < <(tmux_demo list-panes -a -F '#{pane_id}|#{@ezm_slot_mode}|#{session_name}')
  if [[ "$live_tuis" -eq 5 ]]; then
    ready=1
    break
  fi
  sleep 0.5
done
if [[ "$ready" -ne 1 ]]; then
  printf 'error: five OpenCode TUIs did not become ready within 40 seconds\n' >&2
  exit 1
fi

readonly MAIN_SESSION="$(tmux_demo list-sessions -F '#{session_name}' | command awk '!/__mode_cache$/ {print; exit}')"
tmux_demo set-option -g @ezm_demo_zoom_events \
  "initial|$(tmux_demo display-message -p -t "$MAIN_SESSION" '#{window_zoomed_flag}|#{@ezm_slot_id}|#{@ezm_slot_mode}')"
tmux_demo set-hook -g after-resize-pane \
  "set-option -agF @ezm_demo_zoom_events ',resize|#{window_zoomed_flag}|#{@ezm_slot_id}|#{@ezm_slot_mode}'"
for slot in 2 3; do
  pane="$(tmux_demo list-panes -t "$MAIN_SESSION" -F '#{pane_id}|#{@ezm_slot_id}' | \
    command awk -F'|' -v wanted="$slot" '$2 == wanted {print $1; exit}')"
  tmux_demo bind-key -T prefix "$slot" select-pane -t "$pane"
done

printf 'Recording real isolated TUIs...\n'
env -i "${DEMO_ENV[@]}" \
  "$REAL_VHS" "$SCRIPT_DIR/demo.tape" -o "$VHS_CAPTURE"

printf 'Normalizing the source-video pacing...\n'
"$REAL_FFMPEG" -v error -i "$VHS_CAPTURE" -an \
  -vf 'setpts=1.75*PTS' -r 10 "$SOURCE_VIDEO"

printf 'Encoding the candidate GIF...\n'
"$REAL_FFMPEG" -v error -i "$SOURCE_VIDEO" \
  -filter_complex '[0:v]fps=15,split[frames][palette_source];[palette_source]palettegen=max_colors=192:stats_mode=diff[palette];[frames][palette]paletteuse=dither=sierra2_4a:diff_mode=rectangle' \
  -loop 0 "$OUTPUT_PATH"

sleep 1

tmux_demo show-options -gv @ezm_demo_zoom_events | \
  command tr ',' '\n' >"$ZOOM_EVIDENCE"

tmux_demo list-sessions -F '#{session_name}|#{session_windows}|#{@ezm_project_dir}' \
  >"$EVIDENCE_DIR/tmux-sessions.txt"
tmux_demo list-panes -a -F '#{session_name}|#{pane_id}|#{@ezm_slot_id}|#{@ezm_slot_mode}|#{@ezm_slot_worktree}|#{pane_current_command}|#{pane_pid}|#{window_zoomed_flag}' \
  >"$EVIDENCE_DIR/tmux-panes.txt"

while IFS='|' read -r session pane slot mode worktree command pid zoomed; do
  safe_pane="${pane#%}"
  tmux_demo capture-pane -p -e -t "$pane" >"$EVIDENCE_DIR/pane-${safe_pane}-${mode:-unknown}.ansi.txt"
done <"$EVIDENCE_DIR/tmux-panes.txt"

theme_root="$PRIVATE_TMP/ez-mux-ezm-demo/opencode-tui"
mkdir -p -- "$EVIDENCE_DIR/opencode-themes"
for slot in 1 2 3 4 5; do
  cp -- "$theme_root/slot-$slot/tui.json" "$EVIDENCE_DIR/opencode-themes/slot-$slot.json"
done

expected_themes=(nightowl orng osaka-jade catppuccin monokai)
verification_status=0
{
  printf 'release_binary=release ezm copied into private run directory\n'
  printf 'vhs=%s\n' "$("$REAL_VHS" --version 2>&1)"
  printf 'tmux=%s\n' "$("$REAL_TMUX" -V)"
  printf 'opencode=%s\n' "$("$REAL_OPENCODE" --version)"
  printf 'nvim=%s\n' "$("$REAL_NVIM" --version | command sed -n '1p')"
  printf 'private_socket=%s\n' "$TMUX_SOCKET"

  visible_agents="$(command awk -F'|' '$4 == "agent" && $1 !~ /__mode_cache$/ {count++} END {print count+0}' "$EVIDENCE_DIR/tmux-panes.txt")"
  printf 'visible_agent_panes=%s\n' "$visible_agents"
  if [[ "$visible_agents" -ne 5 ]]; then
    printf 'FAIL expected five visible agent panes\n'
    verification_status=1
  fi

  for slot in 1 2 3 4 5; do
    expected_theme="${expected_themes[$((slot - 1))]}"
    if ! command grep -Fq "\"theme\": \"$expected_theme\"" "$EVIDENCE_DIR/opencode-themes/slot-$slot.json"; then
      printf 'FAIL slot %s theme is not %s\n' "$slot" "$expected_theme"
      verification_status=1
    fi
    if ! command awk -F'|' -v slot="$slot" '$3 == slot && $4 == "agent" && $5 ~ ("ezm-demo-" slot "$") {found=1} END {exit !found}' "$EVIDENCE_DIR/tmux-panes.txt"; then
      printf 'FAIL slot %s lacks matching agent/worktree metadata\n' "$slot"
      verification_status=1
    fi
  done

  if ! command awk -F'|' '$3 == "2" && $4 == "neovim" {found=1} END {exit !found}' "$EVIDENCE_DIR/tmux-panes.txt"; then
    printf 'FAIL cached Neovim mode evidence is absent\n'
    verification_status=1
  fi
  if ! command awk -F'|' '$3 == "2" && $4 == "shell" {found=1} END {exit !found}' "$EVIDENCE_DIR/tmux-panes.txt"; then
    printf 'FAIL cached shell mode evidence is absent\n'
    verification_status=1
  fi

  if command grep -Eq 'Trace/BPT trap|mode tool opencode exited' "$EVIDENCE_DIR"/pane-*.ansi.txt; then
    printf 'FAIL an OpenCode TUI crashed during the recording\n'
    verification_status=1
  fi
  live_tui_captures="$({ command grep -El 'unsent|Ask anything|Audit this worktree|Compare this branch' "$EVIDENCE_DIR"/pane-*-agent.ansi.txt || true; } | command wc -l | command tr -d ' ')"
  printf 'live_opencode_tui_captures=%s\n' "$live_tui_captures"
  if [[ "$live_tui_captures" -ne 5 ]]; then
    printf 'FAIL expected five captured live OpenCode TUI screens\n'
    verification_status=1
  fi
  if ! command grep -Fq 'worktree: ezm-demo-2' "$EVIDENCE_DIR"/pane-*-shell.ansi.txt || \
    ! command grep -Fq 'branch:   slot-2' "$EVIDENCE_DIR"/pane-*-shell.ansi.txt; then
    printf 'FAIL shell capture does not identify slot 2 worktree and branch\n'
    verification_status=1
  fi
  if ! command grep -Fq 'ez-mux demo — slot 2' "$EVIDENCE_DIR"/pane-*-neovim.ansi.txt; then
    printf 'FAIL Neovim capture does not show the slot 2 README\n'
    verification_status=1
  fi
  if ! command grep -Fq 'Audit this worktree for TODOs' "$EVIDENCE_DIR"/pane-*-agent.ansi.txt; then
    printf 'FAIL slot 3 staged prompt is absent from the agent TUI captures\n'
    verification_status=1
  fi
  if ! command grep -Fq 'Compare this branch with main' "$EVIDENCE_DIR"/pane-*-agent.ansi.txt; then
    printf 'FAIL slot 5 staged prompt is absent from the agent TUI captures\n'
    verification_status=1
  fi
  if command grep -Fq 'Compare this branch with main' "$EVIDENCE_DIR"/pane-*-shell.ansi.txt; then
    printf 'FAIL slot 5 staged prompt was typed into a shell instead of OpenCode\n'
    verification_status=1
  fi

  zoom_flags="$(command awk -F'|' 'NR == 1 || $2 != previous {printf "%s", $2; previous = $2}' "$ZOOM_EVIDENCE")"
  printf 'sampled_zoom_flags=%s\n' "$zoom_flags"
  if [[ "$zoom_flags" != "01010" ]]; then
    printf 'FAIL expected exact zoom transition sequence 0->1->0->1->0\n'
    verification_status=1
  fi
  if ! command grep -Eq '^resize\|1\|3\|agent$' "$ZOOM_EVIDENCE"; then
    printf 'FAIL slot 3 agent zoom was not sampled\n'
    verification_status=1
  fi
  if ! command grep -Eq '^resize\|1\|5\|agent$' "$ZOOM_EVIDENCE"; then
    printf 'FAIL slot 5 agent zoom was not sampled\n'
    verification_status=1
  fi

  if [[ "$verification_status" -eq 0 ]]; then
    printf 'PASS isolated runtime, worktrees, modes, and themes verified\n'
  fi
} >"$EVIDENCE_DIR/verification.txt"

"$REAL_FFPROBE" -v error -count_frames -select_streams v:0 \
  -show_entries stream=codec_name,width,height,avg_frame_rate,nb_read_frames:format=duration,size \
  -of default=noprint_wrappers=1 "$SOURCE_VIDEO" >"$EVIDENCE_DIR/source-video.txt"
"$REAL_FFPROBE" -v error -count_frames -select_streams v:0 \
  -show_entries stream=codec_name,width,height,avg_frame_rate,nb_read_frames:format=duration,size \
  -of default=noprint_wrappers=1 "$OUTPUT_PATH" >"$EVIDENCE_DIR/candidate-gif.txt"
"$REAL_FFMPEG" -v error -i "$OUTPUT_PATH" -f null -
printf 'PASS ffmpeg decoded every GIF frame\n' >"$EVIDENCE_DIR/frame-decode.txt"

readonly KEY_SCENE_DIR="$EVIDENCE_DIR/key-scenes"
readonly SCENE_TIMES_FILE="$EVIDENCE_DIR/scene-times.txt"
scene_times=(8.00 5.00 16.00 20.00 22.00 24.80)
scene_slugs=(all-five zoom-slot-3 neovim-slot-2 shell-slot-2 restored-all zoom-slot-5)
scene_labels=(
  "all five themed OpenCode TUIs"
  "slot 3 agent zoom with staged prompt"
  "slot 2 Neovim README"
  "slot 2 shell worktree output"
  "restored five-agent workspace"
  "slot 5 agent zoom with staged prompt"
)

mkdir -p -- "$KEY_SCENE_DIR"
: >"$SCENE_TIMES_FILE"
scene_inputs=()
for index in "${!scene_times[@]}"; do
  scene_number="$(printf '%02d' "$((index + 1))")"
  scene_file="$KEY_SCENE_DIR/${scene_number}-${scene_slugs[$index]}.png"
  "$REAL_FFMPEG" -v error -n -i "$SOURCE_VIDEO" -ss "${scene_times[$index]}" \
    -frames:v 1 \
    -vf 'scale=800:450:force_original_aspect_ratio=decrease,pad=800:450:(ow-iw)/2:(oh-ih)/2:color=0x11111b' \
    "$scene_file"
  scene_inputs+=( -i "$scene_file" )
  printf '%s|%ss|%s|%s\n' \
    "$scene_number" "${scene_times[$index]}" "${scene_labels[$index]}" "$scene_file" \
    >>"$SCENE_TIMES_FILE"
done

"$REAL_FFMPEG" -v error -n "${scene_inputs[@]}" \
  -filter_complex 'xstack=inputs=6:layout=0_0|800_0|0_450|800_450|0_900|800_900:fill=0x11111b' \
  -frames:v 1 "$CONTACT_SHEET"

if [[ "$verification_status" -ne 0 ]]; then
  printf 'error: runtime evidence validation failed; see %s\n' "$EVIDENCE_DIR/verification.txt" >&2
  exit "$verification_status"
fi

printf '\nCandidate GIF: %s\n' "$OUTPUT_PATH"
printf 'Source video: %s\n' "$SOURCE_VIDEO"
printf 'Contact sheet: %s\n' "$CONTACT_SHEET"
printf 'Evidence: %s\n' "$EVIDENCE_DIR"
