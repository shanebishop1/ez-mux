use serde::Serialize;

use crate::support::foundation_harness::FoundationHarness;

#[derive(Serialize)]
pub(crate) struct SlotSnapshot {
    pub(crate) slot_id: u8,
    pub(crate) pane_id: String,
    pub(crate) worktree: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PaneGraphEntry {
    pub(crate) left: i32,
    pub(crate) top: i32,
    pub(crate) width: i32,
    pub(crate) height: i32,
}

pub(crate) fn read_slot_snapshot(
    harness: &FoundationHarness,
    session_name: &str,
) -> Result<Vec<SlotSnapshot>, String> {
    let mut slots = Vec::new();
    for slot_id in 1_u8..=5 {
        let pane_key = format!("@ezm_slot_{slot_id}_pane");
        let worktree_key = format!("@ezm_slot_{slot_id}_worktree");

        let pane_id = harness
            .tmux_capture(&["show-options", "-v", "-t", session_name, &pane_key])?
            .trim()
            .to_owned();
        let worktree = harness
            .tmux_capture(&["show-options", "-v", "-t", session_name, &worktree_key])?
            .trim()
            .to_owned();

        slots.push(SlotSnapshot {
            slot_id,
            pane_id,
            worktree,
        });
    }
    Ok(slots)
}

pub(crate) fn slot_snapshots_match(left: &[SlotSnapshot], right: &[SlotSnapshot]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    left.iter().zip(right.iter()).all(|(lhs, rhs)| {
        lhs.slot_id == rhs.slot_id && lhs.pane_id == rhs.pane_id && lhs.worktree == rhs.worktree
    })
}

pub(crate) fn slot_worktree_mapping_stable(left: &[SlotSnapshot], right: &[SlotSnapshot]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    left.iter().zip(right.iter()).all(|(lhs, rhs)| {
        lhs.slot_id == rhs.slot_id
            && super::fixtures::paths_equivalent(&lhs.worktree, &rhs.worktree)
    })
}

pub(crate) fn read_pane_graph(
    harness: &FoundationHarness,
    session_name: &str,
) -> Result<Vec<PaneGraphEntry>, String> {
    let raw = harness.tmux_capture(&[
        "list-panes",
        "-t",
        &format!("{session_name}:0"),
        "-F",
        "#{pane_left}|#{pane_top}|#{pane_width}|#{pane_height}",
    ])?;

    let mut graph = Vec::new();
    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let mut parts = line.split('|');
        let left = parts
            .next()
            .ok_or_else(|| format!("missing pane_left in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane_left in `{line}`: {error}"))?;
        let top = parts
            .next()
            .ok_or_else(|| format!("missing pane_top in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane_top in `{line}`: {error}"))?;
        let width = parts
            .next()
            .ok_or_else(|| format!("missing pane_width in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane_width in `{line}`: {error}"))?;
        let height = parts
            .next()
            .ok_or_else(|| format!("missing pane_height in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane_height in `{line}`: {error}"))?;

        graph.push(PaneGraphEntry {
            left,
            top,
            width,
            height,
        });
    }

    graph.sort_by_key(|entry| (entry.left, entry.top, entry.width, entry.height));
    Ok(graph)
}

pub(crate) fn pane_graph_stable(left: &[PaneGraphEntry], right: &[PaneGraphEntry]) -> bool {
    left == right
}

#[cfg(test)]
mod tests {
    use super::{PaneGraphEntry, SlotSnapshot, pane_graph_stable, slot_worktree_mapping_stable};

    #[test]
    fn pane_graph_stability_ignores_runtime_pane_ids() {
        let before = vec![
            PaneGraphEntry {
                left: 0,
                top: 0,
                width: 30,
                height: 20,
            },
            PaneGraphEntry {
                left: 31,
                top: 0,
                width: 40,
                height: 40,
            },
            PaneGraphEntry {
                left: 72,
                top: 0,
                width: 30,
                height: 20,
            },
            PaneGraphEntry {
                left: 0,
                top: 21,
                width: 30,
                height: 19,
            },
            PaneGraphEntry {
                left: 72,
                top: 21,
                width: 30,
                height: 19,
            },
        ];
        let after = before.clone();

        assert!(pane_graph_stable(&before, &after));
    }

    #[test]
    fn slot_worktree_mapping_stability_allows_pane_id_churn() {
        let before = vec![
            SlotSnapshot {
                slot_id: 1,
                pane_id: String::from("%1"),
                worktree: String::from("/tmp/wt-1"),
            },
            SlotSnapshot {
                slot_id: 2,
                pane_id: String::from("%2"),
                worktree: String::from("/tmp/wt-2"),
            },
        ];
        let after = vec![
            SlotSnapshot {
                slot_id: 1,
                pane_id: String::from("%9"),
                worktree: String::from("/tmp/wt-1"),
            },
            SlotSnapshot {
                slot_id: 2,
                pane_id: String::from("%10"),
                worktree: String::from("/tmp/wt-2"),
            },
        ];

        assert!(slot_worktree_mapping_stable(&before, &after));
    }
}
