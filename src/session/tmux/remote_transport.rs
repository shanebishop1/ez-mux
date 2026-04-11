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

pub(super) fn build_remote_invocation(
    authority: &ParsedSshAuthority,
    remote_script: &str,
    use_tssh: bool,
    use_mosh: bool,
) -> String {
    if use_tssh {
        return build_tssh_invocation(authority, remote_script);
    }

    if use_mosh {
        return build_mosh_invocation(authority, remote_script);
    }

    build_ssh_invocation(authority, remote_script)
}

fn build_ssh_invocation(authority: &ParsedSshAuthority, remote_script: &str) -> String {
    let mut invocation = String::from("ssh -tt");
    if let Some(port) = authority.port {
        invocation.push_str(" -p ");
        invocation.push_str(&port.to_string());
    }
    invocation.push_str(" '");
    invocation.push_str(&escape_single_quotes(&authority.target));
    invocation.push('\'');
    invocation.push_str(" '");
    invocation.push_str(&escape_single_quotes(remote_script));
    invocation.push('\'');
    invocation
}

fn build_tssh_invocation(authority: &ParsedSshAuthority, remote_script: &str) -> String {
    let mut invocation = String::from("tssh -tt");
    if let Some(port) = authority.port {
        invocation.push_str(" -p ");
        invocation.push_str(&port.to_string());
    }
    invocation.push_str(" '");
    invocation.push_str(&escape_single_quotes(&authority.target));
    invocation.push('\'');
    invocation.push_str(" '");
    invocation.push_str(&escape_single_quotes(remote_script));
    invocation.push('\'');
    invocation
}

fn build_mosh_invocation(authority: &ParsedSshAuthority, remote_script: &str) -> String {
    let mut invocation = String::from("mosh --no-init");
    if let Some(port) = authority.port {
        invocation.push_str(" --ssh='ssh -p ");
        invocation.push_str(&port.to_string());
        invocation.push('\'');
    }
    invocation.push_str(" '");
    invocation.push_str(&escape_single_quotes(&authority.target));
    invocation.push_str("' -- 'sh' '-lc' '");
    invocation.push_str(&escape_single_quotes(remote_script));
    invocation.push('\'');
    invocation
}

fn escape_single_quotes(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}

#[cfg(test)]
mod tests {
    use super::super::remote_authority::parse_remote_ssh_authority;
    use super::{build_remote_invocation, remote_transport_label};

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

        assert!(invocation.contains("ssh -tt -p 7443"));
        assert!(invocation.contains("'shell.remote.example'"));
        assert!(invocation.contains("cd '"));
    }

    #[test]
    fn mosh_invocation_uses_custom_ssh_port_and_remote_script() {
        let authority =
            parse_remote_ssh_authority("https://shell.remote.example:7443").expect("authority");
        let invocation =
            build_remote_invocation(&authority, "cd '/srv/remotes' && nvim", false, true);

        assert!(invocation.contains("mosh --no-init --ssh='ssh -p 7443'"));
        assert!(invocation.contains("'shell.remote.example' -- 'sh' '-lc' '"));
        assert!(invocation.contains("cd '"));
    }

    #[test]
    fn tssh_invocation_uses_custom_port_and_remote_script() {
        let authority =
            parse_remote_ssh_authority("https://shell.remote.example:7443").expect("authority");
        let invocation =
            build_remote_invocation(&authority, "cd '/srv/remotes' && nvim", true, false);

        assert!(invocation.contains("tssh -tt -p 7443"));
        assert!(invocation.contains("'shell.remote.example'"));
        assert!(invocation.contains("cd '"));
        assert!(!invocation.contains("mosh --no-init"));
    }
}
