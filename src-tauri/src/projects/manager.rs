//! Project lifecycle: register a folder, list what is registered, read the
//! files inside a folder, and flip the per-project internet toggle.

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::config::app_config::{ProjectRecord, Registry};
use crate::config::paths::AppDirs;
use crate::error::{AppError, AppResult};
use crate::models::registry as models;
use crate::projects::layout::{
    ProjectFile, ProjectFolders, ProjectPaths, ProjectTemplate, RESERVED_DIRS, STATE_DIR,
};
use crate::projects::templates;
use crate::projects::{Project, SourceFile, SourceStatus};
use crate::util::{hash, time};

/// Upper bound on a single folder scan, so a huge directory cannot stall the UI.
const MAX_SOURCES: usize = 4_000;

/// Formats the ingest pipeline can read in v1.
const SUPPORTED_EXTENSIONS: [&str; 6] = ["pdf", "docx", "md", "txt", "csv", "xlsx"];

/// Everything the manager needs that does not come from the caller.
pub struct Context<'a> {
    pub registry: &'a Registry,
    pub dirs: &'a AppDirs,
}

pub fn is_supported_source(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            let extension = extension.to_ascii_lowercase();
            SUPPORTED_EXTENSIONS.contains(&extension.as_str())
        })
        .unwrap_or(false)
}

/// Stable id for a folder, so reopening it reuses the same project entry.
pub fn project_id_for(path: &Path) -> String {
    let label = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project");
    let key = path.to_string_lossy().replace('\\', "/").to_lowercase();
    let digest = hash::short_id(&key);
    format!("{}-{}", hash::slug(label), &digest[..8])
}

fn folder_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Untitled project")
        .to_string()
}

fn to_project(context: &Context, record: &ProjectRecord) -> Project {
    let root = PathBuf::from(&record.path);
    let on_disk = ProjectPaths::new(&root).read();

    let source_count = scan_sources(&record.id, &root)
        .map(|files| files.len())
        .unwrap_or(0);

    Project {
        id: record.id.clone(),
        name: record.name.clone(),
        path: record.path.to_string_lossy().to_string(),
        template: record.template,
        source_count,
        internet_enabled: record.internet_enabled,
        system_prompt: record.system_prompt.clone(),
        diary_enabled: on_disk.as_ref().map(|file| file.diary_enabled).unwrap_or(false),
        custom_folders: on_disk
            .as_ref()
            .map(|file| file.folders.custom.clone())
            .unwrap_or_default(),
        model: models::selected_label(context.dirs, &context.registry.settings),
        last_opened_at: record
            .last_opened_at
            .clone()
            .unwrap_or_else(|| folder_modified_at(&root).unwrap_or_else(time::now_iso)),
    }
}

fn folder_modified_at(path: &Path) -> Option<String> {
    std::fs::metadata(path).ok()?.modified().ok().map(time::to_iso)
}

pub fn list(context: &Context) -> Vec<Project> {
    let mut projects: Vec<Project> = context
        .registry
        .projects
        .iter()
        .filter(|record| record.path.is_dir())
        .map(|record| to_project(context, record))
        .collect();

    projects.sort_by(|a, b| b.last_opened_at.cmp(&a.last_opened_at));
    projects
}

pub fn open(registry: &mut Registry, dirs: &AppDirs, id: &str) -> AppResult<Project> {
    let record = registry
        .find(id)
        .cloned()
        .ok_or_else(|| AppError::UnknownProject(id.to_string()))?;

    if !record.path.is_dir() {
        return Err(AppError::msg(format!(
            "the folder for “{}” is no longer at {}",
            record.name,
            record.path.display()
        )));
    }

    let touched = ProjectRecord {
        last_opened_at: Some(time::now_iso()),
        ..record
    };
    registry.upsert(touched.clone());
    registry.save(&dirs.state_file)?;

    // An older project gains the standard folders the first time it is opened
    // after upgrading, without touching the files it already has. A manifest
    // still sitting in `.LocalAIse/` moves to the project root.
    let paths = ProjectPaths::new(touched.path.clone());
    match paths.read() {
        Some(file) => {
            if !paths.project_file().exists() {
                paths.write(&file)?;
            }
            paths.ensure_structure(&file.folders, file.diary_enabled)?;
        }
        None => paths.ensure_structure(&ProjectFolders::default(), false)?,
    }

    Ok(to_project(&Context { registry, dirs }, &touched))
}

pub fn sources(registry: &Registry, id: &str) -> AppResult<Vec<SourceFile>> {
    let record = registry
        .find(id)
        .ok_or_else(|| AppError::UnknownProject(id.to_string()))?;
    scan_sources(&record.id, &record.path)
}

/// Registers a folder the user picked. Reopening a folder keeps its settings.
pub fn add_folder(registry: &mut Registry, dirs: &AppDirs, folder: &Path) -> AppResult<Project> {
    register(registry, dirs, folder, None)
}

/// Creates a new folder under `parent`, applies the template, then registers it.
pub fn create_folder(
    registry: &mut Registry,
    dirs: &AppDirs,
    parent: &Path,
    template: ProjectTemplate,
) -> AppResult<Project> {
    std::fs::create_dir_all(parent)?;
    let parent = parent
        .canonicalize()
        .map_err(|error| AppError::msg(format!("cannot open {}: {error}", parent.display())))?;

    let folder = unique_folder(&parent, default_name(template));
    std::fs::create_dir_all(&folder)?;
    templates::apply(template, &folder)?;

    register(registry, dirs, &folder, Some(template))
}

fn register(
    registry: &mut Registry,
    dirs: &AppDirs,
    folder: &Path,
    template_override: Option<ProjectTemplate>,
) -> AppResult<Project> {
    let folder = folder
        .canonicalize()
        .map_err(|error| AppError::msg(format!("cannot open {}: {error}", folder.display())))?;

    if !folder.is_dir() {
        return Err(AppError::msg(format!(
            "{} is not a folder",
            folder.display()
        )));
    }

    let id = project_id_for(&folder);
    let name = folder_name(&folder);
    let paths = ProjectPaths::new(folder.clone());

    // A folder that was already a project keeps its template, toggle, system
    // prompt, folder layout and diary setting.
    let on_disk = paths.read();
    let known = registry.find(&id).cloned();

    let template = template_override
        .or_else(|| known.as_ref().map(|record| record.template))
        .or_else(|| on_disk.as_ref().map(|file| file.template))
        .unwrap_or_default();

    let internet_enabled = on_disk
        .as_ref()
        .map(|file| file.internet_enabled)
        .or_else(|| known.as_ref().map(|record| record.internet_enabled))
        .unwrap_or(registry.settings.default_internet);

    let system_prompt = on_disk
        .as_ref()
        .and_then(|file| file.system_prompt.clone())
        .or_else(|| known.as_ref().and_then(|record| record.system_prompt.clone()));

    // An existing manifest keeps its layout, diary setting and RAG scope; a
    // fresh folder gets the defaults.
    let (folders, diary_enabled, rag_paths) = on_disk
        .as_ref()
        .map(|file: &ProjectFile| {
            (file.folders.clone(), file.diary_enabled, file.rag_paths.clone())
        })
        .unwrap_or((
            ProjectFolders::default(),
            false,
            Vec::new(),
        ));

    paths.write(&ProjectFile {
        id: id.clone(),
        name: name.clone(),
        template,
        internet_enabled,
        system_prompt: system_prompt.clone(),
        folders: folders.clone(),
        diary_enabled,
        rag_paths: rag_paths.clone(),
    })?;
    paths.ensure_structure(&folders, diary_enabled)?;

    let record = ProjectRecord {
        id,
        name,
        path: folder,
        template,
        internet_enabled,
        system_prompt,
        last_opened_at: Some(time::now_iso()),
    };

    registry.upsert(record.clone());
    registry.save(&dirs.state_file)?;

    Ok(to_project(&Context { registry, dirs }, &record))
}

pub fn set_internet(
    registry: &mut Registry,
    dirs: &AppDirs,
    id: &str,
    enabled: bool,
) -> AppResult<Project> {
    let record = registry
        .find(id)
        .cloned()
        .ok_or_else(|| AppError::UnknownProject(id.to_string()))?;

    let paths = ProjectPaths::new(record.path.clone());
    let mut file = paths.read().unwrap_or_else(|| ProjectFile {
        system_prompt: record.system_prompt.clone(),
        ..ProjectFile::new(
            record.id.clone(),
            record.name.clone(),
            record.template,
            enabled,
        )
    });
    file.internet_enabled = enabled;
    paths.write(&file)?;

    let updated = ProjectRecord {
        internet_enabled: enabled,
        ..record
    };
    registry.upsert(updated.clone());
    registry.save(&dirs.state_file)?;

    Ok(to_project(&Context { registry, dirs }, &updated))
}

/// Removes a project from the app. With `delete_files` the project folder is
/// deleted from disk as well; otherwise the folder stays exactly as it is and
/// only the app stops listing it.
pub fn remove(
    registry: &mut Registry,
    dirs: &AppDirs,
    id: &str,
    delete_files: bool,
) -> AppResult<()> {
    let record = registry
        .find(id)
        .cloned()
        .ok_or_else(|| AppError::UnknownProject(id.to_string()))?;

    if delete_files {
        // Never delete a filesystem root, and never follow a link out of the
        // folder the user registered.
        let root = &record.path;
        if root.parent().is_none() || root.as_os_str().is_empty() {
            return Err(AppError::msg(
                "that folder is a drive root and cannot be deleted from the app.",
            ));
        }
        match std::fs::remove_dir_all(root) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(AppError::msg(format!(
                    "could not delete {}: {error}",
                    root.display()
                )))
            }
        }
    }

    registry.projects.retain(|project| project.id != id);
    registry.settings.project_profiles.remove(id);
    registry.save(&dirs.state_file)?;
    Ok(())
}

/// Changes the project's display name. The folder on disk keeps its path, so
/// only the label in the app and `project.json` change.
pub fn rename(
    registry: &mut Registry,
    dirs: &AppDirs,
    id: &str,
    new_name: &str,
) -> AppResult<Project> {
    let name = new_name.trim();
    if name.is_empty() {
        return Err(AppError::msg("type a project name first."));
    }
    if name.chars().any(|character| character.is_control()) {
        return Err(AppError::msg("a project name cannot contain control characters."));
    }
    if name.chars().count() > 80 {
        return Err(AppError::msg("that project name is too long."));
    }

    let record = registry
        .find(id)
        .cloned()
        .ok_or_else(|| AppError::UnknownProject(id.to_string()))?;

    let paths = ProjectPaths::new(record.path.clone());
    let mut file = paths.read().unwrap_or_else(|| {
        ProjectFile::new(
            record.id.clone(),
            record.name.clone(),
            record.template,
            record.internet_enabled,
        )
    });
    file.name = name.to_string();
    paths.write(&file)?;

    let updated = ProjectRecord {
        name: name.to_string(),
        ..record
    };
    registry.upsert(updated.clone());
    registry.save(&dirs.state_file)?;

    Ok(to_project(&Context { registry, dirs }, &updated))
}

/// Stores or clears the project's chat system prompt. `None` (or a blank
/// string) returns the project to the built-in default.
pub fn set_system_prompt(
    registry: &mut Registry,
    dirs: &AppDirs,
    id: &str,
    prompt: Option<String>,
) -> AppResult<Project> {
    let prompt = prompt
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty());

    let record = registry
        .find(id)
        .cloned()
        .ok_or_else(|| AppError::UnknownProject(id.to_string()))?;

    let paths = ProjectPaths::new(record.path.clone());
    let mut file = paths.read().unwrap_or_else(|| {
        ProjectFile::new(
            record.id.clone(),
            record.name.clone(),
            record.template,
            record.internet_enabled,
        )
    });
    file.system_prompt = prompt.clone();
    paths.write(&file)?;

    let updated = ProjectRecord {
        system_prompt: prompt,
        ..record
    };
    registry.upsert(updated.clone());
    registry.save(&dirs.state_file)?;

    Ok(to_project(&Context { registry, dirs }, &updated))
}

/// Enables or disables the project's own `diary/` folder. Turning it off keeps
/// every page on disk; the folder is simply ignored until it is turned back on.
pub fn set_diary(
    registry: &Registry,
    dirs: &AppDirs,
    id: &str,
    enabled: bool,
) -> AppResult<Project> {
    let record = registry
        .find(id)
        .cloned()
        .ok_or_else(|| AppError::UnknownProject(id.to_string()))?;

    let paths = ProjectPaths::new(record.path.clone());
    let mut file = paths.read().unwrap_or_else(|| {
        ProjectFile::new(
            record.id.clone(),
            record.name.clone(),
            record.template,
            record.internet_enabled,
        )
    });
    file.diary_enabled = enabled;
    paths.write(&file)?;

    if enabled && !file.folders.diary.is_empty() {
        std::fs::create_dir_all(record.path.join(&file.folders.diary))?;
    }

    Ok(to_project(&Context { registry, dirs }, &record))
}

/// Adds a folder to the project directory and records it in `project.json`.
/// Reserved names (`chats`, `diary`, `outputs`, `.LocalAIse`) are refused.
pub fn add_subfolder(
    registry: &Registry,
    dirs: &AppDirs,
    id: &str,
    name: &str,
) -> AppResult<Project> {
    let name = clean_folder_name(name)
        .ok_or_else(|| AppError::msg("type a folder name first."))?;

    let record = registry
        .find(id)
        .cloned()
        .ok_or_else(|| AppError::UnknownProject(id.to_string()))?;
    let root = PathBuf::from(&record.path);
    let paths = ProjectPaths::new(&root);

    let mut file = paths.read().unwrap_or_else(|| {
        ProjectFile::new(
            record.id.clone(),
            record.name.clone(),
            record.template,
            record.internet_enabled,
        )
    });

    if is_reserved_name(&name) || name == STATE_DIR {
        return Err(AppError::msg(format!(
            "“{name}” is reserved for the app; pick another folder name."
        )));
    }
    if file.folders.custom.contains(&name) || root.join(&name).is_dir() {
        return Err(AppError::msg(format!("a folder named “{name}” already exists.")));
    }

    std::fs::create_dir_all(root.join(&name))?;
    file.folders.custom.push(name.clone());
    paths.write(&file)?;

    Ok(to_project(&Context { registry, dirs }, &record))
}

/// Trims a user-typed folder name and drops characters a filesystem refuses.
fn clean_folder_name(name: &str) -> Option<String> {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => ' ',
            character => character,
        })
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

fn is_reserved_name(name: &str) -> bool {
    crate::projects::layout::RESERVED_DIRS.contains(&name.to_ascii_lowercase().as_str())
}

/// Every supported file in the project's own folders. The app's reserved
/// folders (`chats/`, `diary/`, `outputs/`, `.LocalAIse/`) are skipped so chat
/// transcripts and diary pages never end up as indexed sources. When the
/// project lists a RAG scope in its manifest, only those folders or files are
/// included.
pub fn scan_sources(project_id: &str, root: &Path) -> AppResult<Vec<SourceFile>> {
    let root = root
        .canonicalize()
        .map_err(|error| AppError::msg(format!("cannot read {}: {error}", root.display())))?;

    let rag_paths = ProjectPaths::new(&root)
        .read()
        .map(|file| file.rag_paths)
        .unwrap_or_default()
        .into_iter()
        .filter(|path| !path.trim().is_empty())
        .map(|path| path.trim().replace('\\', "/").trim_matches('/').to_string())
        .filter(|path| !path.is_empty())
        .collect::<Vec<_>>();

    let mut files = Vec::new();

    let walker = WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.depth() == 0 || (!is_hidden(entry) && !is_reserved(entry)));

    for entry in walker {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() || !is_supported_source(entry.path()) {
            continue;
        }

        let Ok(metadata) = entry.metadata() else { continue };
        let relative = entry.path().strip_prefix(&root).unwrap_or(entry.path());
        let name = relative.to_string_lossy().replace('\\', "/");

        if !rag_paths.is_empty()
            && !rag_paths
                .iter()
                .any(|allowed| allowed == &name || name.starts_with(&format!("{allowed}/")))
        {
            continue;
        }

        let file_stamp = metadata
            .modified()
            .map(time::to_iso)
            .unwrap_or_else(|_| time::now_iso());

        let index_db = crate::index::db::IndexDb::new(&root);
        let project_index = index_db.load();
        let status = if project_index.indexed_files.contains_key(&name) {
            SourceStatus::Indexed
        } else {
            SourceStatus::Pending
        };

        files.push(SourceFile {
            id: format!("{project_id}:{name}"),
            name,
            status,
            size_bytes: metadata.len(),
            modified_at: file_stamp,
        });

        if files.len() >= MAX_SOURCES {
            break;
        }
    }

    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(files)
}

fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry.file_name().to_string_lossy().starts_with('.')
}

/// Folders the app writes itself; the knowledge base never reads inside them.
fn is_reserved(entry: &walkdir::DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
    RESERVED_DIRS.contains(&name.as_str())
}

fn default_name(template: ProjectTemplate) -> &'static str {
    match template {
        ProjectTemplate::Budget => "Budget project",
        ProjectTemplate::ContentWriting => "Writing project",
        ProjectTemplate::Research => "Research project",
        ProjectTemplate::General => "New project",
    }
}

/// Appends a number until the folder name is free.
fn unique_folder(parent: &Path, name: &str) -> PathBuf {
    let mut candidate = parent.join(name);
    let mut index = 2;
    while candidate.exists() {
        candidate = parent.join(format!("{name} {index}"));
        index += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_project(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "LocalAIse-scan-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }

    fn project_with_rag(root: &Path, rag_paths: Vec<String>) -> ProjectPaths {
        let paths = ProjectPaths::new(root);
        let mut file = ProjectFile::new(
            "test-project".to_string(),
            "Test".to_string(),
            ProjectTemplate::General,
            false,
        );
        file.rag_paths = rag_paths;
        paths.write(&file).unwrap();
        paths
    }

    fn scanned_names(root: &Path) -> Vec<String> {
        scan_sources("test-project", root)
            .unwrap()
            .into_iter()
            .map(|file| file.name)
            .collect()
    }

    #[test]
    fn scan_skips_chats_diary_and_outputs_but_reads_data() {
        let root = temp_project("reserved");
        write_file(&root.join("data").join("paper.md"), "# Paper");
        write_file(&root.join("chats").join("session.json"), "{}");
        write_file(&root.join("diary").join("2026-10-22.md"), "# Day");
        write_file(&root.join("outputs").join("report.md"), "# Report");
        write_file(&root.join("notes.md"), "# Notes");
        project_with_rag(&root, Vec::new());

        let names = scanned_names(&root);
        assert_eq!(names, vec!["data/paper.md".to_string(), "notes.md".to_string()]);

        let _ = std::fs::remove_dir_all(&root);
    }

    fn test_dirs(base: &Path) -> AppDirs {
        AppDirs {
            base: base.to_path_buf(),
            state_file: base.join("state.json"),
            models: base.join("models"),
            llama: base.join("llama"),
            runtime: base.join("runtime"),
            default_projects: base.join("projects"),
            default_personal: base.join("personal"),
        }
    }

    #[test]
    fn renaming_changes_the_label_but_keeps_the_folder() {
        let base = temp_project("rename");
        let folder = base.join("Budget");
        std::fs::create_dir_all(&folder).unwrap();
        let mut registry = Registry::default();
        let dirs = test_dirs(&base);
        let project = register(&mut registry, &dirs, &folder, None).unwrap();
        let original_path = project.path.clone();

        let renamed = rename(&mut registry, &dirs, &project.id, "House budget").unwrap();
        assert_eq!(renamed.name, "House budget");
        assert_eq!(renamed.path, original_path);
        assert!(folder.exists());
        assert_eq!(
            ProjectPaths::new(&folder).read().unwrap().name,
            "House budget"
        );
        assert_eq!(registry.find(&project.id).unwrap().name, "House budget");
        assert!(rename(&mut registry, &dirs, &project.id, "   ").is_err());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn removing_keeps_files_unless_asked_to_delete_them() {
        let base = temp_project("remove");
        let folder = base.join("Notes");
        std::fs::create_dir_all(&folder).unwrap();
        let mut registry = Registry::default();
        let dirs = test_dirs(&base);
        let project = register(&mut registry, &dirs, &folder, None).unwrap();

        remove(&mut registry, &dirs, &project.id, false).unwrap();
        assert!(registry.find(&project.id).is_none());
        assert!(folder.join("project.json").exists());

        let project = register(&mut registry, &dirs, &folder, None).unwrap();
        remove(&mut registry, &dirs, &project.id, true).unwrap();
        assert!(!folder.exists());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn scan_honours_a_rag_scope_of_folders_and_files() {        let root = temp_project("scope");
        write_file(&root.join("data").join("budget.xlsx"), "x");
        write_file(&root.join("data").join("notes.md"), "# Notes");
        write_file(&root.join("invoices").join("acme.pdf"), "%PDF");
        write_file(&root.join("private").join("secret.md"), "# Secret");
        project_with_rag(
            &root,
            vec!["data".to_string(), "invoices/acme.pdf".to_string()],
        );

        let names = scanned_names(&root);
        assert_eq!(
            names,
            vec![
                "data/budget.xlsx".to_string(),
                "data/notes.md".to_string(),
                "invoices/acme.pdf".to_string()
            ]
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
