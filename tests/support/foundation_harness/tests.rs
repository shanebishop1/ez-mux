use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use super::{FoundationHarness, MAX_TERMINAL_OUTPUT};

#[test]
fn terminal_output_buffer_handles_multibyte_read_and_truncation_boundaries() {
    let _test_guard = super::serial_test_guard();
    let mut output = Vec::new();
    let euro_sign = [0xf0, 0x9f, 0x92, 0xa9];

    super::pty::append_terminal_output(&mut output, &euro_sign[..1]);
    super::pty::append_terminal_output(&mut output, &euro_sign[1..3]);
    super::pty::append_terminal_output(&mut output, &euro_sign[3..]);
    super::pty::append_terminal_output(&mut output, &vec![b'x'; MAX_TERMINAL_OUTPUT - 3]);

    assert_eq!(output.len(), MAX_TERMINAL_OUTPUT);
    assert_eq!(&output[..3], &euro_sign[1..]);

    let rendered = String::from_utf8_lossy(&output);
    assert!(rendered.contains('\u{fffd}'));
    assert!(rendered.ends_with(&"x".repeat(MAX_TERMINAL_OUTPUT - 3)));
}

#[test]
fn concurrent_harnesses_use_distinct_sockets_and_cleanup_only_owned_state() {
    let _test_guard = super::serial_test_guard();
    let first =
        FoundationHarness::new_for_suite("harness-isolation").expect("first isolated harness");
    let second =
        FoundationHarness::new_for_suite("harness-isolation").expect("second isolated harness");
    let first_socket = first.tmux_socket_path().to_owned();
    let second_socket = second.tmux_socket_path().to_owned();

    assert_ne!(first_socket, second_socket);
    first
        .tmux_capture(&["new-session", "-d", "-s", "first-owned", "sleep", "60"])
        .expect("first harness session");
    second
        .tmux_capture(&["new-session", "-d", "-s", "second-owned", "sleep", "60"])
        .expect("second harness session");

    first
        .reset_scenario_state()
        .expect("first harness scenario cleanup");
    assert!(
        first
            .tmux_capture(&["has-session", "-t", "first-owned"])
            .is_err()
    );
    assert!(
        second
            .tmux_capture(&["has-session", "-t", "second-owned"])
            .is_ok()
    );

    drop(first);
    assert!(!first_socket.exists());
    assert!(second.tmux_capture(&["list-sessions"]).is_ok());
    assert!(second_socket.exists());
}

#[test]
fn panic_cleanup_stops_only_the_panicking_harness_server() {
    let _test_guard = super::serial_test_guard();
    let survivor =
        FoundationHarness::new_for_suite("harness-isolation").expect("surviving isolated harness");
    let survivor_socket = survivor.tmux_socket_path().to_owned();

    let mut panicking_socket = None;
    let result = catch_unwind(AssertUnwindSafe(|| {
        let panicking = FoundationHarness::new_for_suite("harness-isolation")
            .expect("panicking isolated harness");
        panicking_socket = Some(panicking.tmux_socket_path().to_owned());
        assert!(
            panicking_socket
                .as_ref()
                .is_some_and(|socket| socket.exists())
        );
        panic!("exercise harness cleanup during assertion unwinding");
    }));
    assert!(result.is_err());

    let panicking_socket = panicking_socket.expect("panicking harness socket path");
    assert!(!panicking_socket.exists());
    assert!(survivor_socket.exists());
    assert!(survivor.tmux_capture(&["list-sessions"]).is_ok());
}

#[test]
fn completed_harness_removes_its_exact_server_and_socket() {
    let _test_guard = super::serial_test_guard();
    let (tmux_bin, socket) = {
        let harness =
            FoundationHarness::new_for_suite("harness-lifecycle").expect("isolated harness");
        harness
            .tmux_capture(&["new-session", "-d", "-s", "owned-project", "sleep", "60"])
            .expect("owned project session");
        harness
            .tmux_capture(&["new-window", "-t", "owned-project", "sleep", "60"])
            .expect("second owned project window");
        assert_eq!(
            harness
                .tmux_capture(&["list-windows", "-t", "owned-project"])
                .expect("owned project windows")
                .lines()
                .count(),
            2
        );
        (
            harness.tmux_bin.clone(),
            harness.tmux_socket_path().to_owned(),
        )
    };

    assert!(!socket.exists(), "owned socket survived harness completion");
    assert!(
        !Command::new(tmux_bin)
            .arg("-S")
            .arg(&socket)
            .arg("-f")
            .arg("/dev/null")
            .arg("list-sessions")
            .status()
            .expect("probe exact owned server")
            .success()
    );
}

#[test]
fn reset_reaps_owned_hup_ignoring_pane_process_and_preserves_other_harness() {
    let _test_guard = super::serial_test_guard();
    let unrelated =
        FoundationHarness::new_for_suite("harness-isolation").expect("unrelated harness");
    unrelated
        .tmux_capture(&["new-session", "-d", "-s", "unrelated-owned", "sleep", "60"])
        .expect("unrelated harness session");

    let harness = FoundationHarness::new_for_suite("harness-lifecycle").expect("isolated harness");
    let leaked_pid = spawn_hup_ignoring_pane(&harness, "owned-leak-reset");
    assert!(
        process_exists(leaked_pid),
        "fixture process was not alive before scenario reset (pid={leaked_pid})"
    );
    send_hup(leaked_pid);
    thread::sleep(Duration::from_millis(50));
    assert!(
        process_exists(leaked_pid),
        "HUP-ignoring fixture did not demonstrate the pre-cleanup leak (pid={leaked_pid})"
    );

    harness
        .reset_scenario_state()
        .expect("reset owned scenario state");
    let terminated = wait_for_process_exit(leaked_pid);
    if !terminated {
        kill_owned_fixture(leaked_pid);
    }

    assert!(
        terminated,
        "reset left the HUP-ignoring tmux descendant alive (pid={leaked_pid})"
    );
    assert!(
        harness
            .tmux_capture(&["has-session", "-t", super::E2E_ANCHOR_SESSION])
            .is_ok(),
        "reset removed the required anchor session"
    );
    assert!(
        unrelated
            .tmux_capture(&["has-session", "-t", "unrelated-owned"])
            .is_ok(),
        "reset affected an unrelated harness"
    );
}

#[test]
fn drop_reaps_owned_hup_ignoring_pane_process_and_preserves_other_harness() {
    let _test_guard = super::serial_test_guard();
    let unrelated =
        FoundationHarness::new_for_suite("harness-isolation").expect("unrelated harness");
    unrelated
        .tmux_capture(&["new-session", "-d", "-s", "unrelated-owned", "sleep", "60"])
        .expect("unrelated harness session");

    let (leaked_pid, socket) = {
        let harness =
            FoundationHarness::new_for_suite("harness-lifecycle").expect("isolated harness");
        let leaked_pid = spawn_hup_ignoring_pane(&harness, "owned-leak-drop");
        assert!(
            process_exists(leaked_pid),
            "fixture process was not alive before harness drop (pid={leaked_pid})"
        );
        (leaked_pid, harness.tmux_socket_path().to_owned())
    };

    let terminated = wait_for_process_exit(leaked_pid);
    if !terminated {
        kill_owned_fixture(leaked_pid);
    }

    assert!(
        terminated,
        "harness drop left the HUP-ignoring tmux descendant alive (pid={leaked_pid})"
    );
    assert!(!socket.exists(), "owned tmux socket survived harness drop");
    assert!(
        unrelated
            .tmux_capture(&["has-session", "-t", "unrelated-owned"])
            .is_ok(),
        "harness drop affected an unrelated harness"
    );
}

fn spawn_hup_ignoring_pane(harness: &FoundationHarness, session_name: &str) -> u32 {
    harness
        .tmux_capture(&[
            "new-session",
            "-d",
            "-s",
            session_name,
            "sh",
            "-c",
            "trap '' HUP; exec sleep 600",
        ])
        .expect("HUP-ignoring fixture session");

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let panes = harness
            .tmux_capture(&[
                "list-panes",
                "-t",
                session_name,
                "-F",
                "#{pane_pid}|#{pane_current_command}",
            ])
            .expect("HUP-ignoring fixture pane");
        if let Some(pid) = panes.lines().find_map(|line| {
            let (pid, command) = line.split_once('|')?;
            (command.trim() == "sleep")
                .then(|| pid.trim().parse::<u32>().expect("fixture pane pid"))
        }) {
            return pid;
        }
        assert!(
            Instant::now() < deadline,
            "HUP-ignoring fixture did not reach sleep"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn process_exists(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .is_ok_and(|status| status.success())
}

fn send_hup(pid: u32) {
    assert!(
        Command::new("kill")
            .args(["-HUP", &pid.to_string()])
            .status()
            .is_ok_and(|status| status.success()),
        "failed sending HUP to owned fixture (pid={pid})"
    );
}

fn wait_for_process_exit(pid: u32) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    while process_exists(pid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    !process_exists(pid)
}

fn kill_owned_fixture(pid: u32) {
    let _ = Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .status();
}
