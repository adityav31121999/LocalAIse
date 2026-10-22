//! Starter content for new projects.
//!
//! A template only seeds a folder the app itself creates, and it never
//! overwrites an existing file.

use std::path::Path;

use crate::error::AppResult;
use crate::projects::layout::ProjectTemplate;

struct SeedFile {
    path: &'static str,
    contents: &'static str,
}

const BUDGET: &[SeedFile] = &[
    SeedFile {
        path: "data/categories.csv",
        contents: "category,monthly_limit\nsoftware,120\nhardware,400\ntravel,250\n",
    },
    SeedFile {
        path: "data/notes.md",
        contents: "# Budget notes\n\nDrop invoices into `data/invoices/` and they show up as sources.\n",
    },
];

const CONTENT_WRITING: &[SeedFile] = &[
    SeedFile {
        path: "data/brand-notes.md",
        contents: "# Brand notes\n\nTone, vocabulary and anything the drafts must stay consistent with.\n",
    },
    SeedFile {
        path: "data/drafts/README.md",
        contents: "Drafts live here. The assistant reads this folder as its style reference.\n",
    },
    SeedFile {
        path: "data/README.md",
        contents: "# Write project\n\nReference material the assistant should read goes in this folder.\n",
    },
];

const RESEARCH: &[SeedFile] = &[
    SeedFile {
        path: "data/notes.md",
        contents: "# Research notes\n\nQuestions, findings and open threads.\n",
    },
    SeedFile {
        path: "data/papers/README.md",
        contents: "Put PDFs here so they can be cited from chat.\n",
    },
];

/// Every newly created project gets a short guide to the folder structure, so
/// the directory stays self-explanatory when opened in a file explorer.
const STRUCTURE_README: SeedFile = SeedFile {
    path: "README.md",
    contents: "# Project folder\n\n\
        - `data/` — the files the assistant reads, answers from and cites.\n\
        - `chats/` — every chat session saved as its own JSON file.\n\
        - `diary/` — one Markdown page per day, when the project diary is on.\n\
        - `outputs/` — files the assistant produces for you.\n\
        - other folders are yours; create anything and the assistant can read it.\n",
};

/// Writes the template's starter files into a newly created folder.
pub fn apply(template: ProjectTemplate, root: &Path) -> AppResult<Vec<String>> {
    let seeds = match template {
        ProjectTemplate::Budget => BUDGET,
        ProjectTemplate::ContentWriting => CONTENT_WRITING,
        ProjectTemplate::Research => RESEARCH,
        ProjectTemplate::General => &[],
    };

    let mut written = Vec::new();
    for seed in std::iter::once(&STRUCTURE_README).chain(seeds.iter()) {
        let target = root.join(seed.path);
        if target.exists() {
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, seed.contents)?;
        written.push(seed.path.to_string());
    }

    Ok(written)
}
