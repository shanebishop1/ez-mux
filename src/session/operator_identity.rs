use super::SessionError;

/// Resolves the operator identity required for remote-prefix shell routing.
///
/// # Errors
/// Returns an error when remote-prefix routing is active and no non-empty
/// operator identity is configured.
pub fn resolve_operator_identity_for_remote_prefix(
    remote_prefix: Option<&str>,
    configured_operator: Option<&str>,
) -> Result<Option<String>, SessionError> {
    let remote_prefix_active = remote_prefix
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());

    if !remote_prefix_active {
        return Ok(None);
    }

    let operator = configured_operator
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(SessionError::MissingOperatorForRemotePrefix)?;

    Ok(Some(operator.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::resolve_operator_identity_for_remote_prefix;

    #[test]
    fn remote_prefix_requires_non_empty_operator() {
        let error = resolve_operator_identity_for_remote_prefix(Some("/srv/remotes"), None)
            .expect_err("missing operator should fail");
        assert!(
            error
                .to_string()
                .contains("remote-prefix routing requires OPERATOR")
        );
    }

    #[test]
    fn remote_prefix_accepts_explicit_operator() {
        let resolved =
            resolve_operator_identity_for_remote_prefix(Some("/srv/remotes"), Some("alice"))
                .expect("operator should resolve");
        assert_eq!(resolved, Some(String::from("alice")));
    }

    #[test]
    fn local_mode_does_not_require_operator() {
        let resolved = resolve_operator_identity_for_remote_prefix(None, None)
            .expect("local mode should not require operator");
        assert_eq!(resolved, None);
    }
}
