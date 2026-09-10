use std::path::PathBuf;

use super::support::{FakeTmux, test_runtime_context};

#[test]
fn runtime_context_isolated_between_projects_and_preserved_on_reopen() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_a = temp.path().join("project-a");
    let project_b = temp.path().join("project-b");
    let project_empty = temp.path().join("project-empty");
    std::fs::create_dir_all(&project_a).expect("create project A");
    std::fs::create_dir_all(&project_b).expect("create project B");
    std::fs::create_dir_all(&project_empty).expect("create empty project");
    let tmux = FakeTmux {
        interactive_attach: true,
        ..FakeTmux::default()
    };
    let context_a = test_runtime_context("/srv/a", "a.example");
    let context_b = test_runtime_context("/srv/b", "b.example");
    let context_empty = ez_mux::config::RuntimeContext::default();

    let first_a = ez_mux::session::ensure_project_session_with_runtime_context(
        &project_a, &context_a, 1, true, &tmux,
    )
    .expect("project A should start");
    let first_b = ez_mux::session::ensure_project_session_with_runtime_context(
        &project_b, &context_b, 1, true, &tmux,
    )
    .expect("project B should start");
    let first_empty = ez_mux::session::ensure_project_session_with_runtime_context(
        &project_empty,
        &context_empty,
        1,
        true,
        &tmux,
    )
    .expect("project without a password should start");
    let reopened_a = ez_mux::session::ensure_project_session_with_runtime_context(
        &project_a, &context_b, 1, true, &tmux,
    )
    .expect("project A should reopen");

    assert_eq!(
        first_a.remote_project_dir,
        PathBuf::from("/srv/a/project-a")
    );
    assert_eq!(
        first_b.remote_project_dir,
        PathBuf::from("/srv/b/project-b")
    );
    assert_eq!(reopened_a.remote_project_dir, first_a.remote_project_dir);
    assert_ne!(first_a.remote_project_dir, first_b.remote_project_dir);
    assert_eq!(
        tmux.runtime_passwords
            .borrow()
            .get(&first_a.identity.session_name),
        Some(&Some(String::from("password-a")))
    );
    assert_eq!(
        tmux.runtime_passwords
            .borrow()
            .get(&first_b.identity.session_name),
        Some(&Some(String::from("password-b")))
    );
    assert_eq!(
        tmux.runtime_passwords
            .borrow()
            .get(&first_empty.identity.session_name),
        Some(&None)
    );
}

#[test]
fn same_url_projects_keep_distinct_session_credentials() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_a = temp.path().join("project-a");
    let project_b = temp.path().join("project-b");
    std::fs::create_dir_all(&project_a).expect("create project A");
    std::fs::create_dir_all(&project_b).expect("create project B");
    let tmux = FakeTmux::default();
    let context_a = test_runtime_context("/srv/a", "shared.example");
    let mut context_b = test_runtime_context("/srv/b", "shared.example");
    context_b.remote.shared_server.password.value = Some(String::from("password-b"));

    let first_a = ez_mux::session::ensure_project_session_with_runtime_context(
        &project_a, &context_a, 1, true, &tmux,
    )
    .expect("project A should start");
    let first_b = ez_mux::session::ensure_project_session_with_runtime_context(
        &project_b, &context_b, 1, true, &tmux,
    )
    .expect("project B should start");
    let passwords = tmux.runtime_passwords.borrow();

    assert_eq!(
        passwords.get(&first_a.identity.session_name),
        Some(&Some(String::from("password-s")))
    );
    assert_eq!(
        passwords.get(&first_b.identity.session_name),
        Some(&Some(String::from("password-b")))
    );
}

#[test]
fn absent_password_on_reopen_preserves_existing_session_credential() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("project-a");
    std::fs::create_dir_all(&project).expect("create project");
    let tmux = FakeTmux::default();
    let context = test_runtime_context("/srv/a", "a.example");
    let first = ez_mux::session::ensure_project_session_with_runtime_context(
        &project, &context, 1, true, &tmux,
    )
    .expect("project should start");
    let mut context_without_password = context;
    context_without_password.remote.shared_server.password.value = None;

    ez_mux::session::ensure_project_session_with_runtime_context(
        &project,
        &context_without_password,
        1,
        true,
        &tmux,
    )
    .expect("project should reopen without replacing its credential");

    assert_eq!(
        tmux.runtime_passwords
            .borrow()
            .get(&first.identity.session_name),
        Some(&Some(String::from("password-a")))
    );
}
