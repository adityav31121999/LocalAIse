//! Calendar tool: list upcoming events from the personal space.

use std::path::Path;

use crate::tools::ToolResult;

/// List upcoming calendar events from the personal space.
pub fn list_events_tool(personal_space: &Path) -> ToolResult {
    match crate::personal::calendar::list_events(personal_space) {
        Ok(events) => {
            if events.is_empty() {
                return ToolResult::ok("list_events", "No calendar events found.");
            }

            let mut output = format!("{} calendar events:\n\n", events.len());
            for event in events.iter().take(20) {
                output.push_str(&format!(
                    "- **{}** — {}\n",
                    event.title, event.start_at
                ));
                if let Some(loc) = &event.location {
                    if !loc.is_empty() {
                        output.push_str(&format!("  Location: {loc}\n"));
                    }
                }
                if let Some(desc) = &event.description {
                    if !desc.is_empty() {
                        let short: String = desc.chars().take(100).collect();
                        output.push_str(&format!("  {short}\n"));
                    }
                }
            }

            ToolResult::ok("list_events", output)
        }
        Err(e) => ToolResult::err("list_events", format!("Could not read calendar: {e}")),
    }
}
