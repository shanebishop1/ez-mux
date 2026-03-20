use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use ez_mux::session::SessionAction;
use ez_mux::session::TmuxClient;
use ez_mux::session::ensure_project_session;
use ez_mux::session::resolve_session_identity;

#[derive(Default)]
struct FakeTmux {
    sessions: RefCell<HashSet<String>>,
    created: RefCell<Vec<(String, PathBuf)>>,
    attached: RefCell<Vec<String>>,
    skipped_non_interactive_attach: RefCell<u32>,
    interactive_attach: bool,
}

impl TmuxClient for FakeTmux {
    fn session_exists(&self, session_name: &str) -> Result<bool, ez_mux::session::SessionError> {
        Ok(self.sessions.borrow().contains(session_name))
    }

    fn create_detached_session(
        &self,
        session_name: &str,
        cwd: &Path,
    ) -> Result<(), ez_mux::session::SessionError> {
        self.sessions.borrow_mut().insert(session_name.to_string());
        self.created
            .borrow_mut()
            .push((session_name.to_string(), cwd.to_path_buf()));
        Ok(())
    }

    fn attach_session(&self, session_name: &str) -> Result<(), ez_mux::session::SessionError> {
        self.attached.borrow_mut().push(session_name.to_string());
        if !self.interactive_attach {
            *self.skipped_non_interactive_attach.borrow_mut() += 1;
        }

        Ok(())
    }
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

#[test]
fn runtime_creates_first_then_attaches_second_without_duplicate_create() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let tmux = FakeTmux {
        interactive_attach: true,
        ..FakeTmux::default()
    };

    let first = ensure_project_session(project_dir, &tmux).expect("first run");
    let second = ensure_project_session(project_dir, &tmux).expect("second run");

    assert_eq!(first.action, SessionAction::Create);
    assert_eq!(second.action, SessionAction::Attach);
    assert_eq!(first.identity.session_name, second.identity.session_name);
    assert_eq!(tmux.created.borrow().len(), 1);
    assert_eq!(tmux.attached.borrow().len(), 1);
    assert_eq!(tmux.attached.borrow()[0], second.identity.session_name);
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

    let first = ensure_project_session(project_dir, &tmux).expect("first run");
    let second = ensure_project_session(project_dir, &tmux).expect("second run");

    assert_eq!(first.action, SessionAction::Create);
    assert_eq!(second.action, SessionAction::Attach);
    assert_eq!(tmux.created.borrow().len(), 1);
    assert_eq!(tmux.attached.borrow().len(), 1);
    assert_eq!(*tmux.skipped_non_interactive_attach.borrow(), 1);
}
