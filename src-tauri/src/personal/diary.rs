//! The diary: one folder per dated page inside the personal space.
//!
//! Dated entries live in `diary/<dd-mm-yyyy>/entry.md` (e.g. `diary/15-11-2026/entry.md`);
//! free-form pages stay as single Markdown files in `diary/`. Each file carries a
//! small front matter block so the app can restore ids and titles. Everything
//! stays plain Markdown: readable, greppable and editable without this program.

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
    /// True for the date-named daily entry (`<dd-mm-yyyy>/<dd-mm-yyyy>.md`); its
    /// file name is fixed and it cannot be renamed.
    #[serde(default)]
    pub name_locked: bool,
    /// Just the file name, e.g. `15-11-2026.md`.
    #[serde(default)]
    pub file_name: String,
}

pub fn diary_dir(personal_space: &Path) -> PathBuf {
    personal_space.join(DIARY_DIR)
}

/// Free pages first, then dated entries newest to oldest.
pub fn list(personal_space: &Path) -> AppResult<Vec<DiaryPage>> {
    let dir = diary_dir(personal_space);
    migrate_legacy_dated_files(&dir);
    let read_dir = match std::fs::read_dir(&dir) {
        Ok(read_dir) => read_dir,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };

    let mut pages: Vec<DiaryPage> = Vec::new();
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            pages.extend(read_dated_dir_pages(&path));
        } else if is_markdown(&path) {
            if let Some(page) = read_page(&path) {
                pages.push(page);
            }
        }
    }

    pages.sort_by(|a, b| match (&a.date, &b.date) {
        (None, None) => a.created_at.cmp(&b.created_at),
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(left), Some(right)) => right
            .cmp(left)
            .then(b.name_locked.cmp(&a.name_locked))
            .then(a.created_at.cmp(&b.created_at)),
    });

    Ok(pages)
}

pub fn create(
    personal_space: &Path,
    date: Option<String>,
    title: String,
    name_locked: bool,
) -> AppResult<DiaryPage> {
    let now = time::now_iso();
    let date = date.filter(|value| is_date_key(value));
    let name_locked = name_locked && date.is_some();
    let page = DiaryPage {
        id: hash::unique_id("page"),
        date,
        title,
        body: String::new(),
        created_at: now.clone(),
        updated_at: now,
        name_locked,
        file_name: String::new(),
    };

    let target = file_for(personal_space, &page, name_locked);
    let page = DiaryPage {
        file_name: file_name_of(&target),
        ..page
    };

    write(personal_space, &page)?;
    Ok(page)
}

/// Writes the page back. A dated entry whose date changed is renamed, so one
/// day never ends up with two files. The date-named daily entry keeps its
/// fixed `<dd-mm-yyyy>.md` name.
pub fn save(personal_space: &Path, page: &DiaryPage) -> AppResult<DiaryPage> {
    let mut saved = page.clone();
    saved.updated_at = time::now_iso();

    let previous = find_file_by_id(personal_space, &page.id);
    let locked = saved.name_locked
        || previous.as_ref().map(|path| is_date_named(path)).unwrap_or(false);
    saved.name_locked = locked;
    saved.file_name = file_name_of(&file_for(personal_space, &saved, locked));

    if let Some(previous) = &previous {
        let target = file_for(personal_space, &saved, locked);
        if *previous != target && previous.exists() {
            let parent = previous.parent().map(Path::to_path_buf);
            std::fs::remove_file(previous)?;
            if let Some(parent) = parent {
                if parent != diary_dir(personal_space) {
                    let _ = std::fs::remove_dir(&parent);
                }
            }
        }
    }

    write(personal_space, &saved)?;
    Ok(saved)
}

pub fn delete(personal_space: &Path, page_id: &str) -> AppResult<()> {
    if let Some(path) = find_file_by_id(personal_space, page_id) {
        let parent = path.parent().map(Path::to_path_buf);
        std::fs::remove_file(&path)?;
        // A dated entry's directory is empty once its file is gone; drop it too.
        if let Some(parent) = parent {
            if parent != diary_dir(personal_space) {
                let _ = std::fs::remove_dir(&parent);
            }
        }
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

    let target = file_for(personal_space, page, page.name_locked);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(target, out)?;
    Ok(())
}

/// Dated locked pages live in `diary/<dd-mm-yyyy>/<dd-mm-yyyy>.md` so each day
/// has exactly one date-named file; other pages of that date live alongside it.
fn file_for(personal_space: &Path, page: &DiaryPage, name_locked: bool) -> PathBuf {
    let dir = diary_dir(personal_space);
    let slug_name = |page: &DiaryPage| {
        let title = if page.title.trim().is_empty() {
            "page"
        } else {
            page.title.as_str()
        };
        format!("{}-{}.md", hash::slug(title), id_suffix(&page.id))
    };
    match &page.date {
        Some(date) if is_date_key(date) => {
            let date_dir = dir.join(date_dir_name(date));
            if name_locked {
                let dir_name = date_dir_name(date);
                date_dir.join(format!("{dir_name}.md"))
            } else {
                date_dir.join(slug_name(page))
            }
        }
        _ => dir.join(slug_name(page)),
    }
}

/// Just the final path segment, e.g. `15-11-2026.md`.
fn file_name_of(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("page.md")
        .to_string()
}

/// A file is the date-named daily entry when its stem equals its `dd-mm-yyyy` directory.
fn is_date_named(path: &Path) -> bool {
    let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("");
    let parent = path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("");
    !stem.is_empty() && stem == parent && date_key_from_dir_name(parent).is_some()
}

/// `YYYY-MM-DD` → `dd-mm-yyyy` for the on-disk directory name.
fn date_dir_name(date: &str) -> String {
    let mut parts = date.split('-');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(year), Some(month), Some(day)) => format!("{day}-{month}-{year}"),
        _ => date.to_string(),
    }
}

/// `dd-mm-yyyy` → `YYYY-MM-DD`, or `None` when the directory name is not a date.
fn date_key_from_dir_name(name: &str) -> Option<String> {
    let bytes = name.as_bytes();
    if !(bytes.len() == 10
        && bytes[2] == b'-'
        && bytes[5] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 2 || index == 5 || byte.is_ascii_digit()))
    {
        return None;
    }
    let day = &name[..2];
    let month = &name[3..5];
    let year = &name[6..10];
    let key = format!("{year}-{month}-{day}");
    is_date_key(&key).then_some(key)
}

/// Reads the entry inside a `dd-mm-yyyy` directory, if there is one.
fn read_dated_dir_pages(dir: &Path) -> Vec<DiaryPage> {
    let Some(date) = dir
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(date_key_from_dir_name)
    else {
        return Vec::new();
    };
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    read_dir
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_markdown(path))
        .filter_map(|path| read_page(&path))
        .map(|mut page| {
            if page.date.is_none() {
                page.date = Some(date.clone());
            }
            page
        })
        .collect()
}

/// Moves legacy `diary/YYYY-MM-DD.md` files into the per-date directory layout.
fn migrate_legacy_dated_files(dir: &Path) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() || !is_markdown(&path) {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("")
            .to_string();
        if !is_date_key(&stem) {
            continue;
        }
        let target_dir = dir.join(date_dir_name(&stem));
        if std::fs::create_dir_all(&target_dir).is_ok() {
            let _ = std::fs::rename(&path, target_dir.join(format!("{}.md", date_dir_name(&stem))));
        }
    }

    // Earlier builds used `entry.md` inside date directories; rename to the date name.
    if let Ok(read_dir) = std::fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(dir_name) = path.file_name().and_then(|name| name.to_str()).map(str::to_string) else {
                continue;
            };
            if date_key_from_dir_name(&dir_name).is_none() {
                continue;
            }
            let legacy = path.join("entry.md");
            let target = path.join(format!("{dir_name}.md"));
            if legacy.exists() && !target.exists() {
                let _ = std::fs::rename(&legacy, &target);
            }
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
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Ok(inner) = std::fs::read_dir(&path) {
                    candidates.extend(inner.flatten().map(|item| item.path()));
                }
            } else {
                candidates.push(path);
            }
        }
    }

    for path in candidates {
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
    let parent_date = path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .and_then(date_key_from_dir_name);
    let date = date
        .or(parent_date)
        .or_else(|| if is_date_key(&stem) { Some(stem) } else { None });

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
        name_locked: is_date_named(path),
        file_name: file_name_of(path),
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
