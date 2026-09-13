use super::{
    REDACTED_SECRET_VALUE, format_output_diagnostics_with_args, legacy_window_zero_session_target,
    parse_primary_window_target, render_startup_trace, tmux_batch_command_for_diagnostics,
    tmux_command_for_diagnostics,
};

#[test]
fn parse_primary_window_target_prefers_active_window_id() {
    let output = "0|@77\n1|@92\n";
    assert_eq!(
        parse_primary_window_target(output),
        Some(String::from("@92"))
    );
}

#[test]
fn parse_primary_window_target_falls_back_to_first_window_id() {
    let output = "0|@77\n0|@92\n";
    assert_eq!(
        parse_primary_window_target(output),
        Some(String::from("@77"))
    );
}

#[test]
fn legacy_window_zero_session_target_detects_list_panes_zero_window_failure() {
    let args = ["list-panes", "-t", "ezm-demo:0", "-F", "#{pane_id}"];
    let stderr = "can't find window: 0";
    assert_eq!(
        legacy_window_zero_session_target(&args, stderr),
        Some((2, "ezm-demo"))
    );
}

#[test]
fn legacy_window_zero_session_target_ignores_non_matching_failures() {
    let args = ["list-panes", "-t", "ezm-demo:2", "-F", "#{pane_id}"];
    let stderr = "can't find window: 2";
    assert_eq!(legacy_window_zero_session_target(&args, stderr), None);
}

#[test]
fn tmux_command_for_diagnostics_redacts_password_set_environment_value_for_global_sync() {
    let rendered = tmux_command_for_diagnostics(&[
        "set-environment",
        "-g",
        "OPENCODE_SERVER_PASSWORD",
        "super-secret",
    ]);

    assert!(!rendered.contains("super-secret"));
    assert!(rendered.contains(REDACTED_SECRET_VALUE));
}

#[test]
fn tmux_command_for_diagnostics_redacts_password_set_environment_value_for_targeted_sync() {
    let rendered = tmux_command_for_diagnostics(&[
        "set-environment",
        "-t",
        "ezm-s42",
        "OPENCODE_SERVER_PASSWORD",
        "another-secret",
    ]);

    assert!(!rendered.contains("another-secret"));
    assert!(rendered.contains(REDACTED_SECRET_VALUE));
}

#[test]
fn new_session_password_diagnostics_redact_special_values_and_output_streams() {
    let secret = "credential with spaces;$(special)'\"";
    let environment = format!("OPENCODE_SERVER_PASSWORD={secret}");
    let command =
        tmux_command_for_diagnostics(&["new-session", "-d", "-s", "cache", "-e", &environment]);
    assert!(!command.contains(secret));
    assert!(command.contains("OPENCODE_SERVER_PASSWORD=<redacted>"));

    let output = std::process::Command::new("sh")
        .args([
            "-c",
            "printf 'stdout=%s\\n' \"$1\"; printf 'stderr=%s\\n' \"$1\" >&2",
            "redaction-fixture",
            secret,
        ])
        .output()
        .expect("shell should echo the redaction fixture");
    let rendered =
        format_output_diagnostics_with_args(&output, &["new-session", "-e", &environment]);
    assert!(!rendered.contains(secret));
    assert!(rendered.contains("stdout=<redacted>"));
    assert!(rendered.contains("stderr=<redacted>"));
}

#[test]
fn tmux_command_for_diagnostics_does_not_inject_redaction_for_unset_without_value() {
    let rendered =
        tmux_command_for_diagnostics(&["set-environment", "-gu", "OPENCODE_SERVER_PASSWORD"]);

    assert_eq!(rendered, "set-environment -gu OPENCODE_SERVER_PASSWORD");
}

#[test]
fn tmux_command_for_diagnostics_keeps_non_secret_environment_values_visible() {
    let rendered =
        tmux_command_for_diagnostics(&["set-environment", "-g", "EZM_REMOTE_PATH", "/srv/remotes"]);

    assert_eq!(rendered, "set-environment -g EZM_REMOTE_PATH /srv/remotes");
}

#[test]
fn tmux_batch_command_for_diagnostics_redacts_password_values() {
    let commands = vec![
        vec![
            String::from("set-environment"),
            String::from("-g"),
            String::from("OPENCODE_SERVER_PASSWORD"),
            String::from("super-secret"),
        ],
        vec![
            String::from("set-option"),
            String::from("-t"),
            String::from("demo"),
            String::from("@foo"),
            String::from("bar"),
        ],
    ];

    let rendered = tmux_batch_command_for_diagnostics(&commands);
    assert!(!rendered.contains("super-secret"));
    assert!(rendered.contains(REDACTED_SECRET_VALUE));
    assert!(rendered.contains("set-option -t demo @foo bar"));
}

#[test]
fn command_diagnostics_redact_url_userinfo_and_password_streams() {
    let secret = "unique-b2-command-secret";
    let output = std::process::Command::new("sh")
        .args([
            "-c",
            &format!(
                "printf 'url=https://operator:{secret}@remote.example:7443/path\\n'; printf 'OPENCODE_SERVER_PASSWORD={secret}\\n' >&2"
            ),
        ])
        .output()
        .expect("shell should emit fixture diagnostics");
    let args = ["set-environment", "-g", "OPENCODE_SERVER_PASSWORD", secret];

    let rendered = format_output_diagnostics_with_args(&output, &args);

    assert!(!rendered.contains(secret));
    assert!(rendered.contains("operator:<redacted>@remote.example:7443/path"));
    assert!(rendered.contains("OPENCODE_SERVER_PASSWORD=<redacted>"));
}

#[test]
fn command_diagnostics_keep_executable_data_separate_from_redacted_rendering() {
    let secret = "unique-b2-executable-secret";
    let executable = vec![
        String::from("set-environment"),
        String::from("-g"),
        String::from("OPENCODE_SERVER_PASSWORD"),
        secret.to_owned(),
    ];
    let rendered = super::tmux_batch_command_for_diagnostics(std::slice::from_ref(&executable));

    assert_eq!(executable[3], secret);
    assert!(!rendered.contains(secret));
    assert!(rendered.contains(REDACTED_SECRET_VALUE));
}

#[test]
fn startup_trace_renders_only_the_redacted_command_boundary() {
    let secret = "unique-b2-startup-trace-secret";
    let command = super::tmux_command_for_diagnostics(&[
        "display-popup",
        &format!("https://operator:{secret}@remote.example:7443/path"),
    ]);
    let trace = render_startup_trace(&command, "1", std::time::Duration::from_millis(4));

    assert!(!trace.contains(secret));
    assert!(trace.contains("operator:<redacted>@remote.example:7443/path"));
}

#[test]
fn startup_trace_redacts_url_userinfo_containing_sub_delimiters() {
    for punctuation in [';', ',', '(', ')'] {
        let sentinel = format!("trace{punctuation}credential");
        let command = super::tmux_command_for_diagnostics(&[
            "display-popup",
            &format!("https://operator:{sentinel}@remote.example:7443/path"),
        ]);
        let trace = render_startup_trace(&command, "1", std::time::Duration::from_millis(4));

        assert!(!trace.contains(&sentinel));
        assert!(trace.contains("operator:<redacted>@remote.example:7443/path"));
    }
}

#[test]
fn output_diagnostics_redact_named_passwords_with_sub_delimiters() {
    for punctuation in [';', ',', '(', ')'] {
        let sentinel = format!("named{punctuation}credential");
        let output = std::process::Command::new("sh")
            .args([
                "-c",
                &format!("printf '%s\\n' 'OPENCODE_SERVER_PASSWORD={sentinel}' >&2"),
            ])
            .output()
            .expect("shell should emit fixture diagnostics");

        let rendered = super::format_output_diagnostics(&output);

        assert!(!rendered.contains(&sentinel));
        assert!(rendered.contains("OPENCODE_SERVER_PASSWORD=<redacted>"));
    }
}
