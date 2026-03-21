use super::SessionError;
use super::command::{tmux_output_value, tmux_primary_window_target, tmux_run};
use super::options::{
    required_pane_option, required_session_option, set_session_option, show_session_option,
};

const SLOT_GLYPH_PRESET_KEY: &str = "@ezm_slot_glyph_preset";
const BORDER_LABEL_OPTION_KEY: &str = "@ezm_border_label";
const BORDER_FORMAT: &str = "#[align=left]#{?@ezm_border_label,#{@ezm_border_label},#{pane_title}}";
const DEFAULT_SLOT_GLYPH_PRESET: &str = "circled";
const CONNECTED_BORDER_RULE: &str = "────────────────────────────────────────────────────";

const SLOT_COLORS: [&str; 5] = ["#5ac8e0", "#eb6f92", "#7fd77a", "#b58df2", "#f2cd72"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotGlyphPreset {
    Circled,
    Fullwidth,
    Plain,
}

impl SlotGlyphPreset {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "circled" => Some(Self::Circled),
            "fullwidth" => Some(Self::Fullwidth),
            "plain" => Some(Self::Plain),
            _ => None,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Circled => "circled",
            Self::Fullwidth => "fullwidth",
            Self::Plain => "plain",
        }
    }

    const fn slot_label(self, slot_id: u8) -> &'static str {
        match self {
            Self::Circled => match slot_id {
                1 => "\u{2460}",
                2 => "\u{2461}",
                3 => "\u{2462}",
                4 => "\u{2463}",
                5 => "\u{2464}",
                _ => "?",
            },
            Self::Fullwidth => match slot_id {
                1 => "\u{ff11}",
                2 => "\u{ff12}",
                3 => "\u{ff13}",
                4 => "\u{ff14}",
                5 => "\u{ff15}",
                _ => "?",
            },
            Self::Plain => match slot_id {
                1 => "1",
                2 => "2",
                3 => "3",
                4 => "4",
                5 => "5",
                _ => "?",
            },
        }
    }
}

pub(super) fn apply_runtime_style(
    session_name: &str,
    configured_preset: &str,
) -> Result<(), SessionError> {
    let preset = parse_configured_preset(configured_preset)?;
    set_session_option(session_name, SLOT_GLYPH_PRESET_KEY, preset.label())?;

    for slot_id in 1_u8..=5 {
        if slot_is_suspended(session_name, slot_id)? {
            continue;
        }
        let pane_id = required_session_option(session_name, &format!("@ezm_slot_{slot_id}_pane"))?;
        let color = slot_color(slot_id);
        let title = slot_border_label(preset, slot_id, color);
        tmux_run(&["select-pane", "-t", &pane_id, "-T", &title])?;
        tmux_run(&[
            "set-option",
            "-p",
            "-t",
            &pane_id,
            BORDER_LABEL_OPTION_KEY,
            &title,
        ])?;
        apply_slot_text_style(&pane_id, color)?;
    }

    let target = tmux_primary_window_target(session_name)?;
    tmux_run(&[
        "set-window-option",
        "-t",
        &target,
        "pane-border-lines",
        "single",
    ])?;
    tmux_run(&[
        "set-window-option",
        "-t",
        &target,
        "pane-border-indicators",
        "off",
    ])?;
    tmux_run(&[
        "set-window-option",
        "-t",
        &target,
        "pane-border-status",
        "top",
    ])?;
    tmux_run(&[
        "set-window-option",
        "-t",
        &target,
        "pane-border-format",
        BORDER_FORMAT,
    ])?;

    let center_slot = resolve_center_slot(session_name)?;
    tmux_run(&[
        "set-window-option",
        "-t",
        &target,
        "pane-border-style",
        &format!("fg={}", slot_color(center_slot)),
    ])?;

    refresh_active_border(session_name)
}

pub(super) fn apply_runtime_style_defaults(session_name: &str) -> Result<(), SessionError> {
    let configured = show_session_option(session_name, SLOT_GLYPH_PRESET_KEY)?
        .unwrap_or_else(|| String::from(DEFAULT_SLOT_GLYPH_PRESET));
    apply_runtime_style(session_name, configured.as_str())
}

pub(super) fn refresh_active_border(session_name: &str) -> Result<(), SessionError> {
    let target = tmux_primary_window_target(session_name)?;
    let active_slot = active_slot_id(session_name)?;
    tmux_run(&[
        "set-window-option",
        "-t",
        &target,
        "pane-active-border-style",
        &format!("fg={}", slot_color(active_slot)),
    ])
}

pub(super) fn refresh_active_border_for_slot(
    session_name: &str,
    slot_id: u8,
) -> Result<(), SessionError> {
    let target = tmux_primary_window_target(session_name)?;
    tmux_run(&[
        "set-window-option",
        "-t",
        &target,
        "pane-active-border-style",
        &format!("fg={}", slot_color(slot_id)),
    ])
}

fn parse_configured_preset(configured: &str) -> Result<SlotGlyphPreset, SessionError> {
    SlotGlyphPreset::parse(configured).ok_or_else(|| SessionError::TmuxCommandFailed {
        command: String::from("apply-runtime-style"),
        stderr: format!(
            "invalid slot glyph preset `{configured}`; expected one of: circled, fullwidth, plain"
        ),
    })
}

fn resolve_center_slot(session_name: &str) -> Result<u8, SessionError> {
    let mut winner = (1_u8, 0_u16);
    for slot_id in 1_u8..=5 {
        if slot_is_suspended(session_name, slot_id)? {
            continue;
        }
        let pane_id = required_session_option(session_name, &format!("@ezm_slot_{slot_id}_pane"))?;
        if pane_is_dead(&pane_id)? {
            continue;
        }
        let width = tmux_output_value(&["display-message", "-p", "-t", &pane_id, "#{pane_width}"])?
            .trim()
            .parse::<u16>()
            .map_err(|error| SessionError::TmuxCommandFailed {
                command: format!("display-message -p -t {pane_id} #{{pane_width}}"),
                stderr: format!("failed parsing pane width for style center resolution: {error}"),
            })?;
        if width > winner.1 {
            winner = (slot_id, width);
        }
    }
    Ok(winner.0)
}

fn slot_is_suspended(session_name: &str, slot_id: u8) -> Result<bool, SessionError> {
    let key = format!("@ezm_slot_{slot_id}_suspended");
    Ok(show_session_option(session_name, &key)?.as_deref() == Some("1"))
}

fn pane_is_dead(pane_id: &str) -> Result<bool, SessionError> {
    let value = tmux_output_value(&["display-message", "-p", "-t", pane_id, "#{pane_dead}"])?;
    Ok(value.trim() == "1")
}

fn active_slot_id(session_name: &str) -> Result<u8, SessionError> {
    let target = tmux_primary_window_target(session_name)?;
    let active_pane = tmux_output_value(&[
        "list-panes",
        "-t",
        &target,
        "-F",
        "#{pane_id}|#{pane_active}",
    ])?
    .lines()
    .find_map(|line| {
        let mut parts = line.split('|');
        let pane_id = parts.next()?.trim();
        let active = parts.next()?.trim();
        if active == "1" {
            Some(pane_id.to_owned())
        } else {
            None
        }
    })
    .ok_or_else(|| SessionError::TmuxCommandFailed {
        command: format!("list-panes -t {target} -F #{{pane_id}}|#{{pane_active}}"),
        stderr: String::from("failed resolving active pane for style refresh"),
    })?;

    let slot_raw = required_pane_option(session_name, 0, &active_pane, "@ezm_slot_id")?;
    slot_raw
        .trim()
        .parse::<u8>()
        .map_err(|error| SessionError::TmuxCommandFailed {
            command: format!("show-options -p -v -t {active_pane} @ezm_slot_id"),
            stderr: format!("failed parsing active slot id for style refresh: {error}"),
        })
}

fn slot_color(slot_id: u8) -> &'static str {
    SLOT_COLORS
        .get(usize::from(slot_id.saturating_sub(1)))
        .copied()
        .unwrap_or("#5ac8e0")
}

fn slot_border_label(preset: SlotGlyphPreset, slot_id: u8, color: &str) -> String {
    format!(
        "#[fg={color},bold]─·{} ·{CONNECTED_BORDER_RULE}#[default]",
        preset.slot_label(slot_id)
    )
}

fn apply_slot_text_style(pane_id: &str, color: &str) -> Result<(), SessionError> {
    let style = format!("fg={color}");
    tmux_run(&["set-option", "-p", "-t", pane_id, "window-style", &style])?;
    tmux_run(&[
        "set-option",
        "-p",
        "-t",
        pane_id,
        "window-active-style",
        &style,
    ])
}

#[cfg(test)]
mod tests {
    use super::{SlotGlyphPreset, slot_border_label, slot_color};

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
}
