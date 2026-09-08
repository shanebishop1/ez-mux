use super::remote_authority::ParsedSshAuthority;

pub(super) fn remote_transport_label(use_tssh: bool, use_mosh: bool) -> &'static str {
    if use_tssh {
        "tssh"
    } else if use_mosh {
        "mosh"
    } else {
        "ssh"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LocalShellStartup {
    Default,
    DisableGitstatus,
}

#[derive(Debug)]
struct StructuredInvocation {
    program: &'static str,
    arguments: Vec<InvocationArgument>,
}

#[derive(Debug)]
enum InvocationArgument {
    Literal(String),
    ShellValue(String),
}

impl StructuredInvocation {
    fn render(self) -> String {
        let mut rendered = vec![self.program.to_owned()];
        rendered.extend(self.arguments.into_iter().map(|argument| match argument {
            InvocationArgument::Literal(value) => value,
            InvocationArgument::ShellValue(value) => format!("'{}'", escape_single_quotes(&value)),
        }));
        rendered.join(" ")
    }
}

pub(super) fn build_remote_invocation(
    authority: &ParsedSshAuthority,
    remote_script: &str,
    use_tssh: bool,
    use_mosh: bool,
) -> String {
    build_remote_invocation_with_local_shell_startup(
        authority,
        remote_script,
        use_tssh,
        use_mosh,
        LocalShellStartup::Default,
    )
}

pub(super) fn build_remote_invocation_with_local_shell_startup(
    authority: &ParsedSshAuthority,
    remote_script: &str,
    use_tssh: bool,
    use_mosh: bool,
    local_shell_startup: LocalShellStartup,
) -> String {
    let invocation = if use_tssh {
        build_tssh_invocation(authority, remote_script)
    } else if use_mosh {
        build_mosh_invocation(authority, remote_script)
    } else {
        build_ssh_invocation(authority, remote_script)
    };

    wrap_local_login_shell(&invocation, local_shell_startup)
}

fn wrap_local_login_shell(invocation: &str, startup: LocalShellStartup) -> String {
    let shell = format!(
        "\"${{SHELL:-/bin/sh}}\" -lic '{}'",
        escape_single_quotes(invocation)
    );
    match startup {
        LocalShellStartup::Default => shell,
        LocalShellStartup::DisableGitstatus => {
            format!("POWERLEVEL9K_DISABLE_GITSTATUS=true {shell}")
        }
    }
}

fn build_ssh_invocation(authority: &ParsedSshAuthority, remote_script: &str) -> String {
    let mut arguments = vec![InvocationArgument::Literal(String::from("-tt"))];
    if let Some(port) = authority.port {
        arguments.push(InvocationArgument::Literal(String::from("-p")));
        arguments.push(InvocationArgument::Literal(port.to_string()));
    }
    arguments.push(InvocationArgument::Literal(String::from("--")));
    arguments.push(InvocationArgument::ShellValue(authority.target.clone()));
    arguments.push(InvocationArgument::ShellValue(remote_script.to_owned()));
    StructuredInvocation {
        program: "ssh",
        arguments,
    }
    .render()
}

fn build_tssh_invocation(authority: &ParsedSshAuthority, remote_script: &str) -> String {
    let mut arguments = vec![InvocationArgument::Literal(String::from("-tt"))];
    if let Some(port) = authority.port {
        arguments.push(InvocationArgument::Literal(String::from("-p")));
        arguments.push(InvocationArgument::Literal(port.to_string()));
    }
    arguments.push(InvocationArgument::Literal(String::from("--")));
    arguments.push(InvocationArgument::ShellValue(authority.target.clone()));
    arguments.push(InvocationArgument::ShellValue(remote_script.to_owned()));
    StructuredInvocation {
        program: "tssh",
        arguments,
    }
    .render()
}

fn build_mosh_invocation(authority: &ParsedSshAuthority, remote_script: &str) -> String {
    let mut arguments = vec![InvocationArgument::Literal(String::from("--no-init"))];
    if let Some(port) = authority.port {
        arguments.push(InvocationArgument::Literal(format!(
            "--ssh='ssh -p {port} --'"
        )));
    }
    arguments.push(InvocationArgument::Literal(String::from("--")));
    arguments.push(InvocationArgument::ShellValue(authority.target.clone()));
    arguments.push(InvocationArgument::Literal(String::from("--")));
    arguments.push(InvocationArgument::Literal(String::from("'sh'")));
    arguments.push(InvocationArgument::Literal(String::from("'-lc'")));
    arguments.push(InvocationArgument::ShellValue(remote_script.to_owned()));
    StructuredInvocation {
        program: "mosh",
        arguments,
    }
    .render()
}

fn escape_single_quotes(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}

#[cfg(test)]
mod tests {
    use super::super::remote_authority::parse_remote_ssh_authority;
    use super::{
        LocalShellStartup, build_remote_invocation,
        build_remote_invocation_with_local_shell_startup, remote_transport_label,
    };

    #[test]
    fn ssh_transport_label_is_stable() {
        assert_eq!(remote_transport_label(false, false), "ssh");
    }

    #[test]
    fn mosh_transport_label_is_stable() {
        assert_eq!(remote_transport_label(false, true), "mosh");
    }

    #[test]
    fn tssh_transport_label_is_stable() {
        assert_eq!(remote_transport_label(true, false), "tssh");
    }

    #[test]
    fn tssh_transport_takes_precedence_over_mosh() {
        assert_eq!(remote_transport_label(true, true), "tssh");
    }

    #[test]
    fn ssh_invocation_uses_target_and_remote_script() {
        let authority =
            parse_remote_ssh_authority("https://shell.remote.example:7443").expect("authority");
        let invocation =
            build_remote_invocation(&authority, "cd '/srv/remotes' && nvim", false, false);

        assert!(invocation.starts_with("\"${SHELL:-/bin/sh}\" -lic '"));
        assert!(invocation.contains("ssh -tt -p 7443"));
        assert!(invocation.contains("'shell.remote.example'"));
        assert!(invocation.contains("cd '"));
    }

    #[test]
    fn ssh_invocation_escapes_nested_login_shell_transport() {
        let authority =
            parse_remote_ssh_authority("https://shell.remote.example:7443").expect("authority");
        let remote_script = "cd '/srv/remote work' && printf '%s\\n' '$HOME'";
        let invocation = build_remote_invocation(&authority, remote_script, false, false);
        let raw_transport = format!(
            "ssh -tt -p 7443 -- 'shell.remote.example' '{}'",
            remote_script.replace('\'', "'\"'\"'")
        );

        assert_eq!(
            invocation,
            format!(
                "\"${{SHELL:-/bin/sh}}\" -lic '{}'",
                raw_transport.replace('\'', "'\"'\"'")
            )
        );
    }

    #[test]
    fn ssh_invocation_can_disable_local_gitstatus_startup() {
        let authority =
            parse_remote_ssh_authority("https://shell.remote.example:7443").expect("authority");
        let invocation = build_remote_invocation_with_local_shell_startup(
            &authority,
            "cd '/srv/remotes' && exec \"${SHELL:-/bin/sh}\" -l",
            false,
            false,
            LocalShellStartup::DisableGitstatus,
        );

        assert!(
            invocation
                .starts_with("POWERLEVEL9K_DISABLE_GITSTATUS=true \"${SHELL:-/bin/sh}\" -lic '")
        );
        assert!(invocation.contains("ssh -tt -p 7443"));
        assert!(invocation.contains("'shell.remote.example'"));
        assert!(invocation.contains("exec \"${SHELL:-/bin/sh}\" -l"));
    }

    #[test]
    fn mosh_invocation_uses_custom_ssh_port_and_remote_script() {
        let authority =
            parse_remote_ssh_authority("https://shell.remote.example:7443").expect("authority");
        let invocation =
            build_remote_invocation(&authority, "cd '/srv/remotes' && nvim", false, true);

        assert!(invocation.starts_with("\"${SHELL:-/bin/sh}\" -lic '"));
        assert!(invocation.contains("mosh --no-init"));
        assert!(invocation.contains("ssh -p 7443"));
        assert!(invocation.contains("ssh -p 7443 --"));
        assert!(invocation.contains("shell.remote.example"));
        assert!(invocation.contains("--"));
        assert!(invocation.contains("sh"));
        assert!(invocation.contains("-lc"));
        assert!(invocation.contains("cd '"));
    }

    #[test]
    fn tssh_invocation_uses_custom_port_and_remote_script() {
        let authority =
            parse_remote_ssh_authority("https://shell.remote.example:7443").expect("authority");
        let invocation =
            build_remote_invocation(&authority, "cd '/srv/remotes' && nvim", true, false);

        assert!(invocation.starts_with("\"${SHELL:-/bin/sh}\" -lic '"));
        assert!(invocation.contains("tssh -tt -p 7443"));
        assert!(invocation.contains("'shell.remote.example'"));
        assert!(invocation.contains("cd '"));
        assert!(!invocation.contains("mosh --no-init"));
    }

    #[test]
    fn every_supported_transport_ends_options_before_the_destination() {
        let authority =
            parse_remote_ssh_authority("operator@[2001:db8::1]:2222").expect("authority");
        for (use_tssh, use_mosh, transport) in [
            (false, false, "ssh"),
            (true, false, "tssh"),
            (false, true, "mosh"),
        ] {
            let invocation = build_remote_invocation(
                &authority,
                "cd '/srv/remote work' && exec shell",
                use_tssh,
                use_mosh,
            );
            assert!(invocation.contains(transport));
            assert!(invocation.contains("-- '\"'\"'operator@[2001:db8::1]'"));
            assert!(invocation.contains("[2001:db8::1]"));
            assert!(invocation.contains("/srv/remote work"));
        }
    }
}
