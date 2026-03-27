use std::collections::{BTreeSet, HashMap};

use super::command::{tmux_output, tmux_output_value, tmux_primary_window_target, tmux_run};
use super::options::{required_session_option, set_pane_option, set_session_option};
use super::slot_swap::validate_canonical_slot_registry;
use super::style::apply_runtime_style_defaults;
use super::SessionError;
use super::CANONICAL_SLOT_IDS;
use super::{canonical_five_pane_column_widths, DEFAULT_CENTER_WIDTH_PCT};
use crate::config::{self, OperatingSystem, ProcessEnv};
use crate::session::RemoteModeContext;
use crate::session::SessionDamageAnalysis;
use crate::session::SessionRepairOutcome;
use crate::session::SharedServerAttachConfig;
use crate::session::SlotMode;

#[derive(Debug, Clone)]
struct SlotMetadata {
    pane_id: String,
    worktree: String,
    cwd: String,
    mode: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SplitDirection {
    Horizontal,
    Vertical,
}

impl SplitDirection {
    const fn flag(self) -> &'static str {
        match self {
            Self::Horizontal => "-h",
            Self::Vertical => "-v",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RecreatePlan {
    target_slot: u8,
    direction: SplitDirection,
    place_before: bool,
}

#[derive(Debug, Clone)]
struct RepairLaunchContext {
    remote_path: Option<String>,
    remote_server_url: Option<String>,
    shared_server: Option<SharedServerAttachConfig>,
    agent_command: Option<String>,
    opencode_themes: config::OpencodeThemeRuntimeResolution,
}

pub(super) fn analyze_session_damage(
    session_name: &str,
) -> Result<SessionDamageAnalysis, SessionError> {
    let slot_metadata = load_slot_metadata(session_name)?;
    let live_panes = list_live_window_panes(session_name)?;
    let slot_to_pane = slot_metadata
        .iter()
        .map(|(&slot_id, metadata)| (slot_id, metadata.pane_id.clone()))
        .collect::<HashMap<_, _>>();

    super::super::repair::analyze_slot_damage(&slot_to_pane, &live_panes)
}

pub(super) fn reconcile_session_damage(
    session_name: &str,
) -> Result<SessionRepairOutcome, SessionError> {
    let launch_context = resolve_repair_launch_context();
    let mut slot_metadata = load_slot_metadata(session_name)?;
    let live_panes = list_live_window_panes(session_name)?;
    recover_stale_slot_pane_bindings(session_name, &mut slot_metadata, &live_panes)?;

    let outcome = reconcile_loaded_session_damage(
        session_name,
        slot_metadata,
        &live_panes,
        recreate_missing_slot,
        persist_slot_metadata,
        validate_canonical_slot_registry,
    )?;

    if outcome.recreated_slots.is_empty() {
        return Ok(outcome);
    }

    restore_canonical_column_widths(session_name)?;
    apply_runtime_style_defaults(session_name)?;
    restore_recreated_slot_modes(session_name, &outcome.recreated_slots, &launch_context)?;

    Ok(outcome)
}

fn recover_stale_slot_pane_bindings(
    session_name: &str,
    slot_metadata: &mut HashMap<u8, SlotMetadata>,
    live_panes: &BTreeSet<String>,
) -> Result<(), SessionError> {
    let live_bindings = discover_live_slot_bindings(live_panes)?;
    let recovered_slots =
        apply_recovered_slot_pane_bindings(slot_metadata, live_panes, &live_bindings);

    for slot_id in recovered_slots {
        let key = format!("@ezm_slot_{slot_id}_pane");
        let Some(metadata) = slot_metadata.get(&slot_id) else {
            continue;
        };
        set_session_option(session_name, &key, &metadata.pane_id)?;
    }

    Ok(())
}

fn discover_live_slot_bindings(
    live_panes: &BTreeSet<String>,
) -> Result<HashMap<u8, String>, SessionError> {
    let mut bindings = HashMap::new();
    for pane_id in live_panes {
        let output = tmux_output(&[
            "show-options",
            "-p",
            "-q",
            "-v",
            "-t",
            pane_id,
            "@ezm_slot_id",
        ])?;
        if !output.status.success() {
            continue;
        }

        let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let Some(slot_id) = parse_live_slot_binding(&value) else {
            continue;
        };
        bindings.entry(slot_id).or_insert_with(|| pane_id.clone());
    }

    Ok(bindings)
}

fn parse_live_slot_binding(value: &str) -> Option<u8> {
    let slot_id = value.trim().parse::<u8>().ok()?;
    if CANONICAL_SLOT_IDS.contains(&slot_id) {
        Some(slot_id)
    } else {
        None
    }
}

fn apply_recovered_slot_pane_bindings(
    slot_metadata: &mut HashMap<u8, SlotMetadata>,
    live_panes: &BTreeSet<String>,
    live_bindings: &HashMap<u8, String>,
) -> Vec<u8> {
    let mut recovered_slots = Vec::new();
    for (&slot_id, live_pane_id) in live_bindings {
        let Some(metadata) = slot_metadata.get_mut(&slot_id) else {
            continue;
        };
        if metadata.pane_id == *live_pane_id || live_panes.contains(&metadata.pane_id) {
            continue;
        }
        metadata.pane_id = live_pane_id.clone();
        recovered_slots.push(slot_id);
    }
    recovered_slots.sort_unstable();
    recovered_slots
}

fn reconcile_loaded_session_damage(
    session_name: &str,
    mut slot_metadata: HashMap<u8, SlotMetadata>,
    live_panes: &BTreeSet<String>,
    mut recreate_slot: impl FnMut(
        &str,
        u8,
        &HashMap<u8, SlotMetadata>,
        &BTreeSet<u8>,
    ) -> Result<String, SessionError>,
    mut persist_slot: impl FnMut(&str, u8, &SlotMetadata) -> Result<(), SessionError>,
    mut validate_slots: impl FnMut(&str) -> Result<(), SessionError>,
) -> Result<SessionRepairOutcome, SessionError> {
    let slot_to_pane = slot_metadata
        .iter()
        .map(|(&slot_id, metadata)| (slot_id, metadata.pane_id.clone()))
        .collect::<HashMap<_, _>>();

    let analysis = super::super::repair::analyze_slot_damage(&slot_to_pane, live_panes)?;
    if !analysis.has_damage() {
        return Ok(SessionRepairOutcome {
            session_name: session_name.to_owned(),
            healthy_slots: analysis.healthy_slots,
            recreated_slots: Vec::new(),
        });
    }

    let missing_slots = analysis
        .recreate_order
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();

    for slot_id in &analysis.recreate_order {
        let new_pane_id = recreate_slot(session_name, *slot_id, &slot_metadata, &missing_slots)?;
        let metadata =
            slot_metadata
                .get_mut(slot_id)
                .ok_or_else(|| SessionError::TmuxCommandFailed {
                    command: format!("reconcile-session-damage -t {session_name}"),
                    stderr: format!("slot metadata missing while reconciling slot {slot_id}"),
                })?;
        metadata.pane_id = new_pane_id;
        persist_slot(session_name, *slot_id, metadata)?;
    }

    validate_slots(session_name)?;

    Ok(SessionRepairOutcome {
        session_name: session_name.to_owned(),
        healthy_slots: analysis.healthy_slots,
        recreated_slots: analysis.recreate_order,
    })
}

fn load_slot_metadata(session_name: &str) -> Result<HashMap<u8, SlotMetadata>, SessionError> {
    let mut metadata = HashMap::with_capacity(CANONICAL_SLOT_IDS.len());
    for slot_id in CANONICAL_SLOT_IDS {
        let pane_key = format!("@ezm_slot_{slot_id}_pane");
        let worktree_key = format!("@ezm_slot_{slot_id}_worktree");
        let cwd_key = format!("@ezm_slot_{slot_id}_cwd");
        let mode_key = format!("@ezm_slot_{slot_id}_mode");
        let pane_id = required_session_option(session_name, &pane_key)?;
        let worktree = required_session_option(session_name, &worktree_key)?;
        let cwd = required_session_option(session_name, &cwd_key)?;
        let mode = required_session_option(session_name, &mode_key)?;
        let _ = metadata.insert(
            slot_id,
            SlotMetadata {
                pane_id,
                worktree,
                cwd,
                mode,
            },
        );
    }
    Ok(metadata)
}

fn list_live_window_panes(session_name: &str) -> Result<BTreeSet<String>, SessionError> {
    let target = tmux_primary_window_target(session_name)?;
    let output = tmux_output_value(&["list-panes", "-t", &target, "-F", "#{pane_id}"])?;
    Ok(output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

fn recreate_missing_slot(
    session_name: &str,
    slot_id: u8,
    slot_metadata: &HashMap<u8, SlotMetadata>,
    missing_slots: &BTreeSet<u8>,
) -> Result<String, SessionError> {
    let plan = recreate_plan(slot_id, missing_slots)?;
    let mut split_direction = plan.direction;
    let mut place_before = plan.place_before;
    let target_slot = plan.target_slot;
    let mut target_pane_id = slot_metadata
        .get(&target_slot)
        .map(|metadata| metadata.pane_id.clone())
        .ok_or_else(|| SessionError::TmuxCommandFailed {
            command: format!("reconcile-session-damage -t {session_name}"),
            stderr: format!("missing backing pane metadata for slot {target_slot}"),
        })?;

    if slot_id == 3 && target_slot == 1 {
        if let Some(anchor_pane) = discover_right_column_anchor_pane(session_name, &target_pane_id)?
        {
            target_pane_id = anchor_pane;
            split_direction = SplitDirection::Vertical;
            place_before = true;
        }
    }

    let mut args = vec!["split-window", plan.direction.flag()];
    if split_direction != plan.direction {
        args[1] = split_direction.flag();
    }
    if place_before {
        args.push("-b");
    }
    args.extend(["-t", &target_pane_id, "-P", "-F", "#{pane_id}"]);
    let pane_id = tmux_output_value(&args)?;

    Ok(pane_id.trim().to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PaneLeftMetric {
    pane_id: String,
    left: u16,
}

fn discover_right_column_anchor_pane(
    session_name: &str,
    center_pane_id: &str,
) -> Result<Option<String>, SessionError> {
    let target = tmux_primary_window_target(session_name)?;
    let output =
        tmux_output_value(&["list-panes", "-t", &target, "-F", "#{pane_id}|#{pane_left}"])?;
    let metrics = parse_pane_left_metrics(&output);
    Ok(select_right_column_anchor(center_pane_id, &metrics))
}

fn parse_pane_left_metrics(output: &str) -> Vec<PaneLeftMetric> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let (pane_id, left) = line.split_once('|')?;
            let pane_id = pane_id.trim();
            let left = left.trim().parse::<u16>().ok()?;
            if pane_id.is_empty() {
                return None;
            }
            Some(PaneLeftMetric {
                pane_id: pane_id.to_owned(),
                left,
            })
        })
        .collect()
}

fn select_right_column_anchor(center_pane_id: &str, metrics: &[PaneLeftMetric]) -> Option<String> {
    let center_left = metrics
        .iter()
        .find(|metric| metric.pane_id == center_pane_id)
        .map(|metric| metric.left)?;

    metrics
        .iter()
        .filter(|metric| metric.left > center_left)
        .max_by_key(|metric| metric.left)
        .map(|metric| metric.pane_id.clone())
}

fn recreate_plan(slot_id: u8, missing_slots: &BTreeSet<u8>) -> Result<RecreatePlan, SessionError> {
    let plan = match slot_id {
        2 => {
            if missing_slots.contains(&4) {
                RecreatePlan {
                    target_slot: 1,
                    direction: SplitDirection::Horizontal,
                    place_before: true,
                }
            } else {
                RecreatePlan {
                    target_slot: 4,
                    direction: SplitDirection::Vertical,
                    place_before: true,
                }
            }
        }
        3 => {
            if missing_slots.contains(&5) {
                RecreatePlan {
                    target_slot: 1,
                    direction: SplitDirection::Horizontal,
                    place_before: false,
                }
            } else {
                RecreatePlan {
                    target_slot: 5,
                    direction: SplitDirection::Vertical,
                    place_before: true,
                }
            }
        }
        4 => RecreatePlan {
            target_slot: 2,
            direction: SplitDirection::Vertical,
            place_before: false,
        },
        5 => RecreatePlan {
            target_slot: 3,
            direction: SplitDirection::Vertical,
            place_before: false,
        },
        _ => {
            return Err(SessionError::TmuxCommandFailed {
                command: String::from("reconcile-session-damage"),
                stderr: format!("slot {slot_id} is not eligible for selective reconcile"),
            });
        }
    };

    Ok(plan)
}

fn resolve_repair_launch_context() -> RepairLaunchContext {
    let env = ProcessEnv;
    let file_config = config::load_config(&env, OperatingSystem::current())
        .map(|loaded| loaded.values)
        .unwrap_or_default();
    let remote_runtime = config::resolve_remote_runtime(&env, &file_config).ok();
    let remote_path = remote_runtime
        .as_ref()
        .and_then(|runtime| runtime.remote_path.value.clone());
    let remote_server_url = remote_runtime
        .as_ref()
        .and_then(|runtime| runtime.remote_server_url.value.clone());
    let remote_routing_active = remote_path.is_some() && remote_server_url.is_some();
    let shared_server = if remote_routing_active {
        remote_runtime.as_ref().and_then(|runtime| {
            runtime
                .shared_server
                .url
                .value
                .as_ref()
                .map(|url| SharedServerAttachConfig {
                    url: url.clone(),
                    password: runtime.shared_server.password.value.clone(),
                })
        })
    } else {
        None
    };

    RepairLaunchContext {
        remote_path,
        remote_server_url,
        shared_server,
        agent_command: config::resolve_agent_command(&file_config),
        opencode_themes: config::resolve_opencode_theme_runtime(&file_config),
    }
}

fn restore_recreated_slot_modes(
    session_name: &str,
    recreated_slots: &[u8],
    launch_context: &RepairLaunchContext,
) -> Result<(), SessionError> {
    for slot_id in recreated_slots {
        let mode_value =
            required_session_option(session_name, &format!("@ezm_slot_{slot_id}_mode"))?;
        let mode = parse_slot_mode_label(*slot_id, &mode_value);
        let remote_context = RemoteModeContext {
            remote_path: launch_context.remote_path.as_deref(),
            remote_server_url: launch_context.remote_server_url.as_deref(),
        };
        super::mode_runtime::switch_slot_mode_for_repair(
            session_name,
            *slot_id,
            mode,
            remote_context,
            launch_context.shared_server.as_ref(),
            launch_context.agent_command.as_deref(),
            launch_context.opencode_themes.theme_for_slot(*slot_id),
        )?;
    }

    Ok(())
}

fn parse_slot_mode_label(slot_id: u8, value: &str) -> SlotMode {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "agent" | "opencode" | "claude" => SlotMode::Agent,
        "shell" | "sh" | "bash" | "zsh" | "fish" | "ubuntu" => SlotMode::Shell,
        "neovim" | "nvim" => SlotMode::Neovim,
        "lazygit" => SlotMode::Lazygit,
        _ => {
            eprintln!(
                "warning: slot {slot_id} has unknown mode metadata value `{value}`; defaulting to agent"
            );
            SlotMode::Agent
        }
    }
}

fn restore_canonical_column_widths(session_name: &str) -> Result<(), SessionError> {
    let target = tmux_primary_window_target(session_name)?;
    let window_width_raw =
        tmux_output_value(&["display-message", "-p", "-t", &target, "#{window_width}"])?;
    let window_width = window_width_raw.trim().parse::<u16>().map_err(|error| {
        SessionError::TmuxCommandFailed {
            command: format!("display-message -p -t {target} #{{window_width}}"),
            stderr: format!("failed parsing window width: {error}"),
        }
    })?;

    let (left_target, center_target, right_target) =
        canonical_five_pane_column_widths(window_width, DEFAULT_CENTER_WIDTH_PCT);
    let left_pane = required_session_option(session_name, "@ezm_slot_2_pane")?;
    let center_pane = required_session_option(session_name, "@ezm_slot_1_pane")?;
    let right_pane = required_session_option(session_name, "@ezm_slot_3_pane")?;

    tmux_run(&[
        "resize-pane",
        "-t",
        &left_pane,
        "-x",
        &left_target.to_string(),
    ])?;
    tmux_run(&[
        "resize-pane",
        "-t",
        &center_pane,
        "-x",
        &center_target.to_string(),
    ])?;
    tmux_run(&[
        "resize-pane",
        "-t",
        &right_pane,
        "-x",
        &right_target.to_string(),
    ])?;

    Ok(())
}

fn persist_slot_metadata(
    session_name: &str,
    slot_id: u8,
    metadata: &SlotMetadata,
) -> Result<(), SessionError> {
    let slot_pane_key = format!("@ezm_slot_{slot_id}_pane");
    set_session_option(session_name, &slot_pane_key, &metadata.pane_id)?;
    set_pane_option(&metadata.pane_id, "@ezm_slot_id", &slot_id.to_string())?;
    set_pane_option(&metadata.pane_id, "@ezm_slot_worktree", &metadata.worktree)?;
    set_pane_option(&metadata.pane_id, "@ezm_slot_cwd", &metadata.cwd)?;
    set_pane_option(&metadata.pane_id, "@ezm_slot_mode", &metadata.mode)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeSet;

    use super::{
        apply_recovered_slot_pane_bindings, parse_live_slot_binding, parse_pane_left_metrics,
        parse_slot_mode_label, reconcile_loaded_session_damage, recreate_plan,
        select_right_column_anchor, PaneLeftMetric, SlotMetadata, SplitDirection,
    };
    use crate::session::SlotMode;

    fn canonical_slot_metadata() -> std::collections::HashMap<u8, SlotMetadata> {
        std::collections::HashMap::from([
            (
                1_u8,
                SlotMetadata {
                    pane_id: String::from("%1"),
                    worktree: String::from("wt-1"),
                    cwd: String::from("/repo/slot-1"),
                    mode: String::from("agent"),
                },
            ),
            (
                2_u8,
                SlotMetadata {
                    pane_id: String::from("%2"),
                    worktree: String::from("wt-2"),
                    cwd: String::from("/repo/slot-2"),
                    mode: String::from("shell"),
                },
            ),
            (
                3_u8,
                SlotMetadata {
                    pane_id: String::from("%3"),
                    worktree: String::from("wt-3"),
                    cwd: String::from("/repo/slot-3"),
                    mode: String::from("neovim"),
                },
            ),
            (
                4_u8,
                SlotMetadata {
                    pane_id: String::from("%4"),
                    worktree: String::from("wt-4"),
                    cwd: String::from("/repo/slot-4"),
                    mode: String::from("lazygit"),
                },
            ),
            (
                5_u8,
                SlotMetadata {
                    pane_id: String::from("%5"),
                    worktree: String::from("wt-5"),
                    cwd: String::from("/repo/slot-5"),
                    mode: String::from("shell"),
                },
            ),
        ])
    }

    #[test]
    fn selective_reconcile_persists_context_only_for_recreated_slots() {
        let slot_metadata = canonical_slot_metadata();
        let live_panes = BTreeSet::from([
            String::from("%1"),
            String::from("%2"),
            String::from("%3"),
            String::from("%5"),
        ]);
        let persisted = RefCell::new(Vec::<(u8, String, String, String)>::new());
        let validated = RefCell::new(0_u8);

        let outcome = reconcile_loaded_session_damage(
            "ezm-session-ctx",
            slot_metadata,
            &live_panes,
            |_session_name, slot_id, _slot_metadata, missing_slots| {
                assert_eq!(slot_id, 4);
                assert_eq!(missing_slots, &BTreeSet::from([4_u8]));
                Ok(String::from("%44"))
            },
            |_session_name, slot_id, metadata| {
                persisted.borrow_mut().push((
                    slot_id,
                    metadata.worktree.clone(),
                    metadata.cwd.clone(),
                    metadata.mode.clone(),
                ));
                Ok(())
            },
            |_session_name| {
                *validated.borrow_mut() += 1;
                Ok(())
            },
        )
        .expect("selective reconcile should succeed");

        assert_eq!(outcome.healthy_slots, vec![1, 2, 3, 5]);
        assert_eq!(outcome.recreated_slots, vec![4]);
        assert_eq!(
            persisted.into_inner(),
            vec![(
                4,
                String::from("wt-4"),
                String::from("/repo/slot-4"),
                String::from("lazygit"),
            )]
        );
        assert_eq!(validated.into_inner(), 1);
    }

    #[test]
    fn apply_recovered_slot_pane_bindings_updates_dead_slot_pointer_from_live_binding() {
        let mut slot_metadata = canonical_slot_metadata();
        slot_metadata.get_mut(&5).expect("slot 5").pane_id = String::from("%dead");
        let live_panes = BTreeSet::from([
            String::from("%1"),
            String::from("%2"),
            String::from("%3"),
            String::from("%4"),
            String::from("%55"),
        ]);
        let live_bindings = std::collections::HashMap::from([(5_u8, String::from("%55"))]);

        let recovered =
            apply_recovered_slot_pane_bindings(&mut slot_metadata, &live_panes, &live_bindings);

        assert_eq!(recovered, vec![5]);
        assert_eq!(slot_metadata.get(&5).expect("slot 5").pane_id, "%55");
    }

    #[test]
    fn apply_recovered_slot_pane_bindings_preserves_live_session_pointer() {
        let mut slot_metadata = canonical_slot_metadata();
        let live_panes = BTreeSet::from([
            String::from("%1"),
            String::from("%2"),
            String::from("%3"),
            String::from("%4"),
            String::from("%5"),
        ]);
        let live_bindings = std::collections::HashMap::from([(5_u8, String::from("%55"))]);

        let recovered =
            apply_recovered_slot_pane_bindings(&mut slot_metadata, &live_panes, &live_bindings);

        assert!(recovered.is_empty());
        assert_eq!(slot_metadata.get(&5).expect("slot 5").pane_id, "%5");
    }

    #[test]
    fn parse_live_slot_binding_accepts_only_canonical_slot_ids() {
        assert_eq!(parse_live_slot_binding("1"), Some(1));
        assert_eq!(parse_live_slot_binding("5"), Some(5));
        assert_eq!(parse_live_slot_binding("0"), None);
        assert_eq!(parse_live_slot_binding("6"), None);
        assert_eq!(parse_live_slot_binding("not-a-slot"), None);
    }

    #[test]
    fn selective_reconcile_keeps_dependent_healthy_slot_context_untouched() {
        let slot_metadata = canonical_slot_metadata();
        let live_panes = BTreeSet::from([
            String::from("%1"),
            String::from("%2"),
            String::from("%4"),
            String::from("%5"),
        ]);
        let persisted_slot_ids = RefCell::new(Vec::<u8>::new());

        let outcome = reconcile_loaded_session_damage(
            "ezm-session-ctx",
            slot_metadata,
            &live_panes,
            |_session_name, slot_id, _slot_metadata, missing_slots| {
                assert_eq!(slot_id, 3);
                assert_eq!(missing_slots, &BTreeSet::from([3_u8]));
                Ok(String::from("%33"))
            },
            |_session_name, slot_id, _metadata| {
                persisted_slot_ids.borrow_mut().push(slot_id);
                Ok(())
            },
            |_session_name| Ok(()),
        )
        .expect("selective reconcile should succeed");

        assert_eq!(outcome.healthy_slots, vec![1, 2, 4, 5]);
        assert_eq!(outcome.recreated_slots, vec![3]);
        assert_eq!(persisted_slot_ids.into_inner(), vec![3]);
    }

    #[test]
    fn recreate_plan_prefers_existing_sibling_pane_for_top_slot_recovery() {
        let missing = BTreeSet::from([3_u8]);

        let plan = recreate_plan(3, &missing).expect("plan");

        assert_eq!(plan.target_slot, 5);
        assert_eq!(plan.direction, SplitDirection::Vertical);
        assert!(plan.place_before);
    }

    #[test]
    fn recreate_plan_uses_center_slot_when_column_is_fully_missing() {
        let missing = BTreeSet::from([3_u8, 5_u8]);

        let plan = recreate_plan(3, &missing).expect("plan");

        assert_eq!(plan.target_slot, 1);
        assert_eq!(plan.direction, SplitDirection::Horizontal);
        assert!(!plan.place_before);
    }

    #[test]
    fn parse_slot_mode_label_accepts_canonical_mode_values() {
        assert_eq!(parse_slot_mode_label(1, "agent"), SlotMode::Agent);
        assert_eq!(parse_slot_mode_label(2, "shell"), SlotMode::Shell);
        assert_eq!(parse_slot_mode_label(3, "neovim"), SlotMode::Neovim);
        assert_eq!(parse_slot_mode_label(4, "lazygit"), SlotMode::Lazygit);
    }

    #[test]
    fn parse_slot_mode_label_accepts_legacy_shell_and_agent_aliases() {
        assert_eq!(parse_slot_mode_label(2, "bash"), SlotMode::Shell);
        assert_eq!(parse_slot_mode_label(2, "ubuntu"), SlotMode::Shell);
        assert_eq!(parse_slot_mode_label(1, "opencode"), SlotMode::Agent);
        assert_eq!(parse_slot_mode_label(1, "claude"), SlotMode::Agent);
    }

    #[test]
    fn parse_slot_mode_label_defaults_unknown_values_to_agent() {
        assert_eq!(parse_slot_mode_label(3, "unknown"), SlotMode::Agent);
    }

    #[test]
    fn select_right_column_anchor_prefers_farthest_right_pane() {
        let metrics = vec![
            PaneLeftMetric {
                pane_id: String::from("%2"),
                left: 0,
            },
            PaneLeftMetric {
                pane_id: String::from("%1"),
                left: 53,
            },
            PaneLeftMetric {
                pane_id: String::from("%5"),
                left: 115,
            },
        ];

        assert_eq!(
            select_right_column_anchor("%1", &metrics),
            Some(String::from("%5"))
        );
    }

    #[test]
    fn select_right_column_anchor_returns_none_without_right_column_candidate() {
        let metrics = vec![
            PaneLeftMetric {
                pane_id: String::from("%2"),
                left: 0,
            },
            PaneLeftMetric {
                pane_id: String::from("%1"),
                left: 53,
            },
        ];

        assert!(select_right_column_anchor("%1", &metrics).is_none());
    }

    #[test]
    fn parse_pane_left_metrics_discards_malformed_rows() {
        let output = "%2|0\nmalformed\n%1|53\n%bad|not-a-number\n";

        assert_eq!(
            parse_pane_left_metrics(output),
            vec![
                PaneLeftMetric {
                    pane_id: String::from("%2"),
                    left: 0,
                },
                PaneLeftMetric {
                    pane_id: String::from("%1"),
                    left: 53,
                },
            ]
        );
    }
}
