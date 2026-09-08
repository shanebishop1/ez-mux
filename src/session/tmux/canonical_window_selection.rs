use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WorkspaceShape {
    active_slots: BTreeSet<u8>,
    suspended_slots: BTreeSet<u8>,
}

impl WorkspaceShape {
    pub(super) fn new(active_slots: BTreeSet<u8>, suspended_slots: BTreeSet<u8>) -> Self {
        Self {
            active_slots,
            suspended_slots,
        }
    }

    pub(super) fn five_pane() -> Self {
        Self::new((1_u8..=5).collect(), BTreeSet::new())
    }

    fn expected_slots(&self) -> BTreeSet<u8> {
        self.active_slots
            .difference(&self.suspended_slots)
            .copied()
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WindowPaneRow {
    window: String,
    pane: String,
    slot: Option<u8>,
}

pub(super) fn select_canonical_window_for_workspace(
    rows: &[WindowPaneRow],
    managed_panes: &BTreeMap<u8, String>,
    workspace: &WorkspaceShape,
) -> Option<String> {
    let mut counts = BTreeMap::<String, usize>::new();
    for window_id in rows
        .iter()
        .map(|row| row.window.as_str())
        .collect::<BTreeSet<_>>()
    {
        let window_rows = rows
            .iter()
            .filter(|row| row.window == window_id)
            .cloned()
            .collect::<Vec<_>>();
        if window_represents_workspace(&window_rows, managed_panes, workspace) {
            let count = window_rows
                .iter()
                .filter(|row| row_matches_persisted_binding(row, managed_panes))
                .count();
            counts.insert(window_id.to_owned(), count);
        }
    }

    counts
        .into_iter()
        .max_by_key(|(window_id, count)| (*count, std::cmp::Reverse(window_id.clone())))
        .map(|(window_id, _)| window_id)
}

pub(super) fn window_represents_workspace(
    rows: &[WindowPaneRow],
    managed_panes: &BTreeMap<u8, String>,
    workspace: &WorkspaceShape,
) -> bool {
    let expected_slots = workspace.expected_slots();
    if expected_slots.is_empty() {
        return false;
    }

    let associated_rows = rows
        .iter()
        .filter(|row| row_matches_persisted_binding(row, managed_panes))
        .collect::<Vec<_>>();
    if associated_rows.is_empty() {
        return false;
    }

    // A tagged pane is only useful evidence when it belongs to the declared
    // workspace. Suspended slots remain valid workspace members while an
    // explicit preset is restoring them, so they are accepted here even
    // though they are not part of the ordinary active-pane minimum. This
    // still rejects an auxiliary/old window containing one unrelated tag.
    let workspace_slots = workspace
        .active_slots
        .union(&workspace.suspended_slots)
        .copied()
        .collect::<BTreeSet<_>>();
    let tagged_slots = associated_rows
        .iter()
        .filter_map(|row| row.slot)
        .collect::<BTreeSet<_>>();
    if tagged_slots
        .iter()
        .any(|slot| !workspace_slots.contains(slot))
    {
        return false;
    }

    let associated_count = associated_rows
        .iter()
        .map(|row| row.pane.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let minimum_coherent_count = expected_slots.len().saturating_sub(1).max(1);

    if expected_slots.len() == 1 {
        return associated_rows.iter().any(|row| {
            row_matches_persisted_binding(row, managed_panes)
                && (row.slot.is_none() || row.slot == Some(1))
        });
    }

    let has_active_workspace_anchor = tagged_slots
        .iter()
        .any(|slot| expected_slots.contains(slot))
        || associated_rows
            .iter()
            .any(|row| row_matches_persisted_binding(row, managed_panes) && row.slot.is_none());

    has_active_workspace_anchor && associated_count >= minimum_coherent_count
}

fn row_matches_persisted_binding(
    row: &WindowPaneRow,
    managed_panes: &BTreeMap<u8, String>,
) -> bool {
    match row.slot {
        Some(slot_id) => managed_panes
            .get(&slot_id)
            .is_some_and(|pane_id| pane_id == &row.pane),
        None => managed_panes.values().any(|pane_id| pane_id == &row.pane),
    }
}

pub(super) fn parse_window_pane_rows(output: &str) -> Vec<WindowPaneRow> {
    parse_window_pane_rows_with_window("", output)
}

pub(super) fn parse_window_pane_rows_with_window(
    window_id: &str,
    output: &str,
) -> Vec<WindowPaneRow> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.trim().split('|');
            let (row_window_id, pane_id, slot_value) = if window_id.is_empty() {
                (parts.next()?.trim(), parts.next()?.trim(), parts.next())
            } else {
                (window_id, parts.next()?.trim(), parts.next())
            };
            if row_window_id.is_empty() || pane_id.is_empty() {
                return None;
            }
            let slot_id = slot_value
                .map(str::trim)
                .and_then(|value| value.parse::<u8>().ok())
                .filter(|slot_id| (1..=5).contains(slot_id));
            Some(WindowPaneRow {
                window: row_window_id.to_owned(),
                pane: pane_id.to_owned(),
                slot: slot_id,
            })
        })
        .collect()
}

pub(super) fn first_window_identity(stdout: &[u8]) -> Option<(&str, &str)> {
    let line = std::str::from_utf8(stdout)
        .ok()?
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    line.split_once('|')
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::{
        WindowPaneRow, WorkspaceShape, parse_window_pane_rows,
        select_canonical_window_for_workspace,
    };

    #[test]
    fn canonical_window_rows_parse_stable_ids_and_valid_slot_bindings() {
        assert_eq!(
            parse_window_pane_rows("@9|%1|1\n@9|%2|6\nmalformed\n"),
            vec![
                WindowPaneRow {
                    window: String::from("@9"),
                    pane: String::from("%1"),
                    slot: Some(1),
                },
                WindowPaneRow {
                    window: String::from("@9"),
                    pane: String::from("%2"),
                    slot: None,
                },
            ]
        );
    }

    #[test]
    fn canonical_window_selection_ignores_auxiliary_and_extra_windows() {
        let rows = vec![
            WindowPaneRow {
                window: String::from("@aux"),
                pane: String::from("%90"),
                slot: None,
            },
            WindowPaneRow {
                window: String::from("@extra"),
                pane: String::from("%91"),
                slot: None,
            },
            WindowPaneRow {
                window: String::from("@managed"),
                pane: String::from("%42"),
                slot: Some(1),
            },
            WindowPaneRow {
                window: String::from("@managed"),
                pane: String::from("%43"),
                slot: Some(2),
            },
            WindowPaneRow {
                window: String::from("@managed"),
                pane: String::from("%44"),
                slot: Some(3),
            },
            WindowPaneRow {
                window: String::from("@managed"),
                pane: String::from("%45"),
                slot: Some(4),
            },
        ];
        let managed = BTreeMap::from([
            (1, String::from("%42")),
            (2, String::from("%43")),
            (3, String::from("%44")),
            (4, String::from("%45")),
        ]);

        assert_eq!(
            select_canonical_window_for_workspace(&rows, &managed, &WorkspaceShape::five_pane()),
            Some(String::from("@managed"))
        );
    }

    #[test]
    fn canonical_window_selection_recovers_from_stale_pane_metadata() {
        let rows = vec![WindowPaneRow {
            window: String::from("@managed"),
            pane: String::from("%new"),
            slot: Some(3),
        }];

        assert_eq!(
            select_canonical_window_for_workspace(
                &rows,
                &BTreeMap::from([(3, String::from("%stale"))]),
                &WorkspaceShape::five_pane(),
            ),
            None
        );
    }

    #[test]
    fn stale_slot_like_panes_do_not_qualify_without_matching_bindings() {
        let rows = (1_u8..=5)
            .map(|slot_id| WindowPaneRow {
                window: String::from("@stale"),
                pane: format!("%stale-{slot_id}"),
                slot: Some(slot_id),
            })
            .collect::<Vec<_>>();

        assert_eq!(
            select_canonical_window_for_workspace(
                &rows,
                &BTreeMap::from([
                    (1, String::from("%dead-1")),
                    (2, String::from("%dead-2")),
                    (3, String::from("%dead-3")),
                    (4, String::from("%dead-4")),
                    (5, String::from("%dead-5")),
                ]),
                &WorkspaceShape::five_pane(),
            ),
            None
        );
    }

    #[test]
    fn canonical_window_selection_requires_a_coherent_workspace_not_one_stray_tag() {
        let rows = vec![
            WindowPaneRow {
                window: String::from("@stale"),
                pane: String::from("%101"),
                slot: Some(4),
            },
            WindowPaneRow {
                window: String::from("@workspace"),
                pane: String::from("%201"),
                slot: Some(1),
            },
            WindowPaneRow {
                window: String::from("@workspace"),
                pane: String::from("%202"),
                slot: Some(2),
            },
            WindowPaneRow {
                window: String::from("@workspace"),
                pane: String::from("%203"),
                slot: Some(3),
            },
            WindowPaneRow {
                window: String::from("@workspace"),
                pane: String::from("%204"),
                slot: Some(4),
            },
        ];

        assert_eq!(
            select_canonical_window_for_workspace(
                &rows,
                &BTreeMap::from([
                    (1, String::from("%201")),
                    (2, String::from("%202")),
                    (3, String::from("%203")),
                    (4, String::from("%204")),
                ]),
                &WorkspaceShape::five_pane(),
            ),
            Some(String::from("@workspace"))
        );
    }

    #[test]
    fn canonical_window_selection_is_independent_of_window_base_index() {
        let rows = (1_u8..=4)
            .map(|slot_id| WindowPaneRow {
                window: String::from("@9"),
                pane: format!("%{slot_id}"),
                slot: Some(slot_id),
            })
            .collect::<Vec<_>>();

        assert_eq!(
            select_canonical_window_for_workspace(
                &rows,
                &rows
                    .iter()
                    .map(|row| (row.slot.expect("test slot"), row.pane.clone()))
                    .collect::<BTreeMap<_, _>>(),
                &WorkspaceShape::five_pane(),
            ),
            Some(String::from("@9"))
        );
    }

    #[test]
    fn one_pane_workspace_requires_the_managed_slot_one_anchor() {
        let workspace = WorkspaceShape::new(BTreeSet::from([1]), BTreeSet::from([2, 3, 4, 5]));
        let rows = vec![WindowPaneRow {
            window: String::from("@aux"),
            pane: String::from("%401"),
            slot: Some(1),
        }];

        assert!(!super::window_represents_workspace(
            &rows,
            &BTreeMap::from([(1, String::from("%stale"))]),
            &workspace,
        ));
        assert!(super::window_represents_workspace(
            &rows,
            &BTreeMap::from([(1, String::from("%401"))]),
            &workspace,
        ));
    }

    #[test]
    fn reduced_workspace_accepts_three_live_slots_with_two_suspended_slots() {
        let workspace = WorkspaceShape::new(BTreeSet::from([1, 2, 3]), BTreeSet::from([4, 5]));
        let rows = (1_u8..=3)
            .map(|slot_id| WindowPaneRow {
                window: String::from("@12"),
                pane: format!("%{slot_id}"),
                slot: Some(slot_id),
            })
            .collect::<Vec<_>>();

        assert!(super::window_represents_workspace(
            &rows,
            &BTreeMap::from([
                (1, String::from("%1")),
                (2, String::from("%2")),
                (3, String::from("%3")),
            ]),
            &workspace,
        ));
    }

    #[test]
    fn explicit_restore_accepts_suspended_slots_in_the_managed_workspace() {
        let workspace = WorkspaceShape::new(BTreeSet::from([1, 2, 3]), BTreeSet::from([4, 5]));
        let rows = (1_u8..=5)
            .map(|slot_id| WindowPaneRow {
                window: String::from("@12"),
                pane: format!("%{slot_id}"),
                slot: Some(slot_id),
            })
            .collect::<Vec<_>>();

        assert!(super::window_represents_workspace(
            &rows,
            &rows
                .iter()
                .map(|row| (row.slot.expect("test slot"), row.pane.clone()))
                .collect::<BTreeMap<_, _>>(),
            &workspace,
        ));
    }
}
