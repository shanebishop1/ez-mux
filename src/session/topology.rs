pub const DEFAULT_CENTER_WIDTH_PCT: u8 = 38;
pub const CENTER_WIDTH_TOLERANCE_PCT: u8 = 3;

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

#[cfg(test)]
mod tests {
    use super::CENTER_WIDTH_TOLERANCE_PCT;
    use super::DEFAULT_CENTER_WIDTH_PCT;
    use super::canonical_five_pane_column_widths;

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
}
