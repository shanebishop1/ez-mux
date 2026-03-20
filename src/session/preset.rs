use clap::ValueEnum;

use super::SessionError;
use super::TmuxClient;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum LayoutPreset {
    ThreePane,
}

impl LayoutPreset {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::ThreePane => "three-pane",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutPresetOutcome {
    pub session_name: String,
    pub preset: LayoutPreset,
}

/// Applies one layout preset to an existing tmux session.
///
/// # Errors
/// Returns an error when tmux cannot apply the requested preset.
pub fn apply_layout_preset(
    session_name: &str,
    preset: LayoutPreset,
    tmux: &impl TmuxClient,
) -> Result<LayoutPresetOutcome, SessionError> {
    tmux.apply_layout_preset(session_name, preset)?;

    Ok(LayoutPresetOutcome {
        session_name: session_name.to_owned(),
        preset,
    })
}
