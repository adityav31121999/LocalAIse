//! Personal space search index across diary entries and calendar events.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::calendar;
use super::diary;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalSearchResult {
    pub kind: String, // "diary" | "event"
    pub id: String,
    pub title: String,
    pub date: Option<String>,
    pub snippet: String,
}

pub fn search_personal_space(personal_space: &Path, query: &str) -> Vec<PersonalSearchResult> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::new();

    // 1. Search Diary
    if let Ok(pages) = diary::list(personal_space) {
        for page in pages {
            let matches_title = page.title.to_lowercase().contains(&needle);
            let matches_body = page.body.to_lowercase().contains(&needle);
            if matches_title || matches_body {
                let snippet = if matches_body {
                    let preview = page.body.lines().next().unwrap_or("").trim();
                    if preview.len() > 100 {
                        format!("{}…", &preview[..100])
                    } else {
                        preview.to_string()
                    }
                } else {
                    page.title.clone()
                };

                results.push(PersonalSearchResult {
                    kind: "diary".to_string(),
                    id: page.id,
                    title: page.title,
                    date: page.date,
                    snippet,
                });
            }
        }
    }

    // 2. Search Calendar Events
    if let Ok(events) = calendar::list_events(personal_space) {
        for event in events {
            let matches_title = event.title.to_lowercase().contains(&needle);
            let matches_desc = event
                .description
                .as_ref()
                .map(|d| d.to_lowercase().contains(&needle))
                .unwrap_or(false);

            if matches_title || matches_desc {
                let snippet = event.description.clone().unwrap_or_else(|| event.title.clone());
                results.push(PersonalSearchResult {
                    kind: "event".to_string(),
                    id: event.id,
                    title: event.title,
                    date: Some(event.start_at),
                    snippet,
                });
            }
        }
    }

    results
}
