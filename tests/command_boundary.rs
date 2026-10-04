use clap::Parser;

#[test]
fn one_cli_exposes_workspace_lifecycle_and_layout() {
    for verb in [
        "open", "start", "attach", "status", "doctor", "close", "stop",
    ] {
        assert!(
            ez_mux::cli::Cli::try_parse_from(["ezm", verb, "demo"]).is_ok(),
            "missing {verb}"
        );
    }
    assert!(ez_mux::cli::Cli::try_parse_from(["ezm", "repair"]).is_ok());
    assert!(ez_mux::cli::Cli::try_parse_from(["ezm", "--connect"]).is_err());
}
