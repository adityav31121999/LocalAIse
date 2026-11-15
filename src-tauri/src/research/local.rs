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
            let ok_ext = p.extension().and_then(|x| x.to_str()).is_some_and(|x| EXTS.contains(&x));
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

/// Common English stop words filtered from ranking terms.
const STOP_WORDS: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "from", "are", "was", "were", "have", "has",
    "had", "not", "but", "you", "your", "they", "them", "their", "there", "what", "when",
    "where", "which", "who", "whom", "how", "why", "does", "did", "done", "can", "could",
    "should", "would", "will", "into", "about", "than", "then", "them", "these", "those",
    "been", "being", "its", "it's", "our", "ours", "his", "her", "hers", "she", "him", "his",
    "any", "all", "each", "every", "some", "such", "only", "also", "more", "most", "other",
    "over", "under", "between", "through", "before", "after", "while", "because",
];

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !STOP_WORDS.contains(w))
        .map(|w| w.to_string())
        .collect()
}

/// Okapi BM25 ranking of the in-memory corpus against the query terms.
/// Returns the top `top` documents with a positive score.
pub fn rank<'a>(docs: &'a [LocalDoc], subs: &[String], top: usize) -> Vec<&'a LocalDoc> {
    const K1: f32 = 1.5;
    const B: f32 = 0.75;

    let query_terms: Vec<String> = subs.iter().flat_map(|s| tokenize(s)).collect();
    if query_terms.is_empty() {
        // Nothing but stop words: matching everything would rank garbage.
        return Vec::new();
    }

    // Tokenize each document once and accumulate stats for IDF.
    let doc_tokens: Vec<Vec<String>> = docs.iter().map(|d| tokenize(&d.text)).collect();
    let n_docs = docs.len().max(1) as f32;
    let avg_len = (doc_tokens.iter().map(|t| t.len()).sum::<usize>() as f32 / n_docs).max(1.0);

    let mut document_frequency: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for tokens in &doc_tokens {
        let mut seen = std::collections::HashSet::new();
        for t in tokens {
            if seen.insert(t.as_str()) {
                *document_frequency.entry(t.as_str()).or_default() += 1;
            }
        }
    }

    // Unique query terms with IDF.
    let mut unique_terms: Vec<&str> = query_terms.iter().map(|t| t.as_str()).collect();
    unique_terms.sort_unstable();
    unique_terms.dedup();
    let idfs: Vec<f32> = unique_terms.iter().map(|t| {
        let df = document_frequency.get(t).copied().unwrap_or(0) as f32;
        (1.0 + (n_docs - df + 0.5) / (df + 0.5)).ln()
    }).collect();

    let mut scored: Vec<(f32, &LocalDoc)> = docs.iter().zip(&doc_tokens).map(|(doc, tokens)| {
        let len = tokens.len().max(1) as f32;
        let mut tf: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for t in tokens {
            *tf.entry(t.as_str()).or_default() += 1;
        }
        let score: f32 = unique_terms.iter().zip(&idfs).map(|(t, idf)| {
            let f = tf.get(t).copied().unwrap_or(0) as f32;
            idf * (f * (K1 + 1.0)) / (f + K1 * (1.0 - B + B * len / avg_len))
        }).sum();
        (score, doc)
    }).filter(|(s, _)| *s > 0.0).collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().take(top).map(|(_, d)| d).collect()
}
#[cfg(test)]
mod tests {
    use super::*;

    fn doc(name: &str, text: &str) -> LocalDoc {
        LocalDoc { path: PathBuf::from(name), title: name.to_string(), text: text.to_string() }
    }

    #[test]
    fn rank_prefers_documents_with_more_query_term_coverage() {
        let docs = vec![
            doc("a.md", "cats cats cats dogs"),
            doc("b.md", "cats dogs and birds plus more about cats and veterinary care for cats"),
            doc("c.md", "unrelated cooking recipes for pasta and sauce"),
        ];
        let ranked = rank(&docs, &["cats veterinary care".to_string()], 2);
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].title, "b.md");
        assert!(!ranked.iter().any(|d| d.title == "c.md"));
    }

    #[test]
    fn rank_ignores_stop_words() {
        let docs = vec![
            doc("a.md", "the and for with that this"),
            doc("b.md", "quasar telescope observations"),
        ];
        let ranked = rank(&docs, &["the and with".to_string()], 5);
        assert!(ranked.is_empty(), "stop-word-only queries should match nothing");
    }

    #[test]
    fn load_corpus_skips_hidden_and_chat_dirs() {
        let dir = std::env::temp_dir().join(format!("LocalAIse-local-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".hidden")).unwrap();
        std::fs::create_dir_all(dir.join("chats")).unwrap();
        std::fs::write(dir.join("keep.md"), "visible content").unwrap();
        std::fs::write(dir.join(".hidden").join("skip.md"), "hidden content").unwrap();
        std::fs::write(dir.join("chats").join("skip.md"), "chat content").unwrap();
        std::fs::write(dir.join("notext.bin"), [0u8; 16]).unwrap();

        let docs = load_corpus(&dir).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].title, "keep.md");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
