use clap::Parser;
#[test]
fn generic_workspace_commands_require_explicit_manifest() {
    for verb in ["open", "status", "close"] {
        assert!(
            ez_mux::cli::Cli::try_parse_from(["ezm", "workspace", "--file", "layout.json", verb])
                .is_ok()
        );
    }
    for verb in ["start", "stop", "adopt", "new", "attach", "doctor"] {
        assert!(
            ez_mux::cli::Cli::try_parse_from(["ezm", "workspace", "--file", "layout.json", verb])
                .is_err()
        );
    }
    assert!(ez_mux::cli::Cli::try_parse_from(["ezm", "repair"]).is_ok());
}
