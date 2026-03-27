use super::{
    SlotGlyphPreset, parse_pane_metrics, parse_session_slot_state,
    parse_slot_glyph_preset_from_options, slot_border_label, slot_color,
};

#[test]
fn glyph_presets_parse_and_render_slot_labels() {
    let circled = SlotGlyphPreset::parse("circled").expect("circled");
    let fullwidth = SlotGlyphPreset::parse("fullwidth").expect("fullwidth");
    let plain = SlotGlyphPreset::parse("plain").expect("plain");

    assert_eq!(circled.slot_label(1), "\u{2460}");
    assert_eq!(fullwidth.slot_label(2), "\u{ff12}");
    assert_eq!(plain.slot_label(3), "3");
    assert!(SlotGlyphPreset::parse("unknown").is_none());
}

#[test]
fn slot_border_label_matches_focus5_connected_line_prefix() {
    let preset = SlotGlyphPreset::parse("circled").expect("circled");
    let label = slot_border_label(preset, 2, "#eb6f92");

    assert!(label.starts_with("#[fg=#eb6f92,bold]─·② ·─"));
    assert!(label.contains("────────────────"));
    assert!(!label.contains("-- "));
}

#[test]
fn slot_palette_keeps_center_slot_blue_mapping() {
    assert_eq!(slot_color(1), "#5ac8e0");
    assert_eq!(slot_color(2), "#eb6f92");
    assert_eq!(slot_color(3), "#7fd77a");
    assert_eq!(slot_color(4), "#b58df2");
    assert_eq!(slot_color(5), "#f2cd72");
}

#[test]
fn parse_session_slot_state_extracts_slot_pane_and_suspended_values() {
    let output = "@ezm_slot_1_pane %10\n@ezm_slot_2_pane %8\n@ezm_slot_4_suspended 1\n";
    let state = parse_session_slot_state(output).expect("parse state");

    assert_eq!(state.pane_by_slot[0].as_deref(), Some("%10"));
    assert_eq!(state.pane_by_slot[1].as_deref(), Some("%8"));
    assert!(state.suspended_by_slot[3]);
    assert!(!state.suspended_by_slot[0]);
}

#[test]
fn parse_session_slot_state_unquotes_tmux_rendered_string_values() {
    let output = "@ezm_slot_1_pane \"%28\"\n@ezm_slot_4_suspended \"1\"\n";
    let state = parse_session_slot_state(output).expect("parse quoted state");

    assert_eq!(state.pane_by_slot[0].as_deref(), Some("%28"));
    assert!(state.suspended_by_slot[3]);
}

#[test]
fn parse_slot_glyph_preset_from_options_reads_unquoted_and_quoted_values() {
    let unquoted = "@ezm_slot_glyph_preset plain\n";
    assert_eq!(
        parse_slot_glyph_preset_from_options(unquoted).as_deref(),
        Some("plain")
    );

    let quoted = "@ezm_slot_glyph_preset \"fullwidth\"\n";
    assert_eq!(
        parse_slot_glyph_preset_from_options(quoted).as_deref(),
        Some("fullwidth")
    );
}

#[test]
fn parse_pane_metrics_reads_dead_and_width_columns() {
    let output = "%8|0|120\n%9|1|80\n";
    let metrics = parse_pane_metrics(output).expect("parse pane metrics");

    assert_eq!(metrics.get("%8").map(|item| item.dead), Some(false));
    assert_eq!(metrics.get("%8").map(|item| item.width), Some(120));
    assert_eq!(metrics.get("%9").map(|item| item.dead), Some(true));
    assert_eq!(metrics.get("%9").map(|item| item.width), Some(80));
}

#[test]
fn parse_pane_metrics_rejects_malformed_lines() {
    let err = parse_pane_metrics("%8|oops|120").expect_err("invalid dead value should fail");
    assert!(err.contains("invalid pane_dead"));
}
