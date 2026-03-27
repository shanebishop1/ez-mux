use super::SessionError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ParsedSshAuthority {
    pub(super) target: String,
    pub(super) port: Option<u16>,
}

pub(super) fn parse_remote_ssh_authority(
    remote_server_url: &str,
) -> Result<ParsedSshAuthority, SessionError> {
    let raw = remote_server_url.trim();
    if raw.is_empty() {
        return Err(invalid_remote_authority(raw, "authority is empty"));
    }

    let authority = raw
        .split_once("://")
        .map_or(raw, |(_, remainder)| remainder)
        .split('/')
        .next()
        .unwrap_or_default()
        .trim();

    if authority.is_empty() {
        return Err(invalid_remote_authority(raw, "host is empty"));
    }

    let (target, port) = parse_authority(raw, authority)?;
    Ok(ParsedSshAuthority { target, port })
}

fn parse_authority(raw: &str, authority: &str) -> Result<(String, Option<u16>), SessionError> {
    if contains_whitespace(authority) {
        return Err(invalid_remote_authority(
            raw,
            "authority contains whitespace",
        ));
    }

    let delimiter_count = authority.chars().filter(|ch| *ch == '@').count();
    if delimiter_count > 1 {
        return Err(invalid_remote_authority(
            raw,
            "authority contains multiple `@` delimiters",
        ));
    }

    let (user_prefix, host_port) = if let Some((user, host_port)) = authority.split_once('@') {
        if user.is_empty() {
            return Err(invalid_remote_authority(
                raw,
                "user segment before `@` is empty",
            ));
        }
        (Some(user), host_port)
    } else {
        (None, authority)
    };

    if host_port.is_empty() {
        return Err(invalid_remote_authority(raw, "host is empty"));
    }

    let (host, port) = if host_port.starts_with('[') {
        parse_bracketed_host_and_port(raw, host_port)?
    } else {
        parse_unbracketed_host_and_port(raw, host_port)?
    };

    let target = if let Some(user_prefix) = user_prefix {
        format!("{user_prefix}@{host}")
    } else {
        host
    };

    Ok((target, port))
}

fn parse_bracketed_host_and_port(
    raw: &str,
    host_port: &str,
) -> Result<(String, Option<u16>), SessionError> {
    let Some(closing) = host_port.find(']') else {
        return Err(invalid_remote_authority(
            raw,
            "missing closing `]` for bracketed host",
        ));
    };

    let host = &host_port[..=closing];
    let host_inner = &host[1..host.len() - 1];
    if host_inner.is_empty() {
        return Err(invalid_remote_authority(raw, "host is empty"));
    }
    if contains_whitespace(host_inner) {
        return Err(invalid_remote_authority(raw, "host contains whitespace"));
    }

    let remainder = host_port[(closing + 1)..].trim();
    if remainder.is_empty() {
        return Ok((host.to_owned(), None));
    }

    let Some(raw_port) = remainder.strip_prefix(':') else {
        return Err(invalid_remote_authority(
            raw,
            format!("unexpected trailing segment `{remainder}` after bracketed host"),
        ));
    };

    let port = parse_port(raw, raw_port)?;
    Ok((host.to_owned(), Some(port)))
}

fn parse_unbracketed_host_and_port(
    raw: &str,
    host_port: &str,
) -> Result<(String, Option<u16>), SessionError> {
    if host_port.contains('[') || host_port.contains(']') {
        return Err(invalid_remote_authority(
            raw,
            "authority contains unmatched bracket delimiter",
        ));
    }

    let colon_count = host_port.chars().filter(|ch| *ch == ':').count();
    if colon_count > 1 {
        return Err(invalid_remote_authority(
            raw,
            "unbracketed IPv6-style authority is unsupported; wrap IPv6 hosts in `[]`",
        ));
    }

    if let Some((host, raw_port)) = host_port.rsplit_once(':') {
        validate_host(raw, host)?;
        let port = parse_port(raw, raw_port)?;
        return Ok((host.to_owned(), Some(port)));
    }

    validate_host(raw, host_port)?;
    Ok((host_port.to_owned(), None))
}

fn validate_host(raw: &str, host: &str) -> Result<(), SessionError> {
    if host.is_empty() {
        return Err(invalid_remote_authority(raw, "host is empty"));
    }
    if contains_whitespace(host) {
        return Err(invalid_remote_authority(raw, "host contains whitespace"));
    }

    Ok(())
}

fn parse_port(raw: &str, raw_port: &str) -> Result<u16, SessionError> {
    if raw_port.is_empty() {
        return Err(invalid_remote_authority(raw, "port is empty after `:`"));
    }
    if !raw_port.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(invalid_remote_authority(
            raw,
            format!("port `{raw_port}` must be numeric"),
        ));
    }

    let port = raw_port.parse::<u16>().map_err(|_| {
        invalid_remote_authority(raw, format!("port `{raw_port}` is out of range (1-65535)"))
    })?;
    if port == 0 {
        return Err(invalid_remote_authority(
            raw,
            "port must be in range 1-65535",
        ));
    }

    Ok(port)
}

fn contains_whitespace(value: &str) -> bool {
    value.chars().any(char::is_whitespace)
}

fn invalid_remote_authority(value: &str, reason: impl Into<String>) -> SessionError {
    SessionError::InvalidRemoteSshAuthority {
        value: value.to_owned(),
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_remote_ssh_authority;

    #[test]
    fn accepts_hostname_and_port_from_url() {
        let parsed = parse_remote_ssh_authority("https://shell.remote.example:7443/path")
            .expect("authority should parse");
        assert_eq!(parsed.target, "shell.remote.example");
        assert_eq!(parsed.port, Some(7443));
    }

    #[test]
    fn accepts_user_prefixed_bracketed_ipv6_authority() {
        let parsed = parse_remote_ssh_authority("ssh://operator@[2001:db8::1]:2222")
            .expect("authority should parse");
        assert_eq!(parsed.target, "operator@[2001:db8::1]");
        assert_eq!(parsed.port, Some(2222));
    }

    #[test]
    fn rejects_empty_host() {
        let error =
            parse_remote_ssh_authority("https://:7443").expect_err("empty host should fail");
        let rendered = error.to_string();
        assert!(rendered.contains("invalid remote ssh authority"));
        assert!(rendered.contains("host is empty"));
    }

    #[test]
    fn rejects_non_numeric_port() {
        let error = parse_remote_ssh_authority("https://shell.remote.example:port")
            .expect_err("non-numeric port should fail");
        let rendered = error.to_string();
        assert!(rendered.contains("port `port` must be numeric"));
    }

    #[test]
    fn rejects_unbracketed_ipv6_authority() {
        let error =
            parse_remote_ssh_authority("2001:db8::1").expect_err("unbracketed IPv6 should fail");
        let rendered = error.to_string();
        assert!(rendered.contains("wrap IPv6 hosts in `[]`"));
    }
}
