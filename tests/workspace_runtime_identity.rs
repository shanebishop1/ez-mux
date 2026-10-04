#![allow(dead_code)]
mod support;
use std::fs;
use support::foundation_harness::{FoundationHarness, serial_test_guard};

#[test]
fn existing_group_identity_comes_from_its_window_without_config_adoption() {
    let _guard = serial_test_guard();
    let h = FoundationHarness::new_for_suite("workspace-runtime-identity").unwrap();
    let root = h.work_dir().join("project");
    fs::create_dir_all(&root).unwrap();
    let file = h.work_dir().join("workspace.json");
    let ready = h.work_dir().join("ready");
    fs::write(
        &file,
        serde_json::json!({"name":"demo","root":root,
        "groups":{"main":{"panes":3,"slots":[{"directory":root,
        "command":format!("touch {}; exec sleep 600",ready.display())}]}}})
        .to_string(),
    )
    .unwrap();
    let args = [
        "workspace",
        "--file",
        file.to_str().unwrap(),
        "open",
        "--no-attach",
    ];
    let result = h.run_ezm_in_dir(&root, &args, &[], 0).unwrap();
    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    let windows = h
        .tmux_capture(&[
            "list-windows",
            "-a",
            "-F",
            "#{session_name}|#{window_id}|#{@ezm_group_owner}",
        ])
        .unwrap();
    let fields: Vec<_> = windows
        .lines()
        .find(|l| l.starts_with("ezm-project-"))
        .unwrap()
        .split('|')
        .collect();
    let owner = fields[2];
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        let panes = h
            .tmux_capture(&["list-panes", "-t", owner, "-F", "#{pane_start_command}"])
            .unwrap();
        if ready.exists() && panes.lines().filter(|l| !l.is_empty()).count() == 3 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "layout did not become ready"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let renamed = "ezm-existing-runtime-owner";
    h.tmux_capture(&["rename-session", "-t", owner, renamed])
        .unwrap();
    h.tmux_capture(&[
        "set-window-option",
        "-t",
        fields[1],
        "@ezm_group_owner",
        renamed,
    ])
    .unwrap();
    let before = h
        .tmux_capture(&["list-panes", "-a", "-F", "#{pane_id}|#{pane_pid}"])
        .unwrap();
    let result = h.run_ezm_in_dir(&root, &args, &[], 0).unwrap();
    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    assert_eq!(
        before,
        h.tmux_capture(&["list-panes", "-a", "-F", "#{pane_id}|#{pane_pid}"])
            .unwrap()
    );
    let close = h
        .run_ezm_in_dir(
            &root,
            &["workspace", "--file", file.to_str().unwrap(), "close"],
            &[],
            0,
        )
        .unwrap();
    assert_eq!(close.exit_code, 0, "{}", close.stderr);
    assert!(h.tmux_capture(&["has-session", "-t", renamed]).is_err());
}

#[test]
fn four_pane_explicit_directories_keep_logical_slot_order() {
    let _guard = serial_test_guard();
    let h = FoundationHarness::new_for_suite("workspace-four-pane-order").unwrap();
    let root = h.work_dir().join("project");
    fs::create_dir_all(&root).unwrap();
    let slots: Vec<_> = (1..=4)
        .map(|n| {
            let dir = root.join(format!("worktree-{n}"));
            fs::create_dir_all(&dir).unwrap();
            serde_json::json!({"directory":dir, "command":"exec sleep 600"})
        })
        .collect();
    let file = h.work_dir().join("workspace.json");
    fs::write(
        &file,
        serde_json::json!({"name":"demo","root":root,"groups":{"main":{"panes":4,"slots":slots}}})
            .to_string(),
    )
    .unwrap();
    let args = [
        "workspace",
        "--file",
        file.to_str().unwrap(),
        "open",
        "--no-attach",
    ];
    let result = h.run_ezm_in_dir(&root, &args, &[], 0).unwrap();
    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    let windows = h
        .tmux_capture(&[
            "list-windows",
            "-a",
            "-F",
            "#{session_name}|#{@ezm_group_owner}",
        ])
        .unwrap();
    let owner = windows
        .lines()
        .find(|l| l.starts_with("ezm-project-"))
        .unwrap()
        .split('|')
        .nth(1)
        .unwrap();
    for n in 1..=4 {
        let directory = h
            .tmux_capture(&[
                "show-options",
                "-qv",
                "-t",
                owner,
                &format!("@ezm_slot_{n}_worktree"),
            ])
            .unwrap();
        assert_eq!(
            directory.trim(),
            root.join(format!("worktree-{n}")).to_str().unwrap()
        );
    }
    let result = h.run_ezm_in_dir(&root, &args, &[], 0).unwrap();
    assert_eq!(result.exit_code, 0, "{}", result.stderr);
}
