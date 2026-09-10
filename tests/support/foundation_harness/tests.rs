use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::Command;

use super::{FoundationHarness, MAX_TERMINAL_OUTPUT};

#[test]
fn terminal_output_buffer_handles_multibyte_read_and_truncation_boundaries() {
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
