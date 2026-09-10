use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use ez_mux::config::SessionRuntimeContext;
use ez_mux::session::SessionDamageAnalysis;
use ez_mux::session::SessionRepairOutcome;
use ez_mux::session::SlotMode;

pub(crate) struct FakeTmux {
    pub(crate) sessions: RefCell<HashSet<String>>,
    pub(crate) created: RefCell<Vec<(String, PathBuf)>>,
    pub(crate) bootstrapped: RefCell<Vec<(String, PathBuf, u8, bool)>>,
    pub(crate) bootstrap_error: RefCell<Option<String>>,
    pub(crate) attached: RefCell<Vec<String>>,
    pub(crate) attach_error: RefCell<Option<String>>,
    pub(crate) mode_switches: RefCell<Vec<(String, u8, SlotMode)>>,
    pub(crate) mode_switch_error: RefCell<Option<String>>,
    pub(crate) swap_calls: RefCell<Vec<(String, u8)>>,
    pub(crate) swap_error: RefCell<Option<String>>,
    pub(crate) focus_calls: RefCell<Vec<(String, u8)>>,
    pub(crate) focus_error: RefCell<Option<String>>,
    pub(crate) popup_toggles: RefCell<Vec<(String, u8)>>,
    pub(crate) popup_toggle_error: RefCell<Option<String>>,
    pub(crate) popup_toggle_open: RefCell<bool>,
    pub(crate) auxiliary_calls: RefCell<Vec<(String, bool)>>,
    pub(crate) auxiliary_error: RefCell<Option<String>>,
    pub(crate) auxiliary_exists: RefCell<bool>,
    pub(crate) auxiliary_available: RefCell<bool>,
    pub(crate) teardown_calls: RefCell<Vec<String>>,
    pub(crate) teardown_error: RefCell<Option<String>>,
    pub(crate) teardown_project_removed: RefCell<bool>,
    pub(crate) helpers_created_during_bootstrap: RefCell<Vec<String>>,
    pub(crate) damage_analysis_calls: RefCell<Vec<String>>,
    pub(crate) repair_calls: RefCell<Vec<String>>,
    pub(crate) damage_analysis: RefCell<SessionDamageAnalysis>,
    pub(crate) repair_outcome: RefCell<SessionRepairOutcome>,
    pub(crate) skipped_non_interactive_attach: RefCell<u32>,
    pub(crate) interactive_attach: bool,
    pub(crate) runtime_contexts: RefCell<HashMap<String, SessionRuntimeContext>>,
    pub(crate) runtime_passwords: RefCell<HashMap<String, Option<String>>>,
}

impl Default for FakeTmux {
    fn default() -> Self {
        Self {
            sessions: RefCell::new(HashSet::new()),
            created: RefCell::new(Vec::new()),
            bootstrapped: RefCell::new(Vec::new()),
            bootstrap_error: RefCell::new(None),
            attached: RefCell::new(Vec::new()),
            attach_error: RefCell::new(None),
            mode_switches: RefCell::new(Vec::new()),
            mode_switch_error: RefCell::new(None),
            swap_calls: RefCell::new(Vec::new()),
            swap_error: RefCell::new(None),
            focus_calls: RefCell::new(Vec::new()),
            focus_error: RefCell::new(None),
            popup_toggles: RefCell::new(Vec::new()),
            popup_toggle_error: RefCell::new(None),
            popup_toggle_open: RefCell::new(false),
            auxiliary_calls: RefCell::new(Vec::new()),
            auxiliary_error: RefCell::new(None),
            auxiliary_exists: RefCell::new(false),
            auxiliary_available: RefCell::new(true),
            teardown_calls: RefCell::new(Vec::new()),
            teardown_error: RefCell::new(None),
            teardown_project_removed: RefCell::new(false),
            helpers_created_during_bootstrap: RefCell::new(Vec::new()),
            damage_analysis_calls: RefCell::new(Vec::new()),
            repair_calls: RefCell::new(Vec::new()),
            damage_analysis: RefCell::new(SessionDamageAnalysis {
                healthy_slots: vec![1, 2, 3, 4, 5],
                missing_visible_slots: Vec::new(),
                missing_backing_slots: Vec::new(),
                recreate_order: Vec::new(),
            }),
            repair_outcome: RefCell::new(SessionRepairOutcome {
                session_name: String::from("ezm-session-default"),
                healthy_slots: vec![1, 2, 3, 4, 5],
                recreated_slots: Vec::new(),
            }),
            skipped_non_interactive_attach: RefCell::new(0),
            interactive_attach: false,
            runtime_contexts: RefCell::new(HashMap::new()),
            runtime_passwords: RefCell::new(HashMap::new()),
        }
    }
}
