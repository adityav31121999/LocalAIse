use std::path::PathBuf;
use localaise_lib::index::db::IndexDb;

fn main() -> anyhow::Result<()> {
    let project_root = PathBuf::from(r"C:\Users\AaVi3\OneDrive\Documents\testProj");
    let index_db = IndexDb::new(&project_root);
    let index = index_db.load();
    
    // Search for the problematic terms
    let search_terms = [
        "sqrt",
        "√",
        "epsilon",
        "ε",
        "ls = 0.1",
        "0.1",
        "self-attention is not",
        "self-attention not",
        "not necessary",
    ];
    
    for term in search_terms {
        println!("\n=== Searching for: '{}' ===", term);
        for chunk in &index.chunks {
            if chunk.content.to_lowercase().contains(&term.to_lowercase()) {
                println!("  FOUND in {}: ...{}...", chunk.file_name, 
                    &chunk.content[..chunk.content.len().min(300)]);
            }
        }
    }
    
    Ok(())
}