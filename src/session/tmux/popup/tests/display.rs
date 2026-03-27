use super::super::display::{
    popup_active_probe_args, popup_attach_command, popup_close_args, popup_display_args,
};
use super::super::session::popup_persistence_args;

#[test]
fn popup_attach_command_targets_popup_helper_session() {
    let command = popup_attach_command("ezm-s100__popup_slot_2");
    assert_eq!(command, "tmux attach-session -t 'ezm-s100__popup_slot_2'");
}

#[test]
fn popup_display_args_target_origin_pane_and_client() {
    let args = popup_display_args(
        "%42",
        "/tmp/popup",
        Some("client-7"),
        "tmux attach-session -t 'ezm-s42__popup_slot_2'",
    );

    let rendered = args.join(" ");
    assert!(rendered.contains("display-popup -t %42"));
    assert!(rendered.contains("-c client-7"));
    assert!(rendered.contains("-d /tmp/popup"));
    assert!(rendered.contains("tmux attach-session -t 'ezm-s42__popup_slot_2'"));
}

#[test]
fn popup_close_args_target_client_when_present() {
    let args = popup_close_args(Some("/dev/pts/88"));
    assert_eq!(
        args,
        vec![
            String::from("display-popup"),
            String::from("-c"),
            String::from("/dev/pts/88"),
            String::from("-C"),
        ]
    );
}

#[test]
fn popup_close_args_omit_client_when_unset() {
    let args = popup_close_args(None);
    assert_eq!(
        args,
        vec![String::from("display-popup"), String::from("-C")]
    );
}

#[test]
fn popup_active_probe_args_include_popup_active_format() {
    let args = popup_active_probe_args(Some("/dev/pts/3"));
    assert_eq!(
        args,
        vec![
            String::from("display-message"),
            String::from("-p"),
            String::from("-c"),
            String::from("/dev/pts/3"),
            String::from("#{popup_active}"),
        ]
    );
}

#[test]
fn popup_helper_sessions_disable_destroy_unattached_for_reopen_toggle() {
    let args = popup_persistence_args("ezm-s100__popup_slot_4");
    assert_eq!(
        args,
        vec![
            String::from("set-option"),
            String::from("-t"),
            String::from("ezm-s100__popup_slot_4"),
            String::from("destroy-unattached"),
            String::from("off"),
        ]
    );
}
