pub const DEFAULT_CENTER_WIDTH_PCT: u8 = 38;
pub const CENTER_WIDTH_TOLERANCE_PCT: u8 = 3;
pub const THREE_PANE_SIDE_TARGET_PCT: u8 = 30;
pub const THREE_PANE_CENTER_TARGET_PCT: u8 = 40;
pub const THREE_PANE_TARGET_TOLERANCE_PCT: u8 = 3;

#[must_use]
/// # Panics
/// Panics if intermediate width math exceeds `u16` bounds, which should be unreachable
/// because all inputs and clamping derive from `window_width: u16`.
pub fn canonical_five_pane_column_widths(
    window_width: u16,
    center_width_pct: u8,
) -> (u16, u16, u16) {
    if window_width < 3 {
        return (1, 1, 1);
    }

    let mut center = (u32::from(window_width) * u32::from(center_width_pct)) / 100;
    center = center.clamp(1, u32::from(window_width - 2));

    let side_total = u32::from(window_width) - center;
    let left = side_total / 2;
    let right = side_total - left;

    (
        u16::try_from(left).expect("left width must fit u16 after bounded arithmetic"),
        u16::try_from(center).expect("center width must fit u16 after bounded arithmetic"),
        u16::try_from(right).expect("right width must fit u16 after bounded arithmetic"),
    )
}

#[must_use]
/// # Panics
/// Panics if intermediate width math exceeds `u16` bounds, which should be unreachable
/// because all inputs and clamping derive from `window_width: u16`.
pub fn three_pane_target_widths(window_width: u16) -> (u16, u16, u16) {
    if window_width < 3 {
        return (1, 1, 1);
    }

    let left = (u32::from(window_width) * u32::from(THREE_PANE_SIDE_TARGET_PCT)) / 100;
    let center = (u32::from(window_width) * u32::from(THREE_PANE_CENTER_TARGET_PCT)) / 100;
    let right = u32::from(window_width) - left - center;

    (
        u16::try_from(left).expect("left width must fit u16 after bounded arithmetic"),
        u16::try_from(center).expect("center width must fit u16 after bounded arithmetic"),
        u16::try_from(right).expect("right width must fit u16 after bounded arithmetic"),
    )
}

#[must_use]
pub fn three_pane_widths_within_tolerance(
    left: u16,
    center: u16,
    right: u16,
    window_width: u16,
) -> bool {
    if window_width == 0 {
        return false;
    }

    let left_pct = i32::from(left) * 100 / i32::from(window_width);
    let center_pct = i32::from(center) * 100 / i32::from(window_width);
    let right_pct = i32::from(right) * 100 / i32::from(window_width);
    let tolerance = i32::from(THREE_PANE_TARGET_TOLERANCE_PCT);

    (left_pct - i32::from(THREE_PANE_SIDE_TARGET_PCT)).abs() <= tolerance
        && (center_pct - i32::from(THREE_PANE_CENTER_TARGET_PCT)).abs() <= tolerance
        && (right_pct - i32::from(THREE_PANE_SIDE_TARGET_PCT)).abs() <= tolerance
}

#[cfg(test)]
mod tests {
    use super::canonical_five_pane_column_widths;
    use super::three_pane_target_widths;
    use super::three_pane_widths_within_tolerance;
    use super::CENTER_WIDTH_TOLERANCE_PCT;
    use super::DEFAULT_CENTER_WIDTH_PCT;
    use super::THREE_PANE_CENTER_TARGET_PCT;
    use super::THREE_PANE_SIDE_TARGET_PCT;
    use super::THREE_PANE_TARGET_TOLERANCE_PCT;

    #[test]
    fn column_widths_are_deterministic_and_sum_to_window_width() {
        let first = canonical_five_pane_column_widths(237, DEFAULT_CENTER_WIDTH_PCT);
        let second = canonical_five_pane_column_widths(237, DEFAULT_CENTER_WIDTH_PCT);

        assert_eq!(first, second);
        assert_eq!(u32::from(first.0 + first.1 + first.2), 237);
    }

    #[test]
    fn center_width_respects_reference_target_with_tolerance() {
        let (left, center, right) =
            canonical_five_pane_column_widths(211, DEFAULT_CENTER_WIDTH_PCT);
        let center_pct = i32::from(center) * 100 / 211;
        let delta = (center_pct - i32::from(DEFAULT_CENTER_WIDTH_PCT)).abs();

        assert!(left > 0);
        assert!(right > 0);
        assert!(delta <= i32::from(CENTER_WIDTH_TOLERANCE_PCT));
    }

    #[test]
    fn three_pane_width_targets_sum_and_stay_center_dominant() {
        let (left, center, right) = three_pane_target_widths(101);

        assert_eq!(left + center + right, 101);
        assert!(center >= left);
        assert!(center >= right);
        assert!((i32::from(left) - 30).abs() <= 1);
        assert!((i32::from(right) - 30).abs() <= 1);
        assert!((i32::from(center) - 40).abs() <= 1);
    }

    #[test]
    fn three_pane_tolerance_policy_allows_single_cell_rounding() {
        assert!(three_pane_widths_within_tolerance(30, 40, 30, 100));
        assert!(three_pane_widths_within_tolerance(29, 41, 30, 100));
        assert!(three_pane_widths_within_tolerance(30, 39, 31, 100));
        assert!(three_pane_widths_within_tolerance(24, 32, 22, 80));
        assert!(!three_pane_widths_within_tolerance(27, 46, 27, 100));
        assert_eq!(THREE_PANE_TARGET_TOLERANCE_PCT, 3);
        assert_eq!(THREE_PANE_SIDE_TARGET_PCT, 30);
        assert_eq!(THREE_PANE_CENTER_TARGET_PCT, 40);
    }
}
