//! The diary: one Markdown file per page inside the personal space.
//!
//! Pages are dated (`2026-10-18.md`) or free-form, and each file carries a small
//! front matter block so the app can restore ids and titles. Everything stays
//! plain Markdown: readable, greppable and editable without this program.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::util::{hash, time};

pub const DIARY_DIR: &str = "diary";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiaryPage {
    pub id: String,
    /// `YYYY-MM-DD` for a dated entry, `None` for a page the user added.
    pub date: Option<String>,
    pub title: String,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
}

pub fn diary_dir(personal_space: &Path) -> PathBuf {
    personal_space.join(DIARY_DIR)
}

/// Free pages first, then dated entries newest to oldest.
pub fn list(personal_space: &Path) -> AppResult<Vec<DiaryPage>> {
    let dir = diary_dir(personal_space);
    let read_dir = match std::fs::read_dir(&dir) {
        Ok(read_dir) => read_dir,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };

    let mut pages: Vec<DiaryPage> = read_dir
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_markdown(path))
        .filter_map(|path| read_page(&path))
        .collect();

    pages.sort_by(|a, b| match (&a.date, &b.date) {
        (None, None) => a.created_at.cmp(&b.created_at),
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(left), Some(right)) => right.cmp(left),
    });

    Ok(pages)
}

pub fn create(personal_space: &Path, date: Option<String>, title: String) -> AppResult<DiaryPage> {
    let now = time::now_iso();
    let page = DiaryPage {
        id: hash::unique_id("page"),
        date: date.filter(|value| is_date_key(value)),
        title,
        body: String::new(),
        created_at: now.clone(),
        updated_at: now,
    };

    write(personal_space, &page)?;
    Ok(page)
}

/// Writes the page back. A dated entry whose date changed is renamed, so one
/// day never ends up with two files.
pub fn save(personal_space: &Path, page: &DiaryPage) -> AppResult<DiaryPage> {
    let mut saved = page.clone();
    saved.updated_at = time::now_iso();

    if let Some(previous) = find_file_by_id(personal_space, &page.id) {
        let target = file_for(personal_space, &saved);
        if previous != target && previous.exists() {
            std::fs::remove_file(&previous)?;
        }
    }

    write(personal_space, &saved)?;
    Ok(saved)
}

pub fn delete(personal_space: &Path, page_id: &str) -> AppResult<()> {
    if let Some(path) = find_file_by_id(personal_space, page_id) {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

fn write(personal_space: &Path, page: &DiaryPage) -> AppResult<()> {
    let dir = diary_dir(personal_space);
    std::fs::create_dir_all(&dir)?;

    let mut out = String::from("---\n");
    out.push_str(&format!("id: {}\n", page.id));
    if let Some(date) = &page.date {
        out.push_str(&format!("date: {date}\n"));
    }
    out.push_str(&format!("title: {}\n", single_line(&page.title)));
    out.push_str(&format!("createdAt: {}\n", page.created_at));
    out.push_str(&format!("updatedAt: {}\n", page.updated_at));
    out.push_str("---\n\n");
    out.push_str(&page.body);
    if !out.ends_with('\n') {
        out.push('\n');
    }

    std::fs::write(file_for(personal_space, page), out)?;
    Ok(())
}

/// Dated pages use the date as their file name so a day is a single file.
fn file_for(personal_space: &Path, page: &DiaryPage) -> PathBuf {
    let dir = diary_dir(personal_space);
    match &page.date {
        Some(date) if is_date_key(date) => dir.join(format!("{date}.md")),
        _ => {
            let title = if page.title.trim().is_empty() {
                "page"
            } else {
                page.title.as_str()
            };
            dir.join(format!("{}-{}.md", hash::slug(title), id_suffix(&page.id)))
        }
    }
}

fn id_suffix(id: &str) -> String {
    let tail: String = id
        .rsplit('-')
        .next()
        .unwrap_or(id)
        .chars()
        .take(8)
        .collect();
    if tail.is_empty() {
        hash::short_id(id)[..8].to_string()
    } else {
        tail
    }
}

fn find_file_by_id(personal_space: &Path, page_id: &str) -> Option<PathBuf> {
    let dir = diary_dir(personal_space);
    let read_dir = std::fs::read_dir(dir).ok()?;

    for entry in read_dir.flatten() {
        let path = entry.path();
        if !is_markdown(&path) {
            continue;
        }
        if read_page(&path).map(|page| page.id) == Some(page_id.to_string()) {
            return Some(path);
        }
    }

    None
}

fn read_page(path: &Path) -> Option<DiaryPage> {
    let raw = std::fs::read_to_string(path).ok()?;
    let (front, body) = split_front_matter(&raw);

    let mut id = None;
    let mut date = None;
    let mut title = String::new();
    let mut created_at = None;
    let mut updated_at = None;

    for line in front.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().to_string();
        match key.trim() {
            "id" => id = Some(value),
            "date" if !value.is_empty() && value != "null" => date = Some(value),
            "title" => title = value,
            "createdAt" => created_at = Some(value),
            "updatedAt" => updated_at = Some(value),
            _ => {}
        }
    }

    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("page")
        .to_string();
    let date = date.or_else(|| if is_date_key(&stem) { Some(stem) } else { None });

    let modified = std::fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
        .map(time::to_iso);

    Some(DiaryPage {
        id: id.unwrap_or_else(|| format!("page-{}", &hash::short_id(&path.to_string_lossy())[..8])),
        date,
        title,
        body: body.to_string(),
        created_at: created_at
            .or_else(|| modified.clone())
            .unwrap_or_else(time::now_iso),
        updated_at: updated_at.or(modified).unwrap_or_else(time::now_iso),
    })
}

/// Splits a leading `---` block from the Markdown body.
fn split_front_matter(raw: &str) -> (&str, &str) {
    if let Some(rest) = raw.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---\n") {
            let front = &rest[..end];
            let body = rest[end + 5..].trim_start_matches('\n');
            return (front, body);
        }
    }
    ("", raw)
}

fn single_line(text: &str) -> String {
    text.replace(['\r', '\n'], " ").trim().to_string()
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("md"))
        .unwrap_or(false)
}

/// `YYYY-MM-DD`.
fn is_date_key(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}
