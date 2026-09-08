use super::SessionError;
use std::net::Ipv6Addr;

const REDACTED_SECRET_VALUE: &str = "<redacted>";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ParsedSshAuthority {
    pub(super) target: String,
    pub(super) port: Option<u16>,
}

pub(super) fn parse_remote_ssh_authority(
    remote_server_url: &str,
) -> Result<ParsedSshAuthority, SessionError> {
    if remote_server_url.chars().any(char::is_control) {
        return Err(invalid_remote_authority(
            remote_server_url,
            "authority contains control characters",
        ));
    }

    let raw = remote_server_url.trim();
    if raw.is_empty() {
        return Err(invalid_remote_authority(raw, "authority is empty"));
    }

    let authority = if let Some((scheme, remainder)) = raw.split_once("://") {
        validate_scheme(raw, scheme)?;
        remainder.split('/').next().unwrap_or_default()
    } else {
        raw.split('/').next().unwrap_or_default()
    }
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
        validate_username(raw, user)?;
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

fn validate_scheme(raw: &str, scheme: &str) -> Result<(), SessionError> {
    let mut characters = scheme.chars();
    let Some(first) = characters.next() else {
        return Err(invalid_remote_authority(raw, "URL scheme is empty"));
    };
    if !first.is_ascii_alphabetic()
        || !characters.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
    {
        return Err(invalid_remote_authority(raw, "URL scheme is malformed"));
    }

    Ok(())
}

fn validate_username(raw: &str, username: &str) -> Result<(), SessionError> {
    if username.starts_with('-') {
        return Err(invalid_remote_authority(raw, "username is option-like"));
    }
    if username.contains(':') {
        return Err(invalid_remote_authority(
            raw,
            "password-bearing userinfo is unsupported",
        ));
    }
    if !username
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return Err(invalid_remote_authority(raw, "username is malformed"));
    }

    Ok(())
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
    if host_inner.parse::<Ipv6Addr>().is_err() {
        return Err(invalid_remote_authority(
            raw,
            "bracketed host must be a valid IPv6 address",
        ));
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
    if host.starts_with('-') {
        return Err(invalid_remote_authority(raw, "host is option-like"));
    }
    if !host
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return Err(invalid_remote_authority(
            raw,
            "host contains unsupported characters",
        ));
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
        value: redact_remote_authority_value(value),
        reason: reason.into(),
    }
}

pub(super) fn redact_remote_authority_value(value: &str) -> String {
    let (scheme_prefix, remainder) = if let Some((scheme, rest)) = value.split_once("://") {
        (format!("{scheme}://"), rest)
    } else {
        (String::new(), value)
    };

    let (authority, suffix) = if let Some(separator) = remainder.find('/') {
        (&remainder[..separator], &remainder[separator..])
    } else {
        (remainder, "")
    };

    sanitize_diagnostic(&format!(
        "{scheme_prefix}{}{suffix}",
        redact_authority_userinfo_secret(authority)
    ))
}

fn sanitize_diagnostic(value: &str) -> String {
    value.chars().fold(String::new(), |mut rendered, ch| {
        if ch.is_control() {
            use std::fmt::Write;
            let _ = write!(rendered, "\\u{{{:x}}}", ch as u32);
        } else {
            rendered.push(ch);
        }
        rendered
    })
}

fn redact_authority_userinfo_secret(authority: &str) -> String {
    let Some((userinfo, host_port)) = authority.rsplit_once('@') else {
        return authority.to_owned();
    };

    let Some((username, _secret)) = userinfo.split_once(':') else {
        return authority.to_owned();
    };

    format!("{username}:{REDACTED_SECRET_VALUE}@{host_port}")
}

#[cfg(test)]
mod tests {
    use super::{REDACTED_SECRET_VALUE, parse_remote_ssh_authority};

    #[test]
    fn accepts_hostname_and_port_from_url() {
        let parsed = parse_remote_ssh_authority("https://shell.remote.example:7443/path")
            .expect("authority should parse");
        assert_eq!(parsed.target, "shell.remote.example");
        assert_eq!(parsed.port, Some(7443));
    }

    #[test]
    fn accepts_plain_host_user_and_port_forms() {
        let parsed = parse_remote_ssh_authority("operator@shell.remote.example:2222")
            .expect("authority should parse");
        assert_eq!(parsed.target, "operator@shell.remote.example");
        assert_eq!(parsed.port, Some(2222));
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
    fn rejects_option_like_destinations() {
        for value in ["-F/tmp/ssh-config", "-oProxyCommand=sentinel"] {
            let error = parse_remote_ssh_authority(value).expect_err("option should fail");
            assert!(error.to_string().contains("option-like"));
        }
    }

    #[test]
    fn rejects_malformed_usernames() {
        for value in ["@shell.remote.example", "bad user@shell.remote.example"] {
            let error = parse_remote_ssh_authority(value).expect_err("username should fail");
            assert!(error.to_string().contains("invalid remote ssh authority"));
        }
    }

    #[test]
    fn rejects_password_userinfo_without_leaking_the_password() {
        let error = parse_remote_ssh_authority("operator:unique-secret@shell.remote.example")
            .expect_err("password userinfo should fail");
        let rendered = error.to_string();
        assert!(rendered.contains("password-bearing userinfo is unsupported"));
        assert!(!rendered.contains("unique-secret"));
        assert!(rendered.contains(REDACTED_SECRET_VALUE));
    }

    #[test]
    fn rejects_control_characters_and_redacts_them_in_diagnostics() {
        let error = parse_remote_ssh_authority("shell.remote\n.example")
            .expect_err("control character should fail");
        let rendered = error.to_string();
        assert!(rendered.contains("control characters"));
        assert!(rendered.contains("\\u{a}"));
        assert!(!rendered.contains('\n'));
    }

    #[test]
    fn rejects_unbracketed_ipv6_authority() {
        let error =
            parse_remote_ssh_authority("2001:db8::1").expect_err("unbracketed IPv6 should fail");
        let rendered = error.to_string();
        assert!(rendered.contains("wrap IPv6 hosts in `[]`"));
    }

    #[test]
    fn remote_authority_error_redacts_userinfo_password() {
        let error =
            parse_remote_ssh_authority("https://operator:super-secret@shell.remote.example:")
                .expect_err("empty port should fail");
        let rendered = error.to_string();

        assert!(rendered.contains("operator:"));
        assert!(rendered.contains(REDACTED_SECRET_VALUE));
        assert!(!rendered.contains("super-secret"));
        assert!(rendered.contains("invalid remote ssh authority"));
    }
}
