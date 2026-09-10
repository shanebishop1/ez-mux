use std::cell::RefCell;
use std::path::PathBuf;

use ez_mux::session::RemoteTransportFlags;
use ez_mux::session::SessionAction;
use ez_mux::session::ensure_project_session_with_remote_path;
use ez_mux::session::ensure_project_session_with_remote_path_and_options;

use super::support::{FakeTmux, ensure_local_project_session};

#[test]
fn runtime_creates_first_then_attaches_second_without_duplicate_create() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let tmux = FakeTmux {
        interactive_attach: true,
        ..FakeTmux::default()
    };

    let first = ensure_local_project_session(project_dir, &tmux).expect("first run");
    let second = ensure_local_project_session(project_dir, &tmux).expect("second run");

    assert_eq!(first.action, SessionAction::Create);
    assert_eq!(second.action, SessionAction::Attach);
    assert_eq!(first.identity.session_name, second.identity.session_name);
    assert_eq!(first.remote_project_dir, first.identity.project_dir);
    assert_eq!(second.remote_project_dir, second.identity.project_dir);
    assert_eq!(tmux.created.borrow().len(), 1);
    assert_eq!(tmux.bootstrapped.borrow().len(), 1);
    assert_eq!(tmux.bootstrapped.borrow()[0].1, first.identity.project_dir);
    assert_eq!(tmux.attached.borrow().len(), 2);
    assert_eq!(tmux.attached.borrow()[0], first.identity.session_name);
    assert_eq!(tmux.attached.borrow()[1], second.identity.session_name);
    assert_eq!(tmux.auxiliary_calls.borrow().len(), 2);
    assert_eq!(*tmux.skipped_non_interactive_attach.borrow(), 0);
}

#[test]
fn runtime_attach_path_is_non_interactive_safe() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let tmux = FakeTmux {
        interactive_attach: false,
        ..FakeTmux::default()
    };

    let first = ensure_local_project_session(project_dir, &tmux).expect("first run");
    let second = ensure_local_project_session(project_dir, &tmux).expect("second run");

    assert_eq!(first.action, SessionAction::Create);
    assert_eq!(second.action, SessionAction::Attach);
    assert_eq!(first.remote_project_dir, first.identity.project_dir);
    assert_eq!(second.remote_project_dir, second.identity.project_dir);
    assert_eq!(tmux.created.borrow().len(), 1);
    assert_eq!(tmux.bootstrapped.borrow().len(), 1);
    assert_eq!(tmux.bootstrapped.borrow()[0].1, first.identity.project_dir);
    assert_eq!(tmux.attached.borrow().len(), 2);
    assert_eq!(tmux.auxiliary_calls.borrow().len(), 2);
    assert_eq!(*tmux.skipped_non_interactive_attach.borrow(), 2);
}

#[test]
fn runtime_perles_missing_skips_auxiliary_window_without_failing_startup() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let tmux = FakeTmux {
        interactive_attach: true,
        auxiliary_available: RefCell::new(false),
        ..FakeTmux::default()
    };

    let first = ensure_local_project_session(project_dir, &tmux).expect("first run");
    let second = ensure_local_project_session(project_dir, &tmux).expect("second run");

    assert_eq!(first.action, SessionAction::Create);
    assert_eq!(second.action, SessionAction::Attach);
    assert_eq!(tmux.created.borrow().len(), 1);
    assert_eq!(tmux.attached.borrow().len(), 2);
    assert_eq!(tmux.auxiliary_calls.borrow().len(), 2);

    let skipped =
        ez_mux::session::auxiliary_viewer("ezm-session-perles-missing", true, false, false, &tmux)
            .expect("skip");
    assert_eq!(
        skipped.action,
        ez_mux::session::AuxiliaryViewerAction::SkippedUnavailable
    );
    assert!(skipped.window_id.is_none());
}

#[test]
fn runtime_create_and_bootstrap_use_local_project_dir_when_remote_path_is_active() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_root = temp.path().join("alpha");
    let project_dir = repo_root.join("worktrees").join("feature-x");
    std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");
    std::fs::create_dir_all(&project_dir).expect("create project dir");
    let tmux = FakeTmux {
        interactive_attach: true,
        ..FakeTmux::default()
    };

    let first = ensure_project_session_with_remote_path(
        project_dir.as_path(),
        Some("/srv/remotes"),
        Some("https://shell.remote.example:7443"),
        RemoteTransportFlags::default(),
        5,
        &tmux,
    )
    .expect("first run");
    let second = ensure_project_session_with_remote_path(
        project_dir.as_path(),
        Some("/srv/remotes"),
        Some("https://shell.remote.example:7443"),
        RemoteTransportFlags::default(),
        5,
        &tmux,
    )
    .expect("second run");

    assert_eq!(first.action, SessionAction::Create);
    assert_eq!(second.action, SessionAction::Attach);
    assert_eq!(
        first.remote_project_dir,
        PathBuf::from("/srv/remotes/alpha/worktrees/feature-x")
    );
    assert_eq!(
        second.remote_project_dir,
        PathBuf::from("/srv/remotes/alpha/worktrees/feature-x")
    );
    assert_eq!(tmux.created.borrow().len(), 1);
    assert_eq!(tmux.created.borrow()[0].1, first.identity.project_dir);
    assert_eq!(tmux.bootstrapped.borrow().len(), 1);
    assert_eq!(tmux.bootstrapped.borrow()[0].1, first.identity.project_dir);
    assert!(!tmux.bootstrapped.borrow()[0].3);
    assert_eq!(tmux.attached.borrow().len(), 2);
}

#[test]
fn runtime_can_disable_worktree_bootstrap_assignment() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let tmux = FakeTmux {
        interactive_attach: true,
        ..FakeTmux::default()
    };

    let outcome = ensure_project_session_with_remote_path_and_options(
        project_dir,
        None,
        None,
        RemoteTransportFlags::default(),
        5,
        true,
        &tmux,
    )
    .expect("run should succeed");

    assert_eq!(outcome.action, SessionAction::Create);
    assert_eq!(tmux.bootstrapped.borrow().len(), 1);
    assert_eq!(
        tmux.bootstrapped.borrow()[0].1,
        outcome.identity.project_dir
    );
    assert!(tmux.bootstrapped.borrow()[0].3);
}
