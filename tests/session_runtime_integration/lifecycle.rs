use std::cell::RefCell;
use std::collections::HashSet;

use ez_mux::session::SessionAction;
use ez_mux::session::resolve_session_identity;

use super::support::{FakeTmux, ensure_local_project_session, error_session_name};

#[test]
fn runtime_create_path_rolls_back_after_attach_failure() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let tmux = FakeTmux {
        interactive_attach: true,
        attach_error: RefCell::new(Some(String::from("attach failed"))),
        ..FakeTmux::default()
    };

    let error =
        ensure_local_project_session(project_dir, &tmux).expect_err("create path should fail");

    let rendered = error.to_string();
    assert!(rendered.contains("attach failed"));
    assert_eq!(tmux.created.borrow().len(), 1);
    assert_eq!(tmux.bootstrapped.borrow().len(), 1);
    assert_eq!(tmux.attached.borrow().len(), 1);
    assert_eq!(
        tmux.teardown_calls.borrow().as_slice(),
        &[error_session_name(project_dir)]
    );
    assert!(
        !tmux
            .sessions
            .borrow()
            .contains(&error_session_name(project_dir))
    );
}

#[test]
fn runtime_rolls_back_when_layout_bootstrap_fails() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let tmux = FakeTmux {
        bootstrap_error: RefCell::new(Some(String::from("injected layout failure"))),
        ..FakeTmux::default()
    };

    let error = ensure_local_project_session(project_dir, &tmux)
        .expect_err("injected bootstrap failure should be returned");

    assert!(error.to_string().contains("injected layout failure"));
    assert_eq!(
        tmux.teardown_calls.borrow().as_slice(),
        &[error_session_name(project_dir)]
    );
    assert!(
        !tmux
            .sessions
            .borrow()
            .contains(&error_session_name(project_dir))
    );
    assert!(tmux.attached.borrow().is_empty());
}

#[test]
fn runtime_rolls_back_when_auxiliary_bootstrap_fails() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let tmux = FakeTmux {
        auxiliary_error: RefCell::new(Some(String::from("injected auxiliary failure"))),
        ..FakeTmux::default()
    };

    let error = ensure_local_project_session(project_dir, &tmux)
        .expect_err("injected auxiliary failure should be returned");

    assert!(error.to_string().contains("injected auxiliary failure"));
    assert_eq!(
        tmux.teardown_calls.borrow().as_slice(),
        &[error_session_name(project_dir)]
    );
    assert!(
        !tmux
            .sessions
            .borrow()
            .contains(&error_session_name(project_dir))
    );
}

#[test]
fn runtime_reports_bootstrap_and_cleanup_failures_together() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let tmux = FakeTmux {
        attach_error: RefCell::new(Some(String::from("injected attach failure"))),
        teardown_error: RefCell::new(Some(String::from("injected cleanup failure"))),
        ..FakeTmux::default()
    };

    let error = ensure_local_project_session(project_dir, &tmux)
        .expect_err("injected attach failure should be returned");
    let rendered = error.to_string();

    assert!(rendered.contains("injected attach failure"));
    assert!(rendered.contains("injected cleanup failure"));
    assert_eq!(
        tmux.teardown_calls.borrow().as_slice(),
        &[error_session_name(project_dir)]
    );
    assert!(
        tmux.sessions
            .borrow()
            .contains(&error_session_name(project_dir))
    );
}

#[test]
fn runtime_never_rolls_back_a_preexisting_session_after_attach_failure() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let session_name = error_session_name(project_dir);
    let tmux = FakeTmux {
        sessions: RefCell::new(HashSet::from([session_name.clone()])),
        attach_error: RefCell::new(Some(String::from("pre-existing attach failure"))),
        ..FakeTmux::default()
    };

    let error = ensure_local_project_session(project_dir, &tmux)
        .expect_err("pre-existing attach failure should be returned");

    assert!(error.to_string().contains("pre-existing attach failure"));
    assert!(tmux.teardown_calls.borrow().is_empty());
    assert!(tmux.sessions.borrow().contains(&session_name));
    assert!(tmux.created.borrow().is_empty());
}

#[test]
fn runtime_rollback_preserves_preexisting_helpers_and_same_prefix_sessions() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let session_name = error_session_name(project_dir);
    let mode_cache = format!("{session_name}__mode_cache");
    let popup = format!("{session_name}__popup_slot_1");
    let unrelated = format!("{session_name}__user-owned");
    let tmux = FakeTmux {
        sessions: RefCell::new(HashSet::from([
            mode_cache.clone(),
            popup.clone(),
            unrelated.clone(),
        ])),
        attach_error: RefCell::new(Some(String::from("attach failed after bootstrap"))),
        interactive_attach: true,
        ..FakeTmux::default()
    };

    ensure_local_project_session(project_dir, &tmux).expect_err("attach should fail");

    let sessions = tmux.sessions.borrow();
    assert!(sessions.contains(&mode_cache));
    assert!(sessions.contains(&popup));
    assert!(sessions.contains(&unrelated));
    assert!(!sessions.contains(&session_name));
}

#[test]
fn runtime_rollback_removes_only_known_helpers_created_during_bootstrap() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let session_name = error_session_name(project_dir);
    let mode_cache = format!("{session_name}__mode_cache");
    let popup = format!("{session_name}__popup_slot_2");
    let unrelated = format!("{session_name}__user-owned");
    let tmux = FakeTmux {
        helpers_created_during_bootstrap: RefCell::new(vec![
            mode_cache.clone(),
            popup.clone(),
            unrelated.clone(),
        ]),
        attach_error: RefCell::new(Some(String::from("attach failed after bootstrap"))),
        interactive_attach: true,
        ..FakeTmux::default()
    };

    ensure_local_project_session(project_dir, &tmux).expect_err("attach should fail");

    let sessions = tmux.sessions.borrow();
    assert!(!sessions.contains(&mode_cache));
    assert!(!sessions.contains(&popup));
    assert!(sessions.contains(&unrelated));
    assert!(!sessions.contains(&session_name));
}

#[test]
fn runtime_can_relaunch_after_successful_bootstrap_rollback() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let tmux = FakeTmux {
        bootstrap_error: RefCell::new(Some(String::from("fail once"))),
        interactive_attach: true,
        ..FakeTmux::default()
    };

    ensure_local_project_session(project_dir, &tmux).expect_err("first run should fail");
    assert!(
        !tmux
            .sessions
            .borrow()
            .contains(&error_session_name(project_dir))
    );

    let outcome = ensure_local_project_session(project_dir, &tmux).expect("second run");

    assert_eq!(outcome.action, SessionAction::Create);
    assert_eq!(tmux.created.borrow().len(), 2);
    assert_eq!(tmux.bootstrapped.borrow().len(), 2);
    assert_eq!(tmux.teardown_calls.borrow().len(), 1);
    assert_eq!(tmux.attached.borrow().len(), 1);
}

#[test]
fn resolver_is_deterministic_for_same_project_path() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path().join("My Project@2026");
    std::fs::create_dir_all(&project_dir).expect("create project dir");

    let first = resolve_session_identity(&project_dir).expect("resolve first");
    let second = resolve_session_identity(&project_dir).expect("resolve second");

    assert_eq!(first.project_key, second.project_key);
    assert_eq!(first.session_name, second.session_name);
    assert!(first.session_name.starts_with("ezm-"));
    assert!(!first.session_name.contains(' '));
}

#[test]
fn resolver_distinguishes_between_different_projects() {
    let temp = tempfile::tempdir().expect("tempdir");
    let first_project = temp.path().join("first");
    let second_project = temp.path().join("second");
    std::fs::create_dir_all(&first_project).expect("create first");
    std::fs::create_dir_all(&second_project).expect("create second");

    let first = resolve_session_identity(&first_project).expect("resolve first");
    let second = resolve_session_identity(&second_project).expect("resolve second");

    assert_ne!(first.project_key, second.project_key);
    assert_ne!(first.session_name, second.session_name);
}
