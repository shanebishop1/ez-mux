use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use crate::session::{
    AuxiliaryViewerOutcome, LayoutPreset, PopupShellOutcome, SlotMode, TeardownOutcome, TmuxClient,
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
        _: u8,
        _: bool,
    ) -> Result<(), crate::session::SessionError> {
        Ok(())
    }

    fn swap_slot_with_center(&self, _: &str, _: u8) -> Result<(), crate::session::SessionError> {
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

    fn teardown_session(&self, _: &str) -> Result<TeardownOutcome, crate::session::SessionError> {
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
    let live_panes = BTreeSet::from([String::from("%1"), String::from("%2"), String::from("%4")]);

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
    let live_panes = BTreeSet::from([String::from("%1"), String::from("%3"), String::from("%5")]);

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

    let error = analyze_slot_damage(&slot_to_pane, &live_panes).expect_err("root slot should fail");
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
        std::slice::from_ref(&expected_session)
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
        std::slice::from_ref(&expected_session)
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
        std::slice::from_ref(&expected_session)
    );
}
