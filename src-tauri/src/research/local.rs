use std::path::{Path, PathBuf};

pub struct LocalDoc { pub path: PathBuf, pub title: String, pub text: String }
impl LocalDoc { pub fn path_string(&self) -> String { self.path.display().to_string() } }

const EXTS: &[&str] = &["md", "txt", "rs", "ts", "py", "json", "toml", "html"];
const MAX_BYTES: u64 = 2 * 1024 * 1024;

pub fn load_corpus(root: &Path) -> Result<Vec<LocalDoc>, String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "node_modules" || name == "target" || name == "chats" { continue; }
            if p.is_dir() { stack.push(p); continue; }
            let ok_ext = p.extension().and_then(|x| x.to_str()).map_or(false, |x| EXTS.contains(&x));
            let small = e.metadata().map(|m| m.len() <= MAX_BYTES).unwrap_or(false);
            if ok_ext && small {
                if let Ok(text) = std::fs::read_to_string(&p) {
                    out.push(LocalDoc { title: name, path: p, text });
                }
            }
        }
    }
    Ok(out)
}

pub fn rank<'a>(docs: &'a [LocalDoc], subs: &[String], top: usize) -> Vec<&'a LocalDoc> {
    let terms: Vec<String> = subs.iter()
        .flat_map(|s| s.split_whitespace().map(|w| w.to_lowercase()))
        .filter(|w| w.len() > 3).collect();
    let mut scored: Vec<(usize, &LocalDoc)> = docs.iter().map(|d| {
        let lower = d.text.to_lowercase();
        (terms.iter().map(|t| lower.matches(t.as_str()).count()).sum(), d)
    }).filter(|(s, _)| *s > 0).collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().take(top).map(|(_, d)| d).collect()
}