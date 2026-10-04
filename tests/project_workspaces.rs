#![allow(dead_code)]
mod support;
use std::fs;
use support::foundation_harness::{FoundationHarness, serial_test_guard};

#[test]
#[allow(clippy::too_many_lines)]
fn groups_are_independent_windows_and_reopen_preserves_panes() {
    let _guard = serial_test_guard();
    let h = FoundationHarness::new_for_suite("project-workspaces").unwrap();
    let root = h.work_dir().join("project");
    fs::create_dir_all(root.join("build")).unwrap();
    fs::create_dir_all(root.join("review")).unwrap();
    let registry = h.work_dir().join("projects.toml");
    fs::write(
        &registry,
        format!(
            r"
[projects.demo]
root = {root:?}
[projects.demo.groups.build]
[[projects.demo.groups.build.slots]]
directory = {build:?}
command = 'echo ready >> {ready}; exec sleep 600'
[projects.demo.groups.review]
[[projects.demo.groups.review.slots]]
directory = {review:?}
command = 'echo ready >> {ready}; exec sleep 600'
",
            ready = h.work_dir().join("ready").display(),
            root = root.to_str().unwrap(),
            build = root.join("build").to_str().unwrap(),
            review = root.join("review").to_str().unwrap()
        ),
    )
    .unwrap();
    let env = [("REMOTE_AGENTS_CONFIG", registry.to_str().unwrap())];
    for group in ["build", "review"] {
        let r = h
            .run_remote_agents_in_dir(
                &root,
                &["open", "demo", "--group", group, "--no-attach"],
                &env,
                0,
            )
            .unwrap();
        assert_eq!(r.exit_code, 0, "{}", r.stderr);
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while fs::read_to_string(h.work_dir().join("ready"))
        .unwrap_or_default()
        .lines()
        .count()
        < 2
    {
        assert!(std::time::Instant::now() < deadline, "agents did not start");
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let before = h
        .tmux_capture(&["list-panes", "-a", "-F", "#{pane_id}|#{pane_pid}"])
        .unwrap();
    let r = h
        .run_remote_agents_in_dir(
            &root,
            &["open", "demo", "--group", "build", "--no-attach"],
            &env,
            0,
        )
        .unwrap();
    assert_eq!(r.exit_code, 0, "{}", r.stderr);
    let after = h
        .tmux_capture(&["list-panes", "-a", "-F", "#{pane_id}|#{pane_pid}"])
        .unwrap();
    assert_eq!(before, after);
    let windows = h
        .tmux_capture(&[
            "list-windows",
            "-a",
            "-F",
            "#{session_name}|#{window_name}|#{@ezm_group_owner}",
        ])
        .unwrap();
    assert!(windows.contains("|build|"), "{windows}");
    assert!(windows.contains("|review|"), "{windows}");
    let line = windows
        .lines()
        .find(|line| line.starts_with("ezm-project-") && line.contains("|build|"))
        .unwrap();
    let parts: Vec<_> = line.split('|').collect();
    let parent = parts[0];
    let build_owner = parts[2];
    let review_owner = windows
        .lines()
        .find(|line| line.contains("|review|"))
        .unwrap()
        .split('|')
        .nth(2)
        .unwrap();
    let original_launch = h
        .tmux_capture(&[
            "show-options",
            "-qv",
            "-t",
            build_owner,
            "@ezm_runtime_agent_command",
        ])
        .unwrap();
    h.tmux_capture(&[
        "set-option",
        "-t",
        parent,
        "@ezm_workspace_project",
        "other",
    ])
    .unwrap();
    let conflict = h
        .run_remote_agents_in_dir(
            &root,
            &["open", "demo", "--group", "build", "--no-attach"],
            &env,
            0,
        )
        .unwrap();
    assert_ne!(conflict.exit_code, 0);
    assert!(conflict.stderr.contains("identity conflict"));
    assert_eq!(
        h.tmux_capture(&["list-panes", "-a", "-F", "#{pane_id}|#{pane_pid}"])
            .unwrap(),
        before
    );
    assert_eq!(
        h.tmux_capture(&[
            "show-options",
            "-qv",
            "-t",
            build_owner,
            "@ezm_runtime_agent_command"
        ])
        .unwrap(),
        original_launch
    );
    h.tmux_capture(&["set-option", "-t", parent, "@ezm_workspace_project", "demo"])
        .unwrap();
    let invalid = h
        .run_ezm_in_dir(
            &root,
            &[
                "__internal",
                "focus",
                "--session",
                build_owner,
                "--slot",
                "2",
            ],
            &env,
            0,
        )
        .unwrap();
    assert_ne!(
        invalid.exit_code, 0,
        "unconfigured slot must not be promoted"
    );
    let review_pane = h
        .tmux_capture(&[
            "show-options",
            "-qv",
            "-t",
            review_owner,
            "@ezm_slot_1_pane",
        ])
        .unwrap();
    let mut client = h.spawn_tmux_client(parent, 40, 140).unwrap();
    for (key, wanted) in [("S", "shell"), ("a", "agent")] {
        client.send_prefix_key(key).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let mode = h
                .tmux_capture(&["show-options", "-qv", "-t", build_owner, "@ezm_slot_1_mode"])
                .unwrap();
            if mode.trim() == wanted {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "group binding did not switch to {wanted}: {mode}; {}",
                client.terminal_output()
            );
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert_eq!(
            h.tmux_capture(&[
                "show-options",
                "-qv",
                "-t",
                review_owner,
                "@ezm_slot_1_mode"
            ])
            .unwrap()
            .trim(),
            "agent"
        );
        assert_eq!(
            h.tmux_capture(&[
                "show-options",
                "-qv",
                "-t",
                review_owner,
                "@ezm_slot_1_pane"
            ])
            .unwrap(),
            review_pane
        );
    }
    drop(client);
    assert_eq!(
        fs::read_to_string(h.work_dir().join("ready"))
            .unwrap()
            .lines()
            .count(),
        2,
        "mode switching respawned an agent"
    );
    let status = h
        .run_remote_agents_in_dir(&root, &["status", "demo"], &env, 0)
        .unwrap();
    assert_eq!(status.exit_code, 0, "{}", status.stderr);
    assert!(status.stdout.contains("build") && status.stdout.contains("review"));
    let extra = h
        .tmux_capture(&[
            "new-window",
            "-d",
            "-P",
            "-F",
            "#{pane_id}",
            "-t",
            &format!("{build_owner}:"),
            "sleep",
            "600",
        ])
        .unwrap();
    h.tmux_capture(&[
        "set-option",
        "-t",
        build_owner,
        "@ezm_slot_3_backing_agent_pane",
        extra.trim(),
    ])
    .unwrap();
    let refused = h
        .run_remote_agents_in_dir(&root, &["close", "demo", "--group", "build"], &env, 0)
        .unwrap();
    assert_ne!(refused.exit_code, 0);
    assert!(refused.stderr.contains("retains a live pane"));
    h.tmux_capture(&["kill-pane", "-t", extra.trim()]).unwrap();
    h.tmux_capture(&[
        "set-option",
        "-u",
        "-t",
        build_owner,
        "@ezm_slot_3_backing_agent_pane",
    ])
    .unwrap();
    h.tmux_capture(&[
        "set-option",
        "-t",
        build_owner,
        "@ezm_workspace_group",
        "somebody_else",
    ])
    .unwrap();
    let refused = h
        .run_remote_agents_in_dir(&root, &["close", "demo", "--group", "build"], &env, 0)
        .unwrap();
    assert_ne!(refused.exit_code, 0);
    assert!(refused.stderr.contains("different project/group"));
    assert!(h.tmux_capture(&["has-session", "-t", build_owner]).is_ok());
    h.tmux_capture(&[
        "set-option",
        "-t",
        build_owner,
        "@ezm_workspace_group",
        "build",
    ])
    .unwrap();
    let close = h
        .run_remote_agents_in_dir(&root, &["close", "demo", "--group", "build"], &env, 0)
        .unwrap();
    assert_eq!(close.exit_code, 0, "{}", close.stderr);
    let remaining = h
        .tmux_capture(&["list-windows", "-a", "-F", "#{window_name}"])
        .unwrap();
    assert!(remaining.lines().any(|l| l == "review"));
    assert!(!remaining.lines().any(|l| l == "build"));
}
