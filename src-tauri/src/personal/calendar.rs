//! Calendar: events stored in standard iCalendar (.ics) format inside personal space.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::util::hash;

pub const CALENDAR_FILE: &str = "calendar.ics";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    /// Date or datetime string in ISO format e.g. "2026-10-18" or "2026-10-18T10:00:00"
    pub start_at: String,
    pub end_at: Option<String>,
    pub location: Option<String>,
}

pub fn calendar_path(personal_space: &Path) -> PathBuf {
    personal_space.join(CALENDAR_FILE)
}

/// Reads and parses all events from the calendar.ics file.
pub fn list_events(personal_space: &Path) -> AppResult<Vec<CalendarEvent>> {
    let path = calendar_path(personal_space);
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)?;
    Ok(parse_ics(&content))
}

/// Adds a new event and updates the ICS file.
pub fn create_event(personal_space: &Path, mut event: CalendarEvent) -> AppResult<CalendarEvent> {
    if event.id.is_empty() {
        event.id = hash::unique_id("evt");
    }
    let mut events = list_events(personal_space)?;
    events.push(event.clone());
    save_all_events(personal_space, &events)?;
    Ok(event)
}

/// Updates an existing event or returns an error if not found.
pub fn update_event(personal_space: &Path, event: CalendarEvent) -> AppResult<CalendarEvent> {
    let mut events = list_events(personal_space)?;
    if let Some(pos) = events.iter().position(|e| e.id == event.id) {
        events[pos] = event.clone();
        save_all_events(personal_space, &events)?;
        Ok(event)
    } else {
        Err(AppError::msg(format!("Event {} not found", event.id)))
    }
}

/// Deletes an event by ID.
pub fn delete_event(personal_space: &Path, event_id: &str) -> AppResult<()> {
    let mut events = list_events(personal_space)?;
    events.retain(|e| e.id != event_id);
    save_all_events(personal_space, &events)
}

/// Exports raw ICS content from personal space.
pub fn export_ics(personal_space: &Path) -> AppResult<String> {
    let path = calendar_path(personal_space);
    if path.exists() {
        Ok(fs::read_to_string(&path)?)
    } else {
        let events = Vec::new();
        Ok(generate_ics(&events))
    }
}

/// Imports raw ICS content into personal space, merging events.
pub fn import_ics(personal_space: &Path, ics_content: &str) -> AppResult<usize> {
    let imported = parse_ics(ics_content);
    let count = imported.len();
    let mut events = list_events(personal_space)?;

    for new_evt in imported {
        if let Some(pos) = events.iter().position(|e| e.id == new_evt.id) {
            events[pos] = new_evt;
        } else {
            events.push(new_evt);
        }
    }

    save_all_events(personal_space, &events)?;
    Ok(count)
}

fn save_all_events(personal_space: &Path, events: &[CalendarEvent]) -> AppResult<()> {
    fs::create_dir_all(personal_space)?;
    let ics = generate_ics(events);
    fs::write(calendar_path(personal_space), ics)?;
    Ok(())
}

fn generate_ics(events: &[CalendarEvent]) -> String {
    let mut out = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//localAIse//Personal Calendar//EN\r\n");
    for e in events {
        out.push_str("BEGIN:VEVENT\r\n");
        out.push_str(&format!("UID:{}\r\n", e.id));
        out.push_str(&format!("SUMMARY:{}\r\n", e.title));
        if let Some(ref desc) = e.description {
            out.push_str(&format!("DESCRIPTION:{}\r\n", desc.replace('\n', "\\n")));
        }
        let clean_start = e.start_at.replace(['-', ':', 'T'], "").trim().to_string();
        out.push_str(&format!("DTSTART:{}\r\n", clean_start));
        if let Some(ref end) = e.end_at {
            let clean_end = end.replace(['-', ':', 'T'], "").trim().to_string();
            out.push_str(&format!("DTEND:{}\r\n", clean_end));
        }
        if let Some(ref loc) = e.location {
            out.push_str(&format!("LOCATION:{}\r\n", loc));
        }
        out.push_str("END:VEVENT\r\n");
    }
    out.push_str("END:VCALENDAR\r\n");
    out
}

fn parse_ics(content: &str) -> Vec<CalendarEvent> {
    let mut events = Vec::new();
    let mut in_vevent = false;
    let mut uid = String::new();
    let mut title = String::new();
    let mut desc: Option<String> = None;
    let mut start_at = String::new();
    let mut end_at: Option<String> = None;
    let mut location: Option<String> = None;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line == "BEGIN:VEVENT" {
            in_vevent = true;
            uid.clear();
            title.clear();
            desc = None;
            start_at.clear();
            end_at = None;
            location = None;
        } else if line == "END:VEVENT" && in_vevent {
            in_vevent = false;
            if !title.is_empty() {
                if uid.is_empty() {
                    uid = hash::unique_id("evt");
                }
                events.push(CalendarEvent {
                    id: uid.clone(),
                    title: title.clone(),
                    description: desc.clone(),
                    start_at: if start_at.is_empty() { "2026-10-18".to_string() } else { start_at.clone() },
                    end_at: end_at.clone(),
                    location: location.clone(),
                });
            }
        } else if in_vevent {
            if let Some(val) = line.strip_prefix("UID:") {
                uid = val.trim().to_string();
            } else if let Some(val) = line.strip_prefix("SUMMARY:") {
                title = val.trim().to_string();
            } else if let Some(val) = line.strip_prefix("DESCRIPTION:") {
                desc = Some(val.trim().replace("\\n", "\n"));
            } else if let Some(val) = line.strip_prefix("DTSTART:") {
                start_at = val.trim().to_string();
            } else if let Some(val) = line.strip_prefix("DTEND:") {
                end_at = Some(val.trim().to_string());
            } else if let Some(val) = line.strip_prefix("LOCATION:") {
                location = Some(val.trim().to_string());
            }
        }
    }

    events
}
