use std::cell::RefCell;

use ez_mux::session::RemoteTransportFlags;
use ez_mux::session::SessionDamageAnalysis;
use ez_mux::session::SessionRepairOutcome;
use ez_mux::session::analyze_session_damage;
use ez_mux::session::auxiliary_viewer;
use ez_mux::session::reconcile_session_damage;
use ez_mux::session::teardown_session;
use ez_mux::session::toggle_popup_shell;

use super::support::FakeTmux;

#[test]
fn popup_toggle_routes_to_tmux_client_and_toggles_open_then_close() {
    let tmux = FakeTmux {
        interactive_attach: true,
        ..FakeTmux::default()
    };

    let first = toggle_popup_shell(
        "ezm-session-88",
        2,
        None,
        None,
        None,
        RemoteTransportFlags::default(),
        &tmux,
    )
    .expect("first toggle");
    let second = toggle_popup_shell(
        "ezm-session-88",
        2,
        None,
        None,
        None,
        RemoteTransportFlags::default(),
        &tmux,
    )
    .expect("second toggle");

    assert_eq!(first.action, ez_mux::session::PopupShellAction::Opened);
    assert_eq!(second.action, ez_mux::session::PopupShellAction::Closed);
    assert_eq!(first.width_pct, 70);
    assert_eq!(first.height_pct, 70);
    assert_eq!(
        tmux.popup_toggles.borrow().as_slice(),
        &[
            (String::from("ezm-session-88"), 2),
            (String::from("ezm-session-88"), 2)
        ]
    );
}

#[test]
fn popup_toggle_surfaces_tmux_failures() {
    let tmux = FakeTmux {
        interactive_attach: true,
        popup_toggle_error: RefCell::new(Some(String::from("display-popup failed"))),
        ..FakeTmux::default()
    };

    let error = toggle_popup_shell(
        "ezm-session-88",
        2,
        None,
        None,
        None,
        RemoteTransportFlags::default(),
        &tmux,
    )
    .expect_err("popup should fail");

    assert!(error.to_string().contains("display-popup failed"));
    assert_eq!(tmux.popup_toggles.borrow().len(), 1);
}

#[test]
fn auxiliary_viewer_create_reuse_close_is_deterministic() {
    let tmux = FakeTmux {
        interactive_attach: true,
        ..FakeTmux::default()
    };

    let created = auxiliary_viewer("ezm-session-91", true, false, false, &tmux).expect("create");
    let reused = auxiliary_viewer("ezm-session-91", true, false, false, &tmux).expect("reuse");
    let closed = auxiliary_viewer("ezm-session-91", false, false, false, &tmux).expect("close");

    assert_eq!(
        created.action,
        ez_mux::session::AuxiliaryViewerAction::Created
    );
    assert_eq!(
        reused.action,
        ez_mux::session::AuxiliaryViewerAction::Reused
    );
    assert_eq!(
        closed.action,
        ez_mux::session::AuxiliaryViewerAction::Closed
    );
    assert_eq!(
        tmux.auxiliary_calls.borrow().as_slice(),
        &[
            (String::from("ezm-session-91"), true),
            (String::from("ezm-session-91"), true),
            (String::from("ezm-session-91"), false)
        ]
    );
}

#[test]
fn auxiliary_viewer_surfaces_tmux_failures() {
    let tmux = FakeTmux {
        interactive_attach: true,
        auxiliary_error: RefCell::new(Some(String::from("new-window failed"))),
        ..FakeTmux::default()
    };

    let error =
        auxiliary_viewer("ezm-session-91", true, false, false, &tmux).expect_err("aux should fail");
    assert!(error.to_string().contains("new-window failed"));
    assert_eq!(tmux.auxiliary_calls.borrow().len(), 1);
}

#[test]
fn teardown_pipeline_is_idempotent_when_helpers_are_absent() {
    let tmux = FakeTmux {
        interactive_attach: true,
        ..FakeTmux::default()
    };

    let first = teardown_session("ezm-session-91", &tmux).expect("first teardown");
    let second = teardown_session("ezm-session-91", &tmux).expect("second teardown");

    assert_eq!(first.session_name, "ezm-session-91");
    assert!(first.project_session_removed);
    assert_eq!(first.helper_sessions_removed, 2);
    assert_eq!(first.helper_processes_removed, 3);

    assert_eq!(second.session_name, "ezm-session-91");
    assert!(!second.project_session_removed);
    assert_eq!(second.helper_sessions_removed, 0);
    assert_eq!(second.helper_processes_removed, 0);

    assert_eq!(
        tmux.teardown_calls.borrow().as_slice(),
        &[
            String::from("ezm-session-91"),
            String::from("ezm-session-91")
        ]
    );
}

#[test]
fn session_damage_analysis_routes_to_tmux_client() {
    let tmux = FakeTmux {
        interactive_attach: true,
        damage_analysis: RefCell::new(SessionDamageAnalysis {
            healthy_slots: vec![1, 2, 4],
            missing_visible_slots: vec![3, 5],
            missing_backing_slots: Vec::new(),
            recreate_order: vec![3, 5],
        }),
        ..FakeTmux::default()
    };

    let analysis = analyze_session_damage("ezm-session-92", &tmux).expect("analysis");

    assert_eq!(analysis.healthy_slots, vec![1, 2, 4]);
    assert_eq!(analysis.recreate_order, vec![3, 5]);
    assert_eq!(
        tmux.damage_analysis_calls.borrow().as_slice(),
        &[String::from("ezm-session-92")]
    );
}

#[test]
fn selective_reconcile_routes_to_tmux_client() {
    let tmux = FakeTmux {
        interactive_attach: true,
        repair_outcome: RefCell::new(SessionRepairOutcome {
            session_name: String::from("ezm-session-93"),
            healthy_slots: vec![1, 2, 4],
            recreated_slots: vec![3, 5],
        }),
        ..FakeTmux::default()
    };

    let outcome = reconcile_session_damage("ezm-session-93", &tmux).expect("repair");

    assert_eq!(outcome.session_name, "ezm-session-93");
    assert_eq!(outcome.recreated_slots, vec![3, 5]);
    assert_eq!(
        tmux.repair_calls.borrow().as_slice(),
        &[String::from("ezm-session-93")]
    );
}
