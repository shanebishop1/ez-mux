#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneWidthSample {
    pub slot_id: u8,
    pub pane_id: String,
    pub width: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoomFlagSupport {
    Supported,
    Unsupported,
    Unknown,
}

#[must_use]
pub fn pick_center_pane(samples: &[PaneWidthSample]) -> Option<&str> {
    samples
        .iter()
        .max_by_key(|sample| (sample.width, u16::from(sample.slot_id)))
        .map(|sample| sample.pane_id.as_str())
}

#[must_use]
pub fn zoom_flag_support_for_command(command_listing: &str, command_name: &str) -> ZoomFlagSupport {
    for line in command_listing.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Some(first_token) = trimmed.split_ascii_whitespace().next() else {
            continue;
        };

        if first_token != command_name {
            continue;
        }

        return if option_group_contains_zoom_flag(trimmed) {
            ZoomFlagSupport::Supported
        } else {
            ZoomFlagSupport::Unsupported
        };
    }

    ZoomFlagSupport::Unknown
}

#[must_use]
pub fn tmux_diagnostics_exit_status(diagnostics: &str) -> Option<i32> {
    let field = diagnostics
        .strip_prefix("status=")?
        .split(';')
        .next()?
        .trim();
    if field.eq_ignore_ascii_case("signal") {
        return None;
    }

    field.parse::<i32>().ok()
}

fn option_group_contains_zoom_flag(command_usage: &str) -> bool {
    let chars: Vec<char> = command_usage.chars().collect();
    let mut index = 0_usize;

    while index < chars.len() {
        if chars[index] == '[' {
            let mut end = index + 1;
            while end < chars.len() && chars[end] != ']' {
                end += 1;
            }

            if end > index + 1 {
                let group = &chars[index + 1..end];
                if group.first() == Some(&'-') && group.contains(&'Z') {
                    return true;
                }
            }

            if end == chars.len() {
                break;
            }
            index = end;
        }

        index += 1;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::pick_center_pane;
    use super::tmux_diagnostics_exit_status;
    use super::zoom_flag_support_for_command;
    use super::PaneWidthSample;
    use super::ZoomFlagSupport;

    #[test]
    fn picks_widest_pane_as_center_target() {
        let samples = vec![
            PaneWidthSample {
                slot_id: 1,
                pane_id: String::from("%1"),
                width: 20,
            },
            PaneWidthSample {
                slot_id: 2,
                pane_id: String::from("%2"),
                width: 30,
            },
            PaneWidthSample {
                slot_id: 3,
                pane_id: String::from("%3"),
                width: 25,
            },
        ];

        assert_eq!(pick_center_pane(&samples), Some("%2"));
    }

    #[test]
    fn resolves_ties_deterministically_to_higher_slot_id() {
        let samples = vec![
            PaneWidthSample {
                slot_id: 2,
                pane_id: String::from("%2"),
                width: 30,
            },
            PaneWidthSample {
                slot_id: 5,
                pane_id: String::from("%5"),
                width: 30,
            },
        ];

        assert_eq!(pick_center_pane(&samples), Some("%5"));
    }

    #[test]
    fn detects_zoom_flag_support_from_list_commands_output() {
        let listing = "swap-pane (swapp) [-dDUZ] [-s src-pane] [-t dst-pane]\nselect-pane (selectp) [-DdeLlMmRUZ] [-T title] [-t target-pane]";

        assert_eq!(
            zoom_flag_support_for_command(listing, "swap-pane"),
            ZoomFlagSupport::Supported
        );
        assert_eq!(
            zoom_flag_support_for_command(listing, "select-pane"),
            ZoomFlagSupport::Supported
        );
    }

    #[test]
    fn detects_when_zoom_flag_is_not_supported() {
        let listing = "swap-pane (swapp) [-dDU] [-s src-pane] [-t dst-pane]\nselect-pane (selectp) [-DdeLlMmRU] [-T title] [-t target-pane]";

        assert_eq!(
            zoom_flag_support_for_command(listing, "swap-pane"),
            ZoomFlagSupport::Unsupported
        );
        assert_eq!(
            zoom_flag_support_for_command(listing, "select-pane"),
            ZoomFlagSupport::Unsupported
        );
    }

    #[test]
    fn returns_unknown_when_command_metadata_is_missing() {
        let listing = "new-window (neww) [-abdkPS]";
        assert_eq!(
            zoom_flag_support_for_command(listing, "swap-pane"),
            ZoomFlagSupport::Unknown
        );
    }

    #[test]
    fn parses_exit_status_from_tmux_diagnostics() {
        assert_eq!(
            tmux_diagnostics_exit_status("status=1; stdout=\"\"; stderr=\"oops\""),
            Some(1)
        );
        assert_eq!(
            tmux_diagnostics_exit_status("status=127; stdout=\"\"; stderr=\"oops\""),
            Some(127)
        );
        assert_eq!(
            tmux_diagnostics_exit_status("status=signal; stdout=\"\"; stderr=\"oops\""),
            None
        );
        assert_eq!(tmux_diagnostics_exit_status("oops"), None);
    }
}
