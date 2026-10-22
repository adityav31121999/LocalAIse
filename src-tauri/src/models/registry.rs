//! Which GGUF files are on this machine, and which one chat should load.
//!
//! Models live in folders, and a folder holds folders: the common layout is
//! `models/llm/<model-name>/<model-name>.gguf`, with the multimodal
//! projector of a vision model sitting in the same `<model-name>` folder.
//! The scan therefore walks sub-folders and groups the files it finds by
//! the folder they live in.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::config::app_config::Settings;
use crate::config::paths::AppDirs;
use crate::error::{AppError, AppResult};
use crate::models::naming;
use crate::models::quant::Quant;
use crate::models::shape::read_meta;

/// How deep the scan walks below a registered folder.
const MAX_DEPTH: usize = 8;

/// What a GGUF file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelFileKind {
    /// The weights of a language model.
    Model,
    /// A multimodal projector that belongs to the weights next to it.
    Projector,
}

/// One GGUF file on disk.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelEntry {
    /// Readable name derived from the file name.
    pub name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    /// True for files in the app's own models folder (they can be removed
    /// from the UI); false for GGUFs the user keeps in a registered folder.
    pub internal: bool,
    /// The folder the file lives in — the `llm-name` folder.
    pub dir: PathBuf,
    pub kind: ModelFileKind,
    /// Parameter count in billions, 0 when the name and the file do not say.
    pub params_billion: f32,
    /// Quantization label such as "Q4_K_M"; empty when unknown.
    pub quantization: String,
    /// GGUF architecture such as "llama"; empty when unknown.
    pub architecture: String,
    /// True when the app stores a configuration of its own for this file.
    #[serde(default)]
    pub has_profile: bool,
}

/// A model and everything that belongs to it.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelGroup {
    /// Stable identifier for the group — the folder it lives in.
    pub id: String,
    /// Display name, taken from the folder or the file.
    pub name: String,
    /// The folder the model lives in.
    pub dir: PathBuf,
    pub internal: bool,
    /// The weight files in the folder. The first one is the primary model.
    pub models: Vec<ModelEntry>,
    /// Projector files that belong to the weights.
    pub projectors: Vec<ModelEntry>,
    /// Bytes of every file in the group.
    pub total_bytes: u64,
}

impl ModelGroup {
    /// The file the app loads for this group.
    pub fn primary(&self) -> Option<&ModelEntry> {
        self.models.first()
    }
}

/// A GGUF file's name says it is a projector.
fn file_name_looks_like_projector(file_name: &str) -> bool {
    let lowered = file_name.to_ascii_lowercase();
    lowered.contains("mmproj")
        || lowered.starts_with("clip-")
        || lowered.starts_with("clip_")
        || lowered.contains("mmproj-")
}

fn has_gguf_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("gguf"))
        .unwrap_or(false)
}

/// Hidden folders hold editor state and caches, not models.
fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|name| name.starts_with('.'))
        .unwrap_or(false)
}

/// Reads a file's metadata into a registry entry.
fn describe(path: &Path, internal: bool) -> ModelEntry {
    let meta = read_meta(path);
    let file_name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();

    let kind = if meta.is_projector || file_name_looks_like_projector(file_name) {
        ModelFileKind::Projector
    } else {
        ModelFileKind::Model
    };

    let params_billion = if meta.parameter_count > 0 {
        (meta.parameter_count as f64 / 1_000_000_000.0) as f32
    } else {
        naming::params_billion_from_name(file_name).unwrap_or(0.0)
    };

    ModelEntry {
        name: label(path),
        path: path.to_path_buf(),
        size_bytes: std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0),
        internal,
        dir: path.parent().map(Path::to_path_buf).unwrap_or_default(),
        kind,
        params_billion,
        quantization: Quant::from_filename(file_name)
            .map(|quant| quant.as_label().to_string())
            .unwrap_or_default(),
        architecture: meta.architecture,
        has_profile: false,
    }
}

/// Every GGUF file under `root`, walking sub-folders.
fn scan_root(root: &Path, internal: bool) -> Vec<ModelEntry> {
    let mut entries = Vec::new();
    if !root.is_dir() {
        return entries;
    }

    let walker = WalkDir::new(root)
        .max_depth(MAX_DEPTH)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !is_hidden(entry));

    for entry in walker.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !has_gguf_extension(path) {
            continue;
        }
        entries.push(describe(path, internal));
    }

    entries
}

/// The models and their projectors, grouped by the folder they live in.
fn scan_groups(root: &Path, internal: bool) -> Vec<ModelGroup> {
    let entries = scan_root(root, internal);
    if entries.is_empty() {
        return Vec::new();
    }

    let mut by_dir: BTreeMap<PathBuf, ModelGroup> = BTreeMap::new();
    for entry in entries {
        let dir = entry.dir.clone();
        let group = by_dir.entry(dir.clone()).or_insert_with(|| ModelGroup {
            id: dir.to_string_lossy().into_owned(),
            name: String::new(),
            dir,
            internal,
            models: Vec::new(),
            projectors: Vec::new(),
            total_bytes: 0,
        });

        match entry.kind {
            ModelFileKind::Model => group.models.push(entry),
            ModelFileKind::Projector => group.projectors.push(entry),
        }
    }

    let mut groups = Vec::new();
    for mut group in by_dir.into_values() {
        // A flat folder — models dropped straight into the root — becomes one
        // group per file, because the folder name says nothing about them.
        if group.dir == root && group.models.len() > 1 {
            groups.extend(split_flat_group(group));
            continue;
        }
        finish_group(&mut group, root);
        groups.push(group);
    }

    groups
}

/// Splits a folder full of unrelated models into one group per model.
fn split_flat_group(group: ModelGroup) -> Vec<ModelGroup> {
    let mut projectors = group.projectors;
    let mut groups = Vec::new();

    for model in group.models {
        let mut single = ModelGroup {
            id: model.path.to_string_lossy().into_owned(),
            name: model.name.clone(),
            dir: group.dir.clone(),
            internal: group.internal,
            models: vec![model],
            projectors: Vec::new(),
            total_bytes: 0,
        };

        // Give the projector to the model it belongs to, judged by name.
        if let Some(index) = best_projector_match(&single, &projectors) {
            single.projectors.push(projectors.remove(index));
        }
        recompute_total(&mut single);
        groups.push(single);
    }

    // Projectors that matched no model stay on their own.
    for projector in projectors {
        let mut lonely = ModelGroup {
            id: projector.path.to_string_lossy().into_owned(),
            name: projector.name.clone(),
            dir: projector.dir.clone(),
            internal: projector.internal,
            models: Vec::new(),
            projectors: vec![projector],
            total_bytes: 0,
        };
        recompute_total(&mut lonely);
        groups.push(lonely);
    }

    groups
}

/// Gives the group its display name and orders its files.
fn finish_group(group: &mut ModelGroup, root: &Path) {
    let dir_name = group
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    // Order the weight files: the one named after the folder comes first,
    // then the biggest.
    let needle = naming::normalize(dir_name);
    group.models.sort_by(|a, b| {
        let a_match = naming::common_prefix_len(&naming::normalize(&a.name), &needle);
        let b_match = naming::common_prefix_len(&naming::normalize(&b.name), &needle);
        b_match
            .cmp(&a_match)
            .then_with(|| b.size_bytes.cmp(&a.size_bytes))
            .then_with(|| a.name.cmp(&b.name))
    });
    group
        .projectors
        .sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes).then_with(|| a.name.cmp(&b.name)));

    group.name = if group.dir == root {
        // A flat folder: the file names it.
        group
            .primary()
            .map(|model| model.name.clone())
            .or_else(|| group.projectors.first().map(|p| p.name.clone()))
            .unwrap_or_else(|| dir_name.to_string())
    } else {
        naming::label(dir_name)
    };

    recompute_total(group);
}

fn recompute_total(group: &mut ModelGroup) {
    group.total_bytes = group
        .models
        .iter()
        .chain(group.projectors.iter())
        .map(|entry| entry.size_bytes)
        .sum();
}

/// The projector that belongs to a group, judged by how much of the name
/// the two share.
fn best_projector_match(group: &ModelGroup, projectors: &[ModelEntry]) -> Option<usize> {
    let name = naming::normalize(&group.name);
    let mut best: Option<(usize, usize)> = None;

    for (index, projector) in projectors.iter().enumerate() {
        let common = naming::common_prefix_len(&name, &naming::normalize(&projector.name));
        if common == 0 {
            continue;
        }
        if best.is_none_or(|(_, length)| common > length) {
            best = Some((index, common));
        }
    }

    best.map(|(index, _)| index)
}

/// The models and their projectors on this machine: the app's own models
/// folder plus every folder the user registered.
pub fn groups(dirs: &AppDirs, extra_folders: &[PathBuf]) -> Vec<ModelGroup> {
    let mut roots: Vec<(PathBuf, bool)> = vec![(dirs.models.clone(), true)];
    for folder in extra_folders {
        roots.push((folder.clone(), false));
    }

    let mut found: Vec<ModelGroup> = Vec::new();
    for (root, internal) in roots {
        found.extend(scan_groups(&root, internal));
    }

    // The same folder can be registered twice, and a registered folder can
    // contain the app's own models folder; groups are deduped on their path.
    let mut seen: HashSet<String> = HashSet::new();
    found.retain(|group| seen.insert(group.id.clone()));

    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// Every GGUF file the app can load: the app's models folder plus every
/// GGUF folder the user added, walking sub-folders.
pub fn installed(dirs: &AppDirs, extra_folders: &[PathBuf]) -> Vec<ModelEntry> {
    let mut entries: Vec<ModelEntry> = groups(dirs, extra_folders)
        .into_iter()
        .flat_map(|group| group.models)
        .collect();

    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

/// The multimodal projector that belongs to a model file.
///
/// llama.cpp expects the projector next to the weights; a model with exactly
/// one projector beside it always uses that one.
pub fn projector_for(model_path: &Path) -> Option<PathBuf> {
    let dir = model_path.parent()?;
    let projectors: Vec<ModelEntry> = scan_root(dir, false)
        .into_iter()
        .filter(|entry| entry.kind == ModelFileKind::Projector)
        .collect();

    if projectors.is_empty() {
        return None;
    }

    let name = naming::normalize(
        model_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default(),
    );

    let mut best: Option<(&ModelEntry, usize)> = None;
    for projector in &projectors {
        let common = naming::common_prefix_len(&name, &naming::normalize(&projector.name));
        if best.is_none_or(|(_, length)| common > length) {
            best = Some((projector, common));
        }
    }

    match best {
        Some((projector, length)) if length > 0 => Some(projector.path.clone()),
        _ if projectors.len() == 1 => Some(projectors[0].path.clone()),
        _ => None,
    }
}

/// Delete a model file, typically from the app's models folder.
pub fn delete_file(path: &Path) -> AppResult<()> {
    std::fs::remove_file(path).map_err(|e| AppError::msg(format!("Could not remove model file: {e}")))
}

/// A readable name from the file name, e.g. `qwen2.5-7b-instruct-q4_k_m`.
pub fn label(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(naming::label)
        .unwrap_or_else(|| "unknown model".to_string())
}

/// The GGUF chat should load: the user's pick, else the first one installed.
pub fn selected(dirs: &AppDirs, settings: &Settings) -> Option<PathBuf> {
    if let Some(path) = &settings.selected_model {
        if path.is_file() && !is_projector(path) {
            return Some(path.clone());
        }
    }
    installed(dirs, &settings.model_folders)
        .into_iter()
        .next()
        .map(|entry| entry.path)
}

pub fn selected_label(dirs: &AppDirs, settings: &Settings) -> String {
    match selected(dirs, settings) {
        Some(path) => label(&path),
        None => "No model installed".to_string(),
    }
}

/// True when the path points at a multimodal projector rather than weights.
pub fn is_projector(path: &Path) -> bool {
    let file_name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
    if file_name_looks_like_projector(file_name) {
        return true;
    }
    read_meta(path).is_projector
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("LocalAIse-registry-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_file(path: &Path, contents: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn the_scan_walks_sub_folders() {
        let root = temp_dir("nested");
        write_file(&root.join("llm/granite-4.2-3b/granite-4.2-3b-q4_k_m.gguf"), b"weights");
        write_file(&root.join("llm/qwen3-8b/qwen3-8b-q4_k_m.gguf"), b"weights");

        let entries = scan_root(&root, false);
        assert_eq!(entries.len(), 2);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_projector_is_grouped_with_its_model() {
        let root = temp_dir("projector");
        write_file(
            &root.join("llm/granite-4.2-3b/granite-4.2-3b-q4_k_m.gguf"),
            b"weights",
        );
        write_file(
            &root.join("llm/granite-4.2-3b/mmproj-granite-4.2-3b-f16.gguf"),
            b"projector",
        );

        let groups = scan_groups(&root, false);
        assert_eq!(groups.len(), 1);
        let group = &groups[0];
        assert_eq!(group.models.len(), 1);
        assert_eq!(group.projectors.len(), 1);
        assert_eq!(group.name, "granite 4.2 3b");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_flat_folder_becomes_one_group_per_file() {
        let root = temp_dir("flat");
        write_file(&root.join("alpha-3b.gguf"), b"weights");
        write_file(&root.join("beta-8b.gguf"), b"weights");

        let groups = scan_groups(&root, false);
        assert_eq!(groups.len(), 2);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_projector_beside_a_model_is_found() {
        let root = temp_dir("sibling");
        let model = root.join("llm/granite-4.2-3b/granite-4.2-3b-q4_k_m.gguf");
        let projector = root.join("llm/granite-4.2-3b/mmproj-granite-4.2-3b-f16.gguf");
        write_file(&model, b"weights");
        write_file(&projector, b"projector");

        assert_eq!(projector_for(&model), Some(projector));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_model_without_a_projector_has_none() {
        let root = temp_dir("no-projector");
        let model = root.join("llm/plain/plain.gguf");
        write_file(&model, b"weights");

        assert_eq!(projector_for(&model), None);

        let _ = std::fs::remove_dir_all(&root);
    }
}
