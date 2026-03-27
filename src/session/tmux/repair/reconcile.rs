use std::collections::{BTreeSet, HashMap};

use super::super::SessionError;
use super::super::command::tmux_output_value;
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

pub(super) fn reconcile_loaded_session_damage(
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

    let analysis = crate::session::repair::analyze_slot_damage(&slot_to_pane, live_panes)?;
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

pub(super) fn recreate_plan(
    slot_id: u8,
    missing_slots: &BTreeSet<u8>,
) -> Result<RecreatePlan, SessionError> {
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
