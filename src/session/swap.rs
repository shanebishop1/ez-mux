#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneWidthSample {
    pub slot_id: u8,
    pub pane_id: String,
    pub width: u16,
}

#[must_use]
pub fn pick_center_pane(samples: &[PaneWidthSample]) -> Option<&str> {
    samples
        .iter()
        .max_by_key(|sample| (sample.width, u16::from(sample.slot_id)))
        .map(|sample| sample.pane_id.as_str())
}

#[must_use]
pub fn supports_zoom_flag_fallback(stderr: &str) -> bool {
    let lowered = stderr.to_ascii_lowercase();
    lowered.contains("unknown option") || lowered.contains("usage:")
}

#[cfg(test)]
mod tests {
    use super::PaneWidthSample;
    use super::pick_center_pane;
    use super::supports_zoom_flag_fallback;

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
    fn zoom_flag_fallback_matches_tmux_unknown_option_errors() {
        assert!(supports_zoom_flag_fallback("unknown option -- Z"));
        assert!(supports_zoom_flag_fallback("usage: swap-pane [-dDU]"));
        assert!(!supports_zoom_flag_fallback("pane not found"));
    }
}
