use std::collections::BTreeMap;
use std::path::PathBuf;

use thiserror::Error;

pub const CANONICAL_SLOT_IDS: [u8; 5] = [1, 2, 3, 4, 5];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotBinding {
    pub slot_id: u8,
    pub pane_id: String,
    pub worktree_path: PathBuf,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SlotRegistry {
    slots: BTreeMap<u8, SlotBinding>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SlotRegistryError {
    #[error("slot id {slot_id} is outside canonical range 1..5")]
    InvalidSlotId { slot_id: u8 },
    #[error("slot {slot_id} is already bound and remapping is blocked")]
    RemapBlocked { slot_id: u8 },
    #[error("canonical layout requires exactly 5 panes, got {pane_count}")]
    InvalidPaneCount { pane_count: usize },
    #[error("at least one worktree is required for slot assignment")]
    MissingWorktrees,
    #[error(
        "pane {pane_id} is already bound to slot {existing_slot_id}; cannot also bind slot {conflicting_slot_id}"
    )]
    DuplicatePaneBinding {
        pane_id: String,
        existing_slot_id: u8,
        conflicting_slot_id: u8,
    },
}

impl SlotRegistry {
    /// Binds a canonical slot to a pane/worktree identity.
    ///
    /// # Errors
    /// Returns an error when the slot id is outside canonical range or when
    /// an existing slot binding would be remapped to a different identity.
    pub fn bind(
        &mut self,
        slot_id: u8,
        pane_id: String,
        worktree_path: PathBuf,
    ) -> Result<(), SlotRegistryError> {
        if !CANONICAL_SLOT_IDS.contains(&slot_id) {
            return Err(SlotRegistryError::InvalidSlotId { slot_id });
        }

        for (&existing_slot_id, existing_binding) in &self.slots {
            if existing_slot_id != slot_id && existing_binding.pane_id == pane_id {
                return Err(SlotRegistryError::DuplicatePaneBinding {
                    pane_id,
                    existing_slot_id,
                    conflicting_slot_id: slot_id,
                });
            }
        }

        if let Some(existing) = self.slots.get(&slot_id) {
            if existing.pane_id != pane_id || existing.worktree_path != worktree_path {
                return Err(SlotRegistryError::RemapBlocked { slot_id });
            }
            return Ok(());
        }

        self.slots.insert(
            slot_id,
            SlotBinding {
                slot_id,
                pane_id,
                worktree_path,
            },
        );
        Ok(())
    }

    #[must_use]
    pub fn bindings(&self) -> Vec<SlotBinding> {
        self.slots.values().cloned().collect()
    }
}

/// Deterministically maps available worktrees onto canonical slot ids.
///
/// # Errors
/// Returns an error when no worktrees are provided.
pub fn assign_worktrees_to_slots(
    worktrees: &[PathBuf],
) -> Result<Vec<(u8, PathBuf)>, SlotRegistryError> {
    if worktrees.is_empty() {
        return Err(SlotRegistryError::MissingWorktrees);
    }

    let fallback = worktrees
        .last()
        .cloned()
        .ok_or(SlotRegistryError::MissingWorktrees)?;

    Ok(CANONICAL_SLOT_IDS
        .iter()
        .enumerate()
        .map(|(index, slot_id)| {
            let worktree = worktrees
                .get(index)
                .cloned()
                .unwrap_or_else(|| fallback.clone());
            (*slot_id, worktree)
        })
        .collect())
}

/// Builds a slot registry for the canonical five-pane tmux layout.
///
/// # Errors
/// Returns an error when pane count is not canonical, when no worktrees are
/// available, or when slot binding invariants are violated.
pub fn build_registry_for_canonical_panes(
    pane_ids: &[String],
    worktrees: &[PathBuf],
) -> Result<SlotRegistry, SlotRegistryError> {
    if pane_ids.len() != CANONICAL_SLOT_IDS.len() {
        return Err(SlotRegistryError::InvalidPaneCount {
            pane_count: pane_ids.len(),
        });
    }

    let assignments = assign_worktrees_to_slots(worktrees)?;
    let mut registry = SlotRegistry::default();

    for ((slot_id, worktree), pane_id) in assignments.into_iter().zip(pane_ids.iter()) {
        registry.bind(slot_id, pane_id.clone(), worktree)?;
    }

    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::SlotRegistry;
    use super::SlotRegistryError;
    use super::assign_worktrees_to_slots;
    use super::build_registry_for_canonical_panes;

    #[test]
    fn deterministic_assignment_maps_worktrees_to_slots_1_through_5() {
        let worktrees = vec!["/wt/1", "/wt/2", "/wt/3", "/wt/4", "/wt/5"]
            .into_iter()
            .map(std::path::PathBuf::from)
            .collect::<Vec<_>>();

        let first = assign_worktrees_to_slots(&worktrees).expect("first assignment");
        let second = assign_worktrees_to_slots(&worktrees).expect("second assignment");

        assert_eq!(first, second);
        assert_eq!(first[0].0, 1);
        assert_eq!(first[0].1, std::path::PathBuf::from("/wt/1"));
        assert_eq!(first[1].0, 2);
        assert_eq!(first[1].1, std::path::PathBuf::from("/wt/2"));
        assert_eq!(first[2].0, 3);
        assert_eq!(first[2].1, std::path::PathBuf::from("/wt/3"));
        assert_eq!(first[3].0, 4);
        assert_eq!(first[3].1, std::path::PathBuf::from("/wt/4"));
        assert_eq!(first[4].0, 5);
        assert_eq!(first[4].1, std::path::PathBuf::from("/wt/5"));
    }

    #[test]
    fn deterministic_assignment_reuses_project_worktree_for_underfilled_candidates() {
        let worktrees = vec!["/wt/1", "/wt/2", "/wt/project"]
            .into_iter()
            .map(std::path::PathBuf::from)
            .collect::<Vec<_>>();

        let assigned = assign_worktrees_to_slots(&worktrees).expect("assignment");

        assert_eq!(assigned[0], (1, std::path::PathBuf::from("/wt/1")));
        assert_eq!(assigned[1], (2, std::path::PathBuf::from("/wt/2")));
        assert_eq!(assigned[2], (3, std::path::PathBuf::from("/wt/project")));
        assert_eq!(assigned[3], (4, std::path::PathBuf::from("/wt/project")));
        assert_eq!(assigned[4], (5, std::path::PathBuf::from("/wt/project")));
    }

    #[test]
    fn registry_rejects_slot_remap() {
        let mut registry = SlotRegistry::default();
        registry
            .bind(1, String::from("%10"), std::path::PathBuf::from("/wt/1"))
            .expect("initial bind should succeed");

        let error = registry
            .bind(1, String::from("%11"), std::path::PathBuf::from("/wt/1"))
            .expect_err("remap should fail");

        assert_eq!(error, SlotRegistryError::RemapBlocked { slot_id: 1 });
    }

    #[test]
    fn registry_rejects_duplicate_pane_identity_across_slots() {
        let mut registry = SlotRegistry::default();
        registry
            .bind(1, String::from("%10"), std::path::PathBuf::from("/wt/1"))
            .expect("initial bind should succeed");

        let error = registry
            .bind(2, String::from("%10"), std::path::PathBuf::from("/wt/2"))
            .expect_err("duplicate pane id across slots should fail");

        assert_eq!(
            error,
            SlotRegistryError::DuplicatePaneBinding {
                pane_id: String::from("%10"),
                existing_slot_id: 1,
                conflicting_slot_id: 2,
            }
        );
    }

    #[test]
    fn build_registry_rejects_duplicate_pane_id_input() {
        let pane_ids = vec!["%10", "%10", "%12", "%13", "%14"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>();
        let worktrees = vec!["/wt/1", "/wt/2", "/wt/3", "/wt/4", "/wt/5"]
            .into_iter()
            .map(std::path::PathBuf::from)
            .collect::<Vec<_>>();

        let error = build_registry_for_canonical_panes(&pane_ids, &worktrees)
            .expect_err("duplicate pane ids should violate registry invariants");

        assert_eq!(
            error,
            SlotRegistryError::DuplicatePaneBinding {
                pane_id: String::from("%10"),
                existing_slot_id: 1,
                conflicting_slot_id: 2,
            }
        );
    }
}
