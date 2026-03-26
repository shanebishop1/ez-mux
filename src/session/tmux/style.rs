use std::collections::HashMap;

use super::command::{tmux_output_value, tmux_primary_window_target, tmux_run, tmux_run_batch};
use super::options::canonical_slot_mismatch_error;
use super::SessionError;

const SLOT_GLYPH_PRESET_KEY: &str = "@ezm_slot_glyph_preset";
const BORDER_LABEL_OPTION_KEY: &str = "@ezm_border_label";
const BORDER_FORMAT: &str = "#[align=left]#{?@ezm_border_label,#{@ezm_border_label},#{pane_title}}";
const DEFAULT_SLOT_GLYPH_PRESET: &str = "circled";
const CONNECTED_BORDER_RULE: &str = "────────────────────────────────────────────────────";
const ACTIVE_SLOT_BORDER_STYLE_FORMAT: &str = "fg=#{?#{==:#{@ezm_slot_id},1},#5ac8e0,#{?#{==:#{@ezm_slot_id},2},#eb6f92,#{?#{==:#{@ezm_slot_id},3},#7fd77a,#{?#{==:#{@ezm_slot_id},4},#b58df2,#f2cd72}}}}";

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

pub(super) fn apply_runtime_style_defaults(session_name: &str) -> Result<(), SessionError> {
    apply_runtime_style_defaults_internal(session_name, None)
}

pub(super) fn apply_runtime_style_defaults_for_target(
    session_name: &str,
    target: &str,
) -> Result<(), SessionError> {
    apply_runtime_style_defaults_internal(session_name, Some(target))
}

fn apply_runtime_style_defaults_internal(
    session_name: &str,
    target_override: Option<&str>,
) -> Result<(), SessionError> {
    let command = format!("show-options -t {session_name}");
    let options_output = load_session_options_output(session_name)?;
    let slot_state = parse_session_slot_state(&options_output).map_err(|reason| {
        SessionError::TmuxCommandFailed {
            command: command.clone(),
            stderr: reason,
        }
    })?;
    let configured = parse_slot_glyph_preset_from_options(&options_output)
        .unwrap_or_else(|| String::from(DEFAULT_SLOT_GLYPH_PRESET));
    let preset = parse_configured_preset(configured.as_str())?;
    apply_runtime_style_with_slot_state(session_name, preset, &slot_state, target_override)
}

fn apply_runtime_style_with_slot_state(
    session_name: &str,
    preset: SlotGlyphPreset,
    slot_state: &SessionSlotState,
    target_override: Option<&str>,
) -> Result<(), SessionError> {
    let mut commands = vec![vec![
        String::from("set-option"),
        String::from("-t"),
        session_name.to_owned(),
        String::from(SLOT_GLYPH_PRESET_KEY),
        String::from(preset.label()),
    ]];

    for slot_id in 1_u8..=5 {
        if slot_state.is_suspended(slot_id) {
            continue;
        }
        let pane_id = slot_state.required_pane_id(session_name, slot_id)?;
        let color = slot_color(slot_id);
        let title = slot_border_label(preset, slot_id, color);
        commands.push(vec![
            String::from("select-pane"),
            String::from("-t"),
            pane_id.to_owned(),
            String::from("-T"),
            title.clone(),
        ]);
        commands.push(vec![
            String::from("set-option"),
            String::from("-p"),
            String::from("-t"),
            pane_id.to_owned(),
            String::from(BORDER_LABEL_OPTION_KEY),
            title,
        ]);
        commands.extend(slot_text_style_commands(pane_id, color));
    }

    let target = if let Some(target) = target_override {
        target.to_owned()
    } else {
        tmux_primary_window_target(session_name)?
    };
    commands.push(vec![
        String::from("set-window-option"),
        String::from("-t"),
        target.clone(),
        String::from("pane-border-lines"),
        String::from("single"),
    ]);
    commands.push(vec![
        String::from("set-window-option"),
        String::from("-t"),
        target.clone(),
        String::from("pane-border-indicators"),
        String::from("off"),
    ]);
    commands.push(vec![
        String::from("set-window-option"),
        String::from("-t"),
        target.clone(),
        String::from("pane-border-status"),
        String::from("top"),
    ]);
    commands.push(vec![
        String::from("set-window-option"),
        String::from("-t"),
        target.clone(),
        String::from("pane-border-format"),
        String::from(BORDER_FORMAT),
    ]);

    let center_slot = resolve_center_slot(session_name, slot_state)?;
    commands.push(vec![
        String::from("set-window-option"),
        String::from("-t"),
        target.clone(),
        String::from("pane-border-style"),
        format!("fg={}", slot_color(center_slot)),
    ]);
    commands.push(vec![
        String::from("set-window-option"),
        String::from("-t"),
        target,
        String::from("pane-active-border-style"),
        String::from(ACTIVE_SLOT_BORDER_STYLE_FORMAT),
    ]);

    tmux_run_batch(&commands)
}

pub(super) fn refresh_active_border(session_name: &str) -> Result<(), SessionError> {
    let target = tmux_primary_window_target(session_name)?;
    tmux_run(&[
        "set-window-option",
        "-t",
        &target,
        "pane-active-border-style",
        ACTIVE_SLOT_BORDER_STYLE_FORMAT,
    ])
}

pub(super) fn refresh_active_border_for_slot(
    session_name: &str,
    _slot_id: u8,
) -> Result<(), SessionError> {
    refresh_active_border(session_name)
}

fn parse_configured_preset(configured: &str) -> Result<SlotGlyphPreset, SessionError> {
    SlotGlyphPreset::parse(configured).ok_or_else(|| SessionError::TmuxCommandFailed {
        command: String::from("apply-runtime-style"),
        stderr: format!(
            "invalid slot glyph preset `{configured}`; expected one of: circled, fullwidth, plain"
        ),
    })
}

fn parse_slot_glyph_preset_from_options(output: &str) -> Option<String> {
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let (key, value) = line.split_once(' ')?;
        if key != SLOT_GLYPH_PRESET_KEY {
            continue;
        }
        let normalized = normalize_session_option_value(value);
        if normalized.is_empty() {
            return None;
        }
        return Some(normalized);
    }

    None
}

fn resolve_center_slot(
    session_name: &str,
    slot_state: &SessionSlotState,
) -> Result<u8, SessionError> {
    let pane_metrics = load_pane_metrics(session_name)?;
    let mut winner = (1_u8, 0_u16);
    for slot_id in 1_u8..=5 {
        if slot_state.is_suspended(slot_id) {
            continue;
        }
        let pane_id = slot_state.required_pane_id(session_name, slot_id)?;
        let metrics = pane_metrics.get(pane_id).ok_or_else(|| {
            canonical_slot_mismatch_error(
                session_name,
                &format!("slot {slot_id} pane {pane_id} missing from list-panes snapshot"),
            )
        })?;
        if metrics.dead {
            continue;
        }
        if metrics.width > winner.1 {
            winner = (slot_id, metrics.width);
        }
    }
    Ok(winner.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PaneMetrics {
    dead: bool,
    width: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionSlotState {
    suspended_by_slot: [bool; 5],
    pane_by_slot: [Option<String>; 5],
}

impl SessionSlotState {
    fn is_suspended(&self, slot_id: u8) -> bool {
        self.suspended_by_slot[slot_index(slot_id)]
    }

    fn required_pane_id<'a>(
        &'a self,
        session_name: &str,
        slot_id: u8,
    ) -> Result<&'a str, SessionError> {
        self.pane_by_slot[slot_index(slot_id)]
            .as_deref()
            .ok_or_else(|| {
                canonical_slot_mismatch_error(
                    session_name,
                    &format!("missing required session option @ezm_slot_{slot_id}_pane"),
                )
            })
    }
}

fn load_session_options_output(session_name: &str) -> Result<String, SessionError> {
    tmux_output_value(&["show-options", "-t", session_name])
}

fn parse_session_slot_state(output: &str) -> Result<SessionSlotState, String> {
    let mut suspended_by_slot = [false; 5];
    let mut pane_by_slot: [Option<String>; 5] = std::array::from_fn(|_| None);

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Some((key, value)) = line.split_once(' ') else {
            continue;
        };
        let Some((slot_idx, suffix)) = slot_index_and_suffix(key) else {
            continue;
        };
        let value = normalize_session_option_value(value);

        match suffix {
            "suspended" => suspended_by_slot[slot_idx] = value == "1",
            "pane" => {
                if value.is_empty() {
                    return Err(format!("slot {} pane option has empty value", slot_idx + 1));
                }
                pane_by_slot[slot_idx] = Some(value);
            }
            _ => {}
        }
    }

    Ok(SessionSlotState {
        suspended_by_slot,
        pane_by_slot,
    })
}

fn slot_index_and_suffix(key: &str) -> Option<(usize, &str)> {
    let rest = key.strip_prefix("@ezm_slot_")?;
    let (slot, suffix) = rest.split_once('_')?;
    let slot_id = slot.parse::<u8>().ok()?;
    if !(1..=5).contains(&slot_id) {
        return None;
    }
    Some((usize::from(slot_id - 1), suffix))
}

fn normalize_session_option_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        let quote = trimmed.as_bytes()[0];
        if (quote == b'"' || quote == b'\'') && trimmed.as_bytes()[trimmed.len() - 1] == quote {
            return trimmed[1..trimmed.len() - 1].to_owned();
        }
    }
    trimmed.to_owned()
}

fn load_pane_metrics(session_name: &str) -> Result<HashMap<String, PaneMetrics>, SessionError> {
    let command =
        format!("list-panes -t {session_name} -F #{{pane_id}}|#{{pane_dead}}|#{{pane_width}}");
    let output = tmux_output_value(&[
        "list-panes",
        "-t",
        session_name,
        "-F",
        "#{pane_id}|#{pane_dead}|#{pane_width}",
    ])?;
    parse_pane_metrics(&output).map_err(|reason| SessionError::TmuxCommandFailed {
        command,
        stderr: reason,
    })
}

fn parse_pane_metrics(output: &str) -> Result<HashMap<String, PaneMetrics>, String> {
    let mut metrics = HashMap::new();

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let mut parts = line.split('|');
        let pane_id = parts
            .next()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("malformed list-panes row: {line}"))?;
        let dead = parts
            .next()
            .ok_or_else(|| format!("missing pane_dead in list-panes row: {line}"))?;
        let width = parts
            .next()
            .ok_or_else(|| format!("missing pane_width in list-panes row: {line}"))?;

        let dead = match dead {
            "0" => false,
            "1" => true,
            other => return Err(format!("invalid pane_dead value `{other}` in row: {line}")),
        };
        let width = width
            .parse::<u16>()
            .map_err(|error| format!("invalid pane_width `{width}` in row `{line}`: {error}"))?;

        metrics.insert(pane_id.to_owned(), PaneMetrics { dead, width });
    }

    Ok(metrics)
}

fn slot_index(slot_id: u8) -> usize {
    usize::from(slot_id.saturating_sub(1))
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

fn slot_text_style_commands(pane_id: &str, color: &str) -> [Vec<String>; 2] {
    let style = format!("fg={color}");
    [
        vec![
            String::from("set-option"),
            String::from("-p"),
            String::from("-t"),
            pane_id.to_owned(),
            String::from("window-style"),
            style.clone(),
        ],
        vec![
            String::from("set-option"),
            String::from("-p"),
            String::from("-t"),
            pane_id.to_owned(),
            String::from("window-active-style"),
            style,
        ],
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        parse_pane_metrics, parse_session_slot_state, parse_slot_glyph_preset_from_options,
        slot_border_label, slot_color, SlotGlyphPreset,
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
}
