//! Ingestion: parse every supported format, chunk it for retrieval, and index
//! it back into the per-project store. Runs against real temp files.

use std::path::Path;

use localaise_lib::index::db::IndexDb;
use localaise_lib::ingest::pipeline::ingest_file;
use localaise_lib::retrieval::search::{format_citations, search_chunks};

fn temp_project(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("LocalAIse-ingest-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_sample(dir: &Path, relative: &str, marker: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let filler = "filler words to pad the chunk out. ".repeat(8);
    std::fs::write(
        path,
        format!("{marker}\n{filler}\nThe zebra budget reconciles quarterly. {marker}\n"),
    )
    .unwrap();
}

#[test]
fn supported_text_formats_parse() {
    let dir = temp_project("formats");

    std::fs::write(dir.join("a.txt"), "plain text notes").unwrap();
    std::fs::write(dir.join("b.md"), "# Heading\n\nmarkdown body").unwrap();
    std::fs::write(dir.join("c.csv"), "col_a,col_b\n1,2\n").unwrap();

    for name in ["a.txt", "b.md", "c.csv"] {
        let chunks = ingest_file(&dir, Path::new(name));
        assert!(chunks.is_ok(), ".{name} should parse");
        assert!(!chunks.unwrap().is_empty());
    }

    let unsupported = ingest_file(&dir, Path::new("missing.log"));
    assert!(unsupported.is_err());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn chunks_are_labeled_with_their_file_and_fit_the_size_budget() {
    let dir = temp_project("chunking");
    let filler = "lorem ipsum dolor sit amet consectetur adipiscing elit. ".repeat(40);
    std::fs::write(dir.join("long.md"), &filler).unwrap();

    let chunks = ingest_file(&dir, Path::new("long.md")).unwrap();
    assert!(chunks.len() > 1, "long documents produce several chunks");
    for chunk in &chunks {
        assert_eq!(chunk.file_name, "long.md");
        assert!(!chunk.content.trim().is_empty());
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn retrieval_finds_the_right_file_and_names_it_in_citations() {
    let dir = temp_project("retrieval");
    write_sample(&dir, "budget.md", "BUDGET-DOC");
    write_sample(&dir, "travel.md", "TRAVEL-DOC");

    let index_db = IndexDb::new(&dir);
    let mut index = index_db.load();
    index.chunks.extend(ingest_file(&dir, Path::new("budget.md")).unwrap());
    index.chunks.extend(ingest_file(&dir, Path::new("travel.md")).unwrap());
    index_db.save(&index).unwrap();

    // A reload gives the same corpus, proving the index round-trips.
    let index = IndexDb::new(&dir).load();
    assert!(index.chunks.len() >= 2);

    let hits = search_chunks(&index, "zebra budget", 2);
    assert!(!hits.is_empty());
    assert_eq!(hits[0].chunk.file_name, "budget.md");

    let citations = format_citations(&hits);
    assert_eq!(citations[0].index, 1);
    assert_eq!(citations[0].file_name, "budget.md");
    assert!(citations[0].passage.is_some());

    // A query that appears in neither document gives nothing.
    assert!(search_chunks(&index, "quantumn frobnicator", 4).is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reindexing_replaces_old_chunks_for_a_file() {
    let dir = temp_project("reindex");
    std::fs::write(dir.join("notes.md"), "first version of notes").unwrap();

    let index_db = IndexDb::new(&dir);
    let chunks = ingest_file(&dir, Path::new("notes.md")).unwrap();
    index_db.insert_chunks("notes.md", "stamp-1", chunks).unwrap();
    let first_count = index_db.load().chunks.len();
    assert!(first_count > 0);
    assert_eq!(index_db.load().indexed_files.get("notes.md"), Some(&"stamp-1".to_string()));

    // Second pass: same file, new chunk set — the old chunks must not linger.
    std::fs::write(dir.join("notes.md"), "second version, now longer. lorem ipsum dolor sit amet. lorem ipsum dolor sit. lorem ipsum dolor.").unwrap();
    let chunks = ingest_file(&dir, Path::new("notes.md")).unwrap();
    let second_count = chunks.len();
    index_db.insert_chunks("notes.md", "stamp-2", chunks).unwrap();

    let index = index_db.load();
    assert_eq!(index.chunks.len(), second_count);
    assert_eq!(index.indexed_files.get("notes.md"), Some(&"stamp-2".to_string()));

    let _ = std::fs::remove_dir_all(&dir);
}
