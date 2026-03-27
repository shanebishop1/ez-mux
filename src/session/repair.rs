use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use super::CANONICAL_SLOT_IDS;
use super::SessionError;
use super::TmuxClient;
use super::resolve_session_identity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDamageAnalysis {
    pub healthy_slots: Vec<u8>,
    pub missing_visible_slots: Vec<u8>,
    pub missing_backing_slots: Vec<u8>,
    pub recreate_order: Vec<u8>,
}

impl SessionDamageAnalysis {
    #[must_use]
    pub fn has_damage(&self) -> bool {
        !self.missing_visible_slots.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRepairOutcome {
    pub session_name: String,
    pub healthy_slots: Vec<u8>,
    pub recreated_slots: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRepairExecution {
    pub session_name: String,
    pub healthy_slots: Vec<u8>,
    pub missing_visible_slots: Vec<u8>,
    pub missing_backing_slots: Vec<u8>,
    pub recreate_order: Vec<u8>,
    pub recreated_slots: Vec<u8>,
}

impl SessionRepairExecution {
    #[must_use]
    pub fn action_label(&self) -> &'static str {
        if self.recreated_slots.is_empty() {
            "noop"
        } else {
            "reconcile"
        }
    }
}

/// Analyzes canonical slot metadata against live panes in tmux.
///
/// # Errors
/// Returns an error when tmux metadata cannot be read.
pub fn analyze_session_damage(
    session_name: &str,
    tmux: &impl TmuxClient,
) -> Result<SessionDamageAnalysis, SessionError> {
    tmux.analyze_session_damage(session_name)
}

/// Recreates only missing canonical panes for one session.
///
/// # Errors
/// Returns an error when selective reconcile cannot safely proceed.
pub fn reconcile_session_damage(
    session_name: &str,
    tmux: &impl TmuxClient,
) -> Result<SessionRepairOutcome, SessionError> {
    tmux.reconcile_session_damage(session_name)
}

/// Repairs the current project's tmux session when damage is detected.
///
/// # Errors
/// Returns an error when project/session resolution fails or tmux reconcile
/// cannot safely complete.
pub fn repair_current_project_session(
    tmux: &impl TmuxClient,
) -> Result<SessionRepairExecution, SessionError> {
    let project_dir = std::env::current_dir().map_err(SessionError::CurrentDir)?;
    repair_project_session(&project_dir, tmux)
}

/// Repairs the current project's tmux session and re-attaches when interactive.
///
/// # Errors
/// Returns an error when project/session resolution fails, tmux reconcile cannot
/// safely complete, or interactive attach fails.
pub fn repair_current_project_session_and_attach(
    tmux: &impl TmuxClient,
) -> Result<SessionRepairExecution, SessionError> {
    let project_dir = std::env::current_dir().map_err(SessionError::CurrentDir)?;
    repair_project_session_and_attach(&project_dir, tmux)
}

/// Repairs one resolved project session when damage is detected.
///
/// # Errors
/// Returns an error when session resolution fails or tmux reconcile cannot
/// safely complete.
pub fn repair_project_session(
    project_dir: &Path,
    tmux: &impl TmuxClient,
) -> Result<SessionRepairExecution, SessionError> {
    let session_name = resolve_session_identity(project_dir)?.session_name;
    let analysis = analyze_session_damage(&session_name, tmux)?;

    let recreated_slots = if analysis.has_damage() {
        reconcile_session_damage(&session_name, tmux)?.recreated_slots
    } else {
        Vec::new()
    };

    Ok(SessionRepairExecution {
        session_name,
        healthy_slots: analysis.healthy_slots,
        missing_visible_slots: analysis.missing_visible_slots,
        missing_backing_slots: analysis.missing_backing_slots,
        recreate_order: analysis.recreate_order,
        recreated_slots,
    })
}

/// Repairs one resolved project session and re-attaches when interactive.
///
/// # Errors
/// Returns an error when session resolution fails, reconcile cannot safely
/// complete, or interactive attach fails.
pub fn repair_project_session_and_attach(
    project_dir: &Path,
    tmux: &impl TmuxClient,
) -> Result<SessionRepairExecution, SessionError> {
    let execution = repair_project_session(project_dir, tmux)?;
    tmux.attach_session(&execution.session_name)?;
    Ok(execution)
}

pub(crate) fn analyze_slot_damage(
    slot_to_pane: &HashMap<u8, String>,
    live_panes: &BTreeSet<String>,
) -> Result<SessionDamageAnalysis, SessionError> {
    let mut healthy_slots = Vec::new();
    let mut missing_visible_slots = Vec::new();

    for slot_id in CANONICAL_SLOT_IDS {
        let pane_id =
            slot_to_pane
                .get(&slot_id)
                .ok_or_else(|| SessionError::TmuxCommandFailed {
                    command: String::from("analyze-session-damage"),
                    stderr: format!("missing required session slot pane option for slot {slot_id}"),
                })?;
        if live_panes.contains(pane_id) {
            healthy_slots.push(slot_id);
        } else {
            missing_visible_slots.push(slot_id);
        }
    }

    if missing_visible_slots.is_empty() {
        return Ok(SessionDamageAnalysis {
            healthy_slots,
            missing_visible_slots,
            missing_backing_slots: Vec::new(),
            recreate_order: Vec::new(),
        });
    }

    let mut missing_slots_set = BTreeSet::new();
    for slot_id in &missing_visible_slots {
        let _ = missing_slots_set.insert(*slot_id);
    }

    let mut changed = true;
    while changed {
        changed = false;
        let current = missing_slots_set.iter().copied().collect::<Vec<_>>();
        for slot_id in current {
            if let Some(backing_slot) = required_backing_slot(slot_id) {
                let backing_pane = slot_to_pane.get(&backing_slot).ok_or_else(|| {
                    SessionError::TmuxCommandFailed {
                        command: String::from("analyze-session-damage"),
                        stderr: format!(
                            "missing required backing session slot pane option for slot {backing_slot}"
                        ),
                    }
                })?;
                if !live_panes.contains(backing_pane) && missing_slots_set.insert(backing_slot) {
                    changed = true;
                }
            }
        }
    }

    if missing_slots_set.contains(&1) {
        return Err(SessionError::TmuxCommandFailed {
            command: String::from("analyze-session-damage"),
            stderr: String::from(
                "slot 1 pane is missing; selective reconcile is unsafe and requires full reset",
            ),
        });
    }

    let mut missing_backing_slots = missing_slots_set
        .iter()
        .copied()
        .filter(|slot_id| !missing_visible_slots.contains(slot_id))
        .collect::<Vec<_>>();
    missing_backing_slots.sort_unstable();

    let mut recreate_order = Vec::new();
    for slot_id in CANONICAL_SLOT_IDS {
        append_slot_with_backing(slot_id, &missing_slots_set, &mut recreate_order);
    }

    Ok(SessionDamageAnalysis {
        healthy_slots,
        missing_visible_slots,
        missing_backing_slots,
        recreate_order,
    })
}

#[must_use]
pub(crate) fn required_backing_slot(slot_id: u8) -> Option<u8> {
    match slot_id {
        2..=4 => Some(1),
        5 => Some(3),
        _ => None,
    }
}

fn append_slot_with_backing(slot_id: u8, missing: &BTreeSet<u8>, ordered: &mut Vec<u8>) {
    if !missing.contains(&slot_id) || ordered.contains(&slot_id) {
        return;
    }
    if let Some(backing_slot) = required_backing_slot(slot_id) {
        append_slot_with_backing(backing_slot, missing, ordered);
    }
    ordered.push(slot_id);
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};
    use std::path::Path;

    use crate::session::{
        AuxiliaryViewerOutcome, LayoutPreset, PopupShellOutcome, SlotMode, TeardownOutcome,
        TmuxClient,
    };

    use super::{analyze_slot_damage, repair_project_session, repair_project_session_and_attach};

    struct RepairTmuxStub {
        analysis: super::SessionDamageAnalysis,
        outcome: super::SessionRepairOutcome,
        reconcile_calls: std::cell::Cell<u8>,
        attach_calls: std::cell::RefCell<Vec<String>>,
        interrupt_on_attach: bool,
    }

    impl TmuxClient for RepairTmuxStub {
        fn session_exists(&self, _: &str) -> Result<bool, crate::session::SessionError> {
            Ok(true)
        }

        fn create_detached_session(
            &self,
            _: &str,
            _: &Path,
        ) -> Result<(), crate::session::SessionError> {
            Ok(())
        }

        fn attach_session(&self, session_name: &str) -> Result<(), crate::session::SessionError> {
            self.attach_calls.borrow_mut().push(session_name.to_owned());
            if self.interrupt_on_attach {
                return Err(crate::session::SessionError::Interrupted);
            }
            Ok(())
        }

        fn validate_session_invariants(&self, _: &str) -> Result<(), crate::session::SessionError> {
            Ok(())
        }

        fn bootstrap_default_layout(
            &self,
            _: &str,
            _: &Path,
        ) -> Result<(), crate::session::SessionError> {
            Ok(())
        }

        fn swap_slot_with_center(
            &self,
            _: &str,
            _: u8,
        ) -> Result<(), crate::session::SessionError> {
            Ok(())
        }

        fn focus_slot(&self, _: &str, _: u8) -> Result<(), crate::session::SessionError> {
            Ok(())
        }

        fn apply_layout_preset(
            &self,
            _: &str,
            _: LayoutPreset,
        ) -> Result<(), crate::session::SessionError> {
            Ok(())
        }

        fn switch_slot_mode(
            &self,
            _: &str,
            _: u8,
            _: SlotMode,
            _: crate::session::SlotModeLaunchContext<'_>,
        ) -> Result<(), crate::session::SessionError> {
            Ok(())
        }

        fn toggle_popup_shell(
            &self,
            _: &str,
            _: u8,
            _: Option<&str>,
            _: Option<&str>,
            _: Option<&str>,
        ) -> Result<PopupShellOutcome, crate::session::SessionError> {
            unreachable!()
        }

        fn auxiliary_viewer(
            &self,
            _: &str,
            _: bool,
        ) -> Result<AuxiliaryViewerOutcome, crate::session::SessionError> {
            unreachable!()
        }

        fn teardown_session(
            &self,
            _: &str,
        ) -> Result<TeardownOutcome, crate::session::SessionError> {
            unreachable!()
        }

        fn analyze_session_damage(
            &self,
            _: &str,
        ) -> Result<super::SessionDamageAnalysis, crate::session::SessionError> {
            Ok(self.analysis.clone())
        }

        fn reconcile_session_damage(
            &self,
            _: &str,
        ) -> Result<super::SessionRepairOutcome, crate::session::SessionError> {
            self.reconcile_calls.set(self.reconcile_calls.get() + 1);
            Ok(self.outcome.clone())
        }
    }

    fn canonical_slot_to_pane() -> HashMap<u8, String> {
        HashMap::from([
            (1_u8, String::from("%1")),
            (2_u8, String::from("%2")),
            (3_u8, String::from("%3")),
            (4_u8, String::from("%4")),
            (5_u8, String::from("%5")),
        ])
    }

    #[test]
    fn no_damage_returns_healthy_slots_only() {
        let slot_to_pane = canonical_slot_to_pane();
        let live_panes = BTreeSet::from([
            String::from("%1"),
            String::from("%2"),
            String::from("%3"),
            String::from("%4"),
            String::from("%5"),
        ]);

        let analysis = analyze_slot_damage(&slot_to_pane, &live_panes).expect("analysis");

        assert_eq!(analysis.healthy_slots, vec![1, 2, 3, 4, 5]);
        assert_eq!(analysis.missing_visible_slots, Vec::<u8>::new());
        assert_eq!(analysis.missing_backing_slots, Vec::<u8>::new());
        assert_eq!(analysis.recreate_order, Vec::<u8>::new());
    }

    #[test]
    fn missing_slot_five_requires_slot_three_backing_when_both_are_gone() {
        let slot_to_pane = canonical_slot_to_pane();
        let live_panes =
            BTreeSet::from([String::from("%1"), String::from("%2"), String::from("%4")]);

        let analysis = analyze_slot_damage(&slot_to_pane, &live_panes).expect("analysis");

        assert_eq!(analysis.healthy_slots, vec![1, 2, 4]);
        assert_eq!(analysis.missing_visible_slots, vec![3, 5]);
        assert_eq!(analysis.missing_backing_slots, Vec::<u8>::new());
        assert_eq!(analysis.recreate_order, vec![3, 5]);
    }

    #[test]
    fn missing_slot_five_only_recreates_slot_five_when_slot_three_is_healthy() {
        let slot_to_pane = canonical_slot_to_pane();
        let live_panes = BTreeSet::from([
            String::from("%1"),
            String::from("%2"),
            String::from("%3"),
            String::from("%4"),
        ]);

        let analysis = analyze_slot_damage(&slot_to_pane, &live_panes).expect("analysis");

        assert_eq!(analysis.healthy_slots, vec![1, 2, 3, 4]);
        assert_eq!(analysis.missing_visible_slots, vec![5]);
        assert_eq!(analysis.missing_backing_slots, Vec::<u8>::new());
        assert_eq!(analysis.recreate_order, vec![5]);
    }

    #[test]
    fn missing_slot_two_and_four_do_not_mark_each_other_as_backing() {
        let slot_to_pane = canonical_slot_to_pane();
        let live_panes =
            BTreeSet::from([String::from("%1"), String::from("%3"), String::from("%5")]);

        let analysis = analyze_slot_damage(&slot_to_pane, &live_panes).expect("analysis");

        assert_eq!(analysis.healthy_slots, vec![1, 3, 5]);
        assert_eq!(analysis.missing_visible_slots, vec![2, 4]);
        assert_eq!(analysis.missing_backing_slots, Vec::<u8>::new());
        assert_eq!(analysis.recreate_order, vec![2, 4]);
    }

    #[test]
    fn missing_slot_three_keeps_slot_five_in_healthy_context() {
        let slot_to_pane = canonical_slot_to_pane();
        let live_panes = BTreeSet::from([
            String::from("%1"),
            String::from("%2"),
            String::from("%4"),
            String::from("%5"),
        ]);

        let analysis = analyze_slot_damage(&slot_to_pane, &live_panes).expect("analysis");

        assert_eq!(analysis.healthy_slots, vec![1, 2, 4, 5]);
        assert_eq!(analysis.missing_visible_slots, vec![3]);
        assert_eq!(analysis.missing_backing_slots, Vec::<u8>::new());
        assert_eq!(analysis.recreate_order, vec![3]);
    }

    #[test]
    fn root_slot_damage_is_reported_as_non_selective() {
        let slot_to_pane = canonical_slot_to_pane();
        let live_panes = BTreeSet::from([
            String::from("%2"),
            String::from("%3"),
            String::from("%4"),
            String::from("%5"),
        ]);

        let error =
            analyze_slot_damage(&slot_to_pane, &live_panes).expect_err("root slot should fail");
        assert!(
            error
                .to_string()
                .contains("slot 1 pane is missing; selective reconcile is unsafe")
        );
    }

    #[test]
    fn repair_project_session_skips_reconcile_when_no_damage() {
        let temp = tempfile::tempdir().expect("tempdir");
        let project_dir = temp.path().join("project");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        let tmux = RepairTmuxStub {
            analysis: super::SessionDamageAnalysis {
                healthy_slots: vec![1, 2, 3, 4, 5],
                missing_visible_slots: Vec::new(),
                missing_backing_slots: Vec::new(),
                recreate_order: Vec::new(),
            },
            outcome: super::SessionRepairOutcome {
                session_name: String::from("unused"),
                healthy_slots: vec![1, 2, 3, 4, 5],
                recreated_slots: vec![4],
            },
            reconcile_calls: std::cell::Cell::new(0),
            attach_calls: std::cell::RefCell::new(Vec::new()),
            interrupt_on_attach: false,
        };

        let execution = repair_project_session(&project_dir, &tmux).expect("repair execution");

        assert_eq!(execution.action_label(), "noop");
        assert!(execution.recreated_slots.is_empty());
        assert_eq!(tmux.reconcile_calls.get(), 0);
    }

    #[test]
    fn repair_project_session_runs_reconcile_when_damage_exists() {
        let temp = tempfile::tempdir().expect("tempdir");
        let project_dir = temp.path().join("project");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        let tmux = RepairTmuxStub {
            analysis: super::SessionDamageAnalysis {
                healthy_slots: vec![1, 2, 3, 5],
                missing_visible_slots: vec![4],
                missing_backing_slots: Vec::new(),
                recreate_order: vec![4],
            },
            outcome: super::SessionRepairOutcome {
                session_name: String::from("unused"),
                healthy_slots: vec![1, 2, 3, 5],
                recreated_slots: vec![4],
            },
            reconcile_calls: std::cell::Cell::new(0),
            attach_calls: std::cell::RefCell::new(Vec::new()),
            interrupt_on_attach: false,
        };

        let execution = repair_project_session(&project_dir, &tmux).expect("repair execution");

        assert_eq!(execution.action_label(), "reconcile");
        assert_eq!(execution.recreated_slots, vec![4]);
        assert_eq!(tmux.reconcile_calls.get(), 1);
    }

    #[test]
    fn repair_project_session_and_attach_reopens_current_session_after_reconcile() {
        let temp = tempfile::tempdir().expect("tempdir");
        let project_dir = temp.path().join("project");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        let expected_session = crate::session::resolve_session_identity(&project_dir)
            .expect("session identity")
            .session_name;
        let tmux = RepairTmuxStub {
            analysis: super::SessionDamageAnalysis {
                healthy_slots: vec![1, 2, 3, 5],
                missing_visible_slots: vec![4],
                missing_backing_slots: Vec::new(),
                recreate_order: vec![4],
            },
            outcome: super::SessionRepairOutcome {
                session_name: expected_session.clone(),
                healthy_slots: vec![1, 2, 3, 5],
                recreated_slots: vec![4],
            },
            reconcile_calls: std::cell::Cell::new(0),
            attach_calls: std::cell::RefCell::new(Vec::new()),
            interrupt_on_attach: false,
        };

        let execution =
            repair_project_session_and_attach(&project_dir, &tmux).expect("repair execution");

        assert_eq!(execution.session_name, expected_session);
        assert_eq!(tmux.reconcile_calls.get(), 1);
        assert_eq!(
            tmux.attach_calls.borrow().as_slice(),
            &[expected_session.clone()]
        );
    }

    #[test]
    fn repair_project_session_and_attach_reopens_even_when_noop() {
        let temp = tempfile::tempdir().expect("tempdir");
        let project_dir = temp.path().join("project");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        let expected_session = crate::session::resolve_session_identity(&project_dir)
            .expect("session identity")
            .session_name;
        let tmux = RepairTmuxStub {
            analysis: super::SessionDamageAnalysis {
                healthy_slots: vec![1, 2, 3, 4, 5],
                missing_visible_slots: Vec::new(),
                missing_backing_slots: Vec::new(),
                recreate_order: Vec::new(),
            },
            outcome: super::SessionRepairOutcome {
                session_name: expected_session.clone(),
                healthy_slots: vec![1, 2, 3, 4, 5],
                recreated_slots: vec![4],
            },
            reconcile_calls: std::cell::Cell::new(0),
            attach_calls: std::cell::RefCell::new(Vec::new()),
            interrupt_on_attach: false,
        };

        let execution =
            repair_project_session_and_attach(&project_dir, &tmux).expect("repair execution");

        assert_eq!(execution.action_label(), "noop");
        assert_eq!(tmux.reconcile_calls.get(), 0);
        assert_eq!(
            tmux.attach_calls.borrow().as_slice(),
            &[expected_session.clone()]
        );
    }

    #[test]
    fn repair_project_session_and_attach_propagates_interrupts() {
        let temp = tempfile::tempdir().expect("tempdir");
        let project_dir = temp.path().join("project");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        let expected_session = crate::session::resolve_session_identity(&project_dir)
            .expect("session identity")
            .session_name;
        let tmux = RepairTmuxStub {
            analysis: super::SessionDamageAnalysis {
                healthy_slots: vec![1, 2, 3, 5],
                missing_visible_slots: vec![4],
                missing_backing_slots: Vec::new(),
                recreate_order: vec![4],
            },
            outcome: super::SessionRepairOutcome {
                session_name: expected_session.clone(),
                healthy_slots: vec![1, 2, 3, 5],
                recreated_slots: vec![4],
            },
            reconcile_calls: std::cell::Cell::new(0),
            attach_calls: std::cell::RefCell::new(Vec::new()),
            interrupt_on_attach: true,
        };

        let error = repair_project_session_and_attach(&project_dir, &tmux)
            .expect_err("attach interrupt should propagate");

        assert!(matches!(error, crate::session::SessionError::Interrupted));
        assert_eq!(
            tmux.attach_calls.borrow().as_slice(),
            &[expected_session.clone()]
        );
    }
}
