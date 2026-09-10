Require ezm
Require git
Require nvim
Require opencode
Require tmux

Set Shell bash
Set Width 1600
Set Height 900
Set FontSize 14
Set FontFamily "Menlo"
Set Theme "Catppuccin Mocha"
Set Padding 8
Set Framerate 10
Set PlaybackSpeed 2.00
Set TypingSpeed 12ms
Set CursorBlink false

Hide
Type `cd "$EZM_DEMO_PROJECT" && export PS1='demo $ ' && printf '\033]2;ez-mux demo\033\\' && clear`
Enter
Sleep 500ms
Show

Type@35ms "ezm"
Sleep 700ms
Enter
Sleep 2s

# Five live OpenCode TUIs, one per real worktree and default slot theme.
Sleep 2s

# Focus slot 3, zoom its real TUI, and stage (but do not submit) a task.
Ctrl+B
Sleep 150ms
Type "3"
Sleep 1s
Ctrl+B
Sleep 150ms
Type "z"
Sleep 1s
Type@12ms "Audit this worktree for TODOs; summarize only."
Sleep 1500ms
Ctrl+B
Sleep 150ms
Type "z"
Sleep 700ms

# Focus slot 2 and switch the same stable slot into real Neovim mode.
Ctrl+B
Sleep 150ms
Type "2"
Sleep 1s
Ctrl+B
Sleep 150ms
Type "N"
Wait+Screen@15s /No Name/
Type@10ms ":edit README.md"
Enter
Wait+Screen@15s /ez-mux demo/
Sleep 700ms

# Switch slot 2 into a shell and prove its assigned checkout and branch.
Ctrl+B
Sleep 150ms
Type "S"
Wait+Screen@15s /demo \$/
Type@7ms `printf 'worktree: %s\nbranch:   %s\n' "$(basename "$PWD")" "$(git branch --show-current)"`
Enter
Wait+Screen@15s /worktree: ezm-demo-2/
Sleep 700ms

# Return slot 2 to its cached real OpenCode TUI.
Ctrl+B
Sleep 150ms
Type "a"
Sleep 5s

# Focus and zoom slot 5, stage a second independent task, then restore all panes.
Ctrl+B
Sleep 150ms
Type "f"
Sleep 150ms
Type "5"
Sleep 4s
Ctrl+B
Sleep 150ms
Type "z"
Sleep 1s
Type@12ms "Compare this branch with main; propose a safe patch."
Sleep 1500ms
Ctrl+B
Sleep 150ms
Type "z"
Sleep 700ms
Sleep 1s
