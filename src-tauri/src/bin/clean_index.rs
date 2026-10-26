use std::path::PathBuf;
use localaise_lib::index::db::IndexDb;

fn main() -> anyhow::Result<()> {
    let project_root = PathBuf::from(r"C:\Users\AaVi3\OneDrive\Documents\testProj");
    let index_db = IndexDb::new(&project_root);
    
    let mut index = index_db.load();
    let before = index.chunks.len();
    
    // Remove the garbled Linear Attention paper
    index.chunks.retain(|c| c.file_name != "data/2006.16236v3.pdf");
    
    let after = index.chunks.len();
    println!("Removed {} chunks from Linear Attention paper", before - after);
    println!("Index now has {} chunks", after);
    
    index_db.save(&index).map_err(|e| anyhow::anyhow!(e))?;
    println!("Index saved!");
    Ok(())
}