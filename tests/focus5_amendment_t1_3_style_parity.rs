#[path = "support/focus5_amendment_t1_1_red_support.rs"]
mod red_support;
mod support;

use std::fs;

use red_support::{center_pane_id, extract_stdout_field, read_pane_widths, read_slot_snapshot};
use support::foundation_harness::FoundationHarness;

const BORDER_FORMAT_EXPECTED: &str =
    "#[align=left]#{?@ezm_border_label,#{@ezm_border_label},#{pane_title}}";
const SLOT_COLORS: [(u8, &str); 5] = [
    (1, "#5ac8e0"),
    (2, "#eb6f92"),
    (3, "#7fd77a"),
    (4, "#b58df2"),
    (5, "#f2cd72"),
];

#[test]
#[allow(clippy::too_many_lines)]
fn t1_3_restores_focus5_connected_border_palette_and_shell_text_inheritance() {
    let harness = FoundationHarness::new_for_suite("focus5-amendment-t1-3")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let launch = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("startup launch failed: {error}"));
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();

    let pane_widths = read_pane_widths(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading pane widths: {error}"));
    let center_pane = center_pane_id(&pane_widths).unwrap_or_default();
    let slots = read_slot_snapshot(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading slot snapshot: {error}"));

    let center_slot = slots
        .iter()
        .find(|slot| slot.pane_id == center_pane)
        .map(|slot| slot.slot_id);

    let border_lines = harness
        .tmux_capture(&[
            "show-window-options",
            "-v",
            "-t",
            &format!("{session}:0"),
            "pane-border-lines",
        ])
        .unwrap_or_default()
        .trim()
        .to_owned();
    let border_status = harness
        .tmux_capture(&[
            "show-window-options",
            "-v",
            "-t",
            &format!("{session}:0"),
            "pane-border-status",
        ])
        .unwrap_or_default()
        .trim()
        .to_owned();
    let border_format = harness
        .tmux_capture(&[
            "show-window-options",
            "-v",
            "-t",
            &format!("{session}:0"),
            "pane-border-format",
        ])
        .unwrap_or_default()
        .trim()
        .to_owned();
    let pane_border_style = harness
        .tmux_capture(&[
            "show-window-options",
            "-v",
            "-t",
            &format!("{session}:0"),
            "pane-border-style",
        ])
        .unwrap_or_default()
        .trim()
        .to_owned();

    let mut mode_switches_ok = true;
    let mut labels_connected = true;
    let mut palette_mapping_ok = true;
    let mut shell_text_inheritance_ok = true;
    let mut active_border_tracks_slot_color = true;
    let mut evidence = vec![
        format!("startup_exit_code={}", launch.exit_code),
        format!("session={session}"),
        format!("center_pane={center_pane}"),
        format!("center_slot={center_slot:?}"),
        format!("pane_border_style={pane_border_style}"),
        format!("pane_border_lines={border_lines}"),
        format!("pane_border_status={border_status}"),
        format!("pane_border_format={border_format}"),
    ];

    for (slot_id, color) in SLOT_COLORS {
        let slot = slots
            .iter()
            .find(|slot| slot.slot_id == slot_id)
            .unwrap_or_else(|| panic!("missing slot snapshot for slot {slot_id}"));
        let slot_id_arg = slot_id.to_string();
        let mode_args = [
            "__internal",
            "mode",
            "--session",
            session.as_str(),
            "--slot",
            slot_id_arg.as_str(),
            "--mode",
            "shell",
        ];
        let mode_switch = harness
            .run_ezm(&mode_args, &[], 0)
            .unwrap_or_else(|error| panic!("shell mode switch failed for slot {slot_id}: {error}"));
        let slot_mode = harness
            .tmux_capture(&[
                "show-options",
                "-v",
                "-t",
                &session,
                &format!("@ezm_slot_{slot_id}_mode"),
            ])
            .unwrap_or_default()
            .trim()
            .to_owned();
        if mode_switch.exit_code != 0 || slot_mode != "shell" {
            mode_switches_ok = false;
        }

        let focus_args = [
            "__internal",
            "focus",
            "--session",
            session.as_str(),
            "--slot",
            slot_id_arg.as_str(),
        ];
        let focus_switch = harness
            .run_ezm(&focus_args, &[], 0)
            .unwrap_or_else(|error| panic!("focus switch failed for slot {slot_id}: {error}"));
        let pane_active_border_style = harness
            .tmux_capture(&[
                "show-window-options",
                "-v",
                "-t",
                &format!("{session}:0"),
                "pane-active-border-style",
            ])
            .unwrap_or_default()
            .trim()
            .to_owned();
        let active_border_matches_slot =
            focus_switch.exit_code == 0 && pane_active_border_style.contains(color);
        if !active_border_matches_slot {
            active_border_tracks_slot_color = false;
        }

        let border_label = harness
            .tmux_capture(&[
                "show-options",
                "-p",
                "-v",
                "-t",
                &slot.pane_id,
                "@ezm_border_label",
            ])
            .unwrap_or_default()
            .trim()
            .to_owned();
        let pane_window_style = harness
            .tmux_capture(&[
                "show-options",
                "-p",
                "-v",
                "-t",
                &slot.pane_id,
                "window-style",
            ])
            .unwrap_or_default()
            .trim()
            .to_owned();
        let pane_window_active_style = harness
            .tmux_capture(&[
                "show-options",
                "-p",
                "-v",
                "-t",
                &slot.pane_id,
                "window-active-style",
            ])
            .unwrap_or_default()
            .trim()
            .to_owned();

        let expected_glyph = slot_glyph(slot_id);
        let label_uses_connected_lines = border_label.contains(&format!("─·{expected_glyph} ·─"))
            && border_label.contains("────────────────");
        let label_uses_slot_color = border_label.contains(&format!("#[fg={color},bold]"));
        let pane_inherits_slot_color =
            pane_window_style.contains(color) && pane_window_active_style.contains(color);

        if !label_uses_connected_lines {
            labels_connected = false;
        }
        if !label_uses_slot_color {
            palette_mapping_ok = false;
        }
        if !pane_inherits_slot_color {
            shell_text_inheritance_ok = false;
        }

        evidence.push(format!("slot{slot_id}_pane={}", slot.pane_id));
        evidence.push(format!(
            "slot{slot_id}_mode_switch_exit={}",
            mode_switch.exit_code
        ));
        evidence.push(format!("slot{slot_id}_mode={slot_mode}"));
        evidence.push(format!("slot{slot_id}_border_label={border_label}"));
        evidence.push(format!("slot{slot_id}_window_style={pane_window_style}"));
        evidence.push(format!(
            "slot{slot_id}_window_active_style={pane_window_active_style}"
        ));
        evidence.push(format!(
            "slot{slot_id}_label_uses_connected_lines={label_uses_connected_lines}"
        ));
        evidence.push(format!(
            "slot{slot_id}_label_uses_slot_color={label_uses_slot_color}"
        ));
        evidence.push(format!(
            "slot{slot_id}_pane_inherits_slot_color={pane_inherits_slot_color}"
        ));
        evidence.push(format!(
            "slot{slot_id}_focus_switch_exit={}",
            focus_switch.exit_code
        ));
        evidence.push(format!(
            "slot{slot_id}_pane_active_border_style={pane_active_border_style}"
        ));
        evidence.push(format!(
            "slot{slot_id}_active_border_matches_slot={active_border_matches_slot}"
        ));
    }

    let center_slot_is_blue = center_slot == Some(1) && pane_border_style.contains("#5ac8e0");
    let border_contract_ok = border_lines == "single"
        && border_status == "top"
        && border_format == BORDER_FORMAT_EXPECTED;

    evidence.push(format!("center_slot_is_blue={center_slot_is_blue}"));
    evidence.push(format!("border_contract_ok={border_contract_ok}"));
    evidence.push(format!("labels_connected={labels_connected}"));
    evidence.push(format!("palette_mapping_ok={palette_mapping_ok}"));
    evidence.push(format!(
        "shell_text_inheritance_ok={shell_text_inheritance_ok}"
    ));
    evidence.push(format!(
        "active_border_tracks_slot_color={active_border_tracks_slot_color}"
    ));
    evidence.push(format!("mode_switches_ok={mode_switches_ok}"));
    write_green_cluster_evidence(&harness, "style-parity", &evidence)
        .unwrap_or_else(|error| panic!("failed writing T-1.3 style evidence: {error}"));

    let pass = launch.exit_code == 0
        && !session.is_empty()
        && center_slot_is_blue
        && border_contract_ok
        && labels_connected
        && palette_mapping_ok
        && shell_text_inheritance_ok
        && active_border_tracks_slot_color
        && mode_switches_ok;

    assert!(
        pass,
        "T-1.3 style parity restoration failed:\n{}",
        evidence.join("\n")
    );
}

fn slot_glyph(slot_id: u8) -> &'static str {
    match slot_id {
        1 => "①",
        2 => "②",
        3 => "③",
        4 => "④",
        5 => "⑤",
        _ => "?",
    }
}

fn write_green_cluster_evidence(
    harness: &FoundationHarness,
    cluster: &str,
    evidence: &[String],
) -> Result<(), String> {
    let dir = harness.artifact_dir.join("triage-green");
    fs::create_dir_all(&dir)
        .map_err(|error| format!("failed creating triage-green evidence directory: {error}"))?;
    fs::write(dir.join(format!("{cluster}.txt")), evidence.join("\n"))
        .map_err(|error| format!("failed writing triage-green evidence file: {error}"))
}
