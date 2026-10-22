//! Sandboxing: file tools must refuse to touch anything outside the project
//! folder. These are the tests where a bug would directly violate the privacy
//! promise, so they run against a real temp directory.

use std::path::Path;

use localaise_lib::sandbox::{confirm, paths};
use localaise_lib::tools::files;

fn temp_project(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("LocalAIse-sandbox-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("invoices")).unwrap();
    std::fs::write(dir.join("notes.md"), "project notes").unwrap();
    std::fs::write(dir.join("invoices/jan.csv"), "amount,day\n120,1\n").unwrap();
    dir
}

#[test]
fn parent_escape_is_detected() {
    assert!(paths::has_parent_escape(Path::new("../secrets.txt")));
    assert!(paths::has_parent_escape(Path::new("a/b/../../out.txt")));
    assert!(!paths::has_parent_escape(Path::new("invoice/jan.csv")));
}

#[test]
fn ensure_within_accepts_paths_inside_the_root() {
    let root = temp_project("within");
    let inner = root.join("notes.md");
    let resolved = paths::ensure_within(&root, &inner).expect("project file must stay inside");
    // Compare against the raw root: canonicalize() rewrites prefixes on
    // Windows, and that is exactly the trap this function has to avoid
    // creating for its callers.
    assert!(resolved.starts_with(&root));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn ensure_within_refuses_climbing_out() {
    let root = temp_project("climb");
    let outside = root.join("../outside.txt");
    assert!(paths::ensure_within(&root, &outside).is_err());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn read_file_stays_inside_the_project() {
    let root = temp_project("read");

    let forbidden = files::read_file(&root, "../../../Windows/win.ini");
    assert!(forbidden.error.is_some(), "traversal reads must fail");

    let okay = files::read_file(&root, "invoices/jan.csv");
    assert!(okay.output.contains("amount,day"));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn agent_writes_land_in_outputs_only() {
    let root = temp_project("write");

    // A write to the project root is redirected into outputs/.
    let result = files::write_file(&root, "summary.md", "# Summary\n");
    assert!(result.error.is_none());
    assert!(root.join("outputs/summary.md").is_file());
    assert!(!root.join("summary.md").exists());

    // And traversal attempts are refused outright.
    let escaped = files::write_file(&root, "../bomb.txt", "nope");
    assert!(escaped.error.is_some());
    assert!(!root.parent().unwrap().join("bomb.txt").exists());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn list_files_refuses_subdirs_that_escape() {
    let root = temp_project("list");
    let result = files::list_files(&root, "../../");
    assert!(result.error.is_some());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn destructive_actions_need_confirmation() {
    assert!(confirm::check(&confirm::ActionKind::Read, "read notes", false).is_ok());
    assert!(confirm::check(&confirm::ActionKind::Create, "new file", false).is_ok());

    let refused = confirm::check(&confirm::ActionKind::Delete, "remove invoice.csv", false);
    assert!(refused.is_err());
    assert!(confirm::check(&confirm::ActionKind::Delete, "remove invoice.csv", true).is_ok());

    let modify = confirm::check(&confirm::ActionKind::Modify, "rewrite draft", false);
    assert!(modify.is_err());
    assert!(confirm::check(&confirm::ActionKind::Modify, "rewrite draft", true).is_ok());
}
