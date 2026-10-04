use clap::Parser;

#[test]
fn one_cli_exposes_workspace_lifecycle_and_layout() {
    for verb in ["open", "attach", "status", "doctor", "close"] {
        assert!(
            ez_mux::cli::Cli::try_parse_from(["ezm", verb, "demo"]).is_ok(),
            "missing {verb}"
        );
    }
    assert!(ez_mux::cli::Cli::try_parse_from(["ezm", "repair"]).is_ok());
    assert!(ez_mux::cli::Cli::try_parse_from(["ezm", "--connect"]).is_err());
}

#[test]
fn backend_administration_is_not_a_workspace_command() {
    for verb in ["start", "stop"] {
        assert!(ez_mux::cli::Cli::try_parse_from(["ezm", verb, "demo"]).is_err());
    }
}
