use std::collections::{BTreeSet, HashMap};

use super::super::SessionError;
use super::super::canonical_window::canonical_window_anchor_pane;
use super::super::command::{tmux_output, tmux_output_value};
use super::super::layout::LAYOUT_MODE_ONE_PANE;
use super::geometry::discover_right_column_anchor_pane;
use super::metadata::SlotMetadata;
use crate::session::SessionRepairOutcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SplitDirection {
    Horizontal,
    Vertical,
}

impl SplitDirection {
    pub(super) const fn flag(self) -> &'static str {
        match self {
            Self::Horizontal => "-h",
            Self::Vertical => "-v",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RecreatePlan {
    pub(super) target_slot: u8,
    pub(super) direction: SplitDirection,
    pub(super) place_before: bool,
}

type RecreateSlot<'a> = Box<
    dyn FnMut(&str, u8, &HashMap<u8, SlotMetadata>, &BTreeSet<u8>) -> Result<String, SessionError>
        + 'a,
>;
type PersistSlot<'a> = Box<dyn FnMut(&str, u8, &SlotMetadata) -> Result<(), SessionError> + 'a>;
type ValidateSlots<'a> = Box<dyn FnMut(&str) -> Result<(), SessionError> + 'a>;

pub(super) struct ReconcileOperations<'a> {
    pub(super) recreate_slot: RecreateSlot<'a>,
    pub(super) persist_slot: PersistSlot<'a>,
    pub(super) validate_slots: ValidateSlots<'a>,
}

pub(super) fn reconcile_loaded_session_damage(
    session_name: &str,
    slot_metadata: HashMap<u8, SlotMetadata>,
    live_panes: &BTreeSet<String>,
    operations: ReconcileOperations<'_>,
) -> Result<SessionRepairOutcome, SessionError> {
    reconcile_loaded_session_damage_with_suspension(
        session_name,
        slot_metadata,
        live_panes,
        &BTreeSet::new(),
        &BTreeSet::new(),
        operations,
    )
}

pub(super) fn reconcile_loaded_session_damage_with_suspension(
    session_name: &str,
    slot_metadata: HashMap<u8, SlotMetadata>,
    live_panes: &BTreeSet<String>,
    suspended_slots: &BTreeSet<u8>,
    restore_suspended_slots: &BTreeSet<u8>,
    operations: ReconcileOperations<'_>,
) -> Result<SessionRepairOutcome, SessionError> {
    reconcile_loaded_session_damage_with_policy(
        session_name,
        slot_metadata,
        live_panes,
        suspended_slots,
        restore_suspended_slots,
        None,
        operations,
    )
}

pub(super) fn reconcile_loaded_session_damage_with_policy(
    session_name: &str,
    mut slot_metadata: HashMap<u8, SlotMetadata>,
    live_panes: &BTreeSet<String>,
    suspended_slots: &BTreeSet<u8>,
    restore_suspended_slots: &BTreeSet<u8>,
    required_slots: Option<&BTreeSet<u8>>,
    mut operations: ReconcileOperations<'_>,
) -> Result<SessionRepairOutcome, SessionError> {
    let slot_to_pane = slot_metadata
        .iter()
        .map(|(&slot_id, metadata)| (slot_id, metadata.pane_id.clone()))
        .collect::<HashMap<_, _>>();

    let analysis = crate::session::repair::analyze_slot_damage_for_slots(
        &slot_to_pane,
        live_panes,
        suspended_slots,
        restore_suspended_slots,
        required_slots,
    )?;
    if !analysis.has_damage() {
        // A healthy pane graph is not sufficient to declare repair complete:
        // pane-local mode metadata is part of the canonical slot invariant.
        // Run validation on the no-op path as well so a session/pane mode
        // disagreement is reported instead of being silently preserved.
        (operations.validate_slots)(session_name)?;
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
        let new_pane_id =
            (operations.recreate_slot)(session_name, *slot_id, &slot_metadata, &missing_slots)?;
        let metadata =
            slot_metadata
                .get_mut(slot_id)
                .ok_or_else(|| SessionError::TmuxCommandFailed {
                    command: format!("reconcile-session-damage -t {session_name}"),
                    stderr: format!("slot metadata missing while reconciling slot {slot_id}"),
                })?;
        metadata.pane_id = new_pane_id;
        (operations.persist_slot)(session_name, *slot_id, metadata)?;
    }

    (operations.validate_slots)(session_name)?;

    Ok(SessionRepairOutcome {
        session_name: session_name.to_owned(),
        healthy_slots: analysis.healthy_slots,
        recreated_slots: analysis.recreate_order,
    })
}

pub(super) fn recreate_missing_slot(
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

    // In one-pane mode the active slot is also the window's only pane.  Once
    // it is killed tmux destroys the window, so recovery must use the newly
    // persisted canonical workspace anchor rather than trying to split a
    // surviving auxiliary window.
    if slot_id == 1 && is_one_pane_layout(session_name)? {
        return canonical_window_anchor_pane(session_name);
    }

    if !pane_is_live(&target_pane_id)? {
        target_pane_id = match fallback_target_pane(session_name, slot_id, slot_metadata) {
            Ok(pane_id) => pane_id,
            Err(_error) if slot_id == 1 => canonical_window_anchor_pane(session_name)?,
            Err(error) => return Err(error),
        };
        split_direction = fallback_split_direction(slot_id);
        place_before = matches!(slot_id, 2);
    }

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

fn is_one_pane_layout(session_name: &str) -> Result<bool, SessionError> {
    Ok(super::super::options::show_session_option(
        session_name,
        super::super::layout::LAYOUT_MODE_KEY,
    )?
    .is_some_and(|mode| mode == LAYOUT_MODE_ONE_PANE))
}

fn fallback_target_pane(
    session_name: &str,
    slot_id: u8,
    slot_metadata: &HashMap<u8, SlotMetadata>,
) -> Result<String, SessionError> {
    let candidates: &[u8] = match slot_id {
        1 => &[2, 3, 4, 5],
        2 => &[1, 3],
        3 => &[1, 2],
        4 => &[2, 1, 3],
        5 => &[3, 1, 2],
        _ => &[],
    };
    for candidate in candidates {
        let Some(pane_id) = slot_metadata
            .get(candidate)
            .map(|metadata| &metadata.pane_id)
        else {
            continue;
        };
        if pane_is_live(pane_id)? {
            return Ok(pane_id.clone());
        }
    }

    Err(SessionError::TmuxCommandFailed {
        command: format!("reconcile-session-damage -t {session_name}"),
        stderr: format!("no live backing pane available for slot {slot_id}"),
    })
}

fn fallback_split_direction(slot_id: u8) -> SplitDirection {
    match slot_id {
        2 | 3 => SplitDirection::Horizontal,
        _ => SplitDirection::Vertical,
    }
}

fn pane_is_live(pane_id: &str) -> Result<bool, SessionError> {
    let output = tmux_output(&["display-message", "-p", "-t", pane_id, "#{pane_id}"])?;
    if output.status.success() {
        return Ok(true);
    }
    if output.status.code() == Some(1) {
        return Ok(false);
    }
    Err(SessionError::TmuxCommandFailed {
        command: format!("display-message -p -t {pane_id} #{{pane_id}}"),
        stderr: super::super::command::format_output_diagnostics(&output),
    })
}

pub(super) fn recreate_plan(
    slot_id: u8,
    missing_slots: &BTreeSet<u8>,
) -> Result<RecreatePlan, SessionError> {
    let plan = match slot_id {
        1 => RecreatePlan {
            // Slot 1 is the center/anchor pane.  The concrete healthy target
            // is selected below from the live canonical workspace rather than
            // from the active window or a stale pane id.
            target_slot: 2,
            direction: SplitDirection::Horizontal,
            place_before: false,
        },
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
