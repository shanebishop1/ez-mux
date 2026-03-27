use super::super::context::resolve_popup_remote_context;

#[test]
fn popup_remote_context_is_none_when_remote_remap_is_inactive() {
    let context =
        resolve_popup_remote_context("/tmp/local", None, None).expect("context should resolve");
    assert!(context.is_none());
}

#[test]
fn popup_remote_context_resolves_when_remote_path_is_active() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_root = temp.path().join("alpha");
    let nested = repo_root.join("feature");
    std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");
    std::fs::create_dir_all(&nested).expect("create nested");

    let context =
        resolve_popup_remote_context(&nested.display().to_string(), Some("/srv/remotes"), None)
            .expect("context should resolve")
            .expect("context should be present");

    assert_eq!(
        context.remote_dir,
        String::from("/srv/remotes/alpha/feature")
    );
    assert_eq!(context.remote_server_url, None);
}

#[test]
fn popup_remote_context_includes_optional_server_url_when_configured() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_root = temp.path().join("alpha");
    let nested = repo_root.join("feature");
    std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");
    std::fs::create_dir_all(&nested).expect("create nested");

    let context = resolve_popup_remote_context(
        &nested.display().to_string(),
        Some("/srv/remotes"),
        Some(" https://shell.remote.example:7443 "),
    )
    .expect("context should resolve")
    .expect("context should be present");

    assert_eq!(
        context.remote_dir,
        String::from("/srv/remotes/alpha/feature")
    );
    assert_eq!(
        context.remote_server_url,
        Some(String::from("https://shell.remote.example:7443"))
    );
}
