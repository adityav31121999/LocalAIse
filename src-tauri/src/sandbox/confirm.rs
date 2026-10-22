//! Sandbox confirmation: guard destructive agent actions behind a human-approval
//! gate before they are executed.
//!
//! In this release the gate is conservative: any action with a "destructive"
//! flag is logged and the caller must pass `force = true` to proceed.
//! A future release will wire this up to a frontend dialog.

use crate::error::{AppError, AppResult};

/// Category of action the agent wants to perform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionKind {
    /// Read or list — always allowed without confirmation.
    Read,
    /// Create a new file or calendar event.
    Create,
    /// Overwrite or modify an existing resource.
    Modify,
    /// Delete a file, event, or other resource.
    Delete,
    /// External network request.
    Network,
}

impl ActionKind {
    /// Whether this action requires confirmation before proceeding.
    pub fn requires_confirmation(&self) -> bool {
        matches!(self, ActionKind::Delete | ActionKind::Modify)
    }
}

/// Check whether a proposed agent action is allowed to proceed.
///
/// `force` allows the caller to bypass the confirmation gate (e.g. after the
/// user has clicked "Approve" in the UI). When `false` and the action requires
/// confirmation, an error is returned explaining what needs approval.
pub fn check(kind: &ActionKind, description: &str, force: bool) -> AppResult<()> {
    if !kind.requires_confirmation() {
        return Ok(());
    }

    if force {
        return Ok(());
    }

    Err(AppError::msg(format!(
        "This action requires your approval before proceeding: {description}\n\
         Run it again with explicit confirmation to allow it."
    )))
}

/// Log a confirmed action to the caller's activity record.
/// Returns the description back for convenience.
pub fn record_approved(kind: &ActionKind, description: &str) -> String {
    tracing_note(kind, description);
    description.to_string()
}

fn tracing_note(kind: &ActionKind, description: &str) {
    let kind_str = match kind {
        ActionKind::Read => "READ",
        ActionKind::Create => "CREATE",
        ActionKind::Modify => "MODIFY",
        ActionKind::Delete => "DELETE",
        ActionKind::Network => "NETWORK",
    };
    // eprintln so it appears in the Tauri console without a heavy logger dep
    eprintln!("[sandbox] APPROVED {kind_str}: {description}");
}
