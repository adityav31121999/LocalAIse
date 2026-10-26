use std::path::PathBuf;
use localaise_lib::ingest::pipeline::ingest_file;
use localaise_lib::index::db::IndexDb;

fn main() -> anyhow::Result<()> {
    let project_root = PathBuf::from(r"C:\Users\AaVi3\OneDrive\Documents\testProj");
    let index_db = IndexDb::new(&project_root);

    // Scan for source files in data/ and root
    let data_dir = project_root.join("data");
    let mut sources: Vec<String> = Vec::new();

    // README.md
    if project_root.join("README.md").exists() {
        sources.push("README.md".to_string());
    }

    // PDFs in data/
    if data_dir.exists() {
        for entry in std::fs::read_dir(&data_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "pdf") {
                if let Some(name) = path.file_name() {
                    sources.push(format!("data/{}", name.to_string_lossy()));
                }
            }
        }
    }

    println!("Found {} sources", sources.len());

    for source in &sources {
        let rel_path = PathBuf::from(source);
        println!("Ingesting: {}", source);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ingest_file(&project_root, &rel_path)
        }));
        match result {
            Ok(Ok(chunks)) => {
                println!("  -> {} chunks", chunks.len());
                for chunk in &chunks {
                    println!("     chunk {}: {} chars", chunk.chunk_index, chunk.content.len());
                    println!("     preview: {}", &chunk.content[..chunk.content.len().min(200)]);
                }
                index_db.insert_chunks(source, "now", chunks).map_err(|e| anyhow::anyhow!(e))?;
            }
            Ok(Err(e)) => {
                eprintln!("  ERROR: {}", e);
            }
            Err(_) => {
                eprintln!("  PANIC during ingestion (likely pdf-extract bug), skipping");
            }
        }
    }

    println!("Re-index complete!");
    Ok(())
}