//! Paths in the form a user would type them.

use std::path::{Path, PathBuf};

/// Canonicalizes a path and keeps the everyday spelling.
///
/// `std::fs::canonicalize()` answers with a Windows verbatim path
/// (`\\?\C:\Users\…` on drives, `\\?\UNC\server\share\…` on network shares).
/// Those forms work on the file system but look wrong wherever a path is
/// displayed or saved, so this is the variant to use when a path leaves the
/// process: the project registry, the settings and everything the UI shows.
pub fn plain_canonicalize(path: &Path) -> std::io::Result<PathBuf> {
    Ok(plain(path.canonicalize()?))
}

/// Rewrites a verbatim Windows path back to what the user typed.
///
/// `\\?\UNC\server\share` becomes `\\server\share`; `\\?\C:\Users` becomes
/// `C:\Users`. A path without the verbatim prefix passes through unchanged.
pub fn plain(path: PathBuf) -> PathBuf {
    let text = match path.to_str() {
        Some(text) => text,
        None => return path,
    };
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_paths_lose_the_verbatim_prefix() {
        let path = plain(PathBuf::from(r"\\?\C:\Users\me\docs"));
        assert_eq!(path, PathBuf::from(r"C:\Users\me\docs"));
    }

    #[test]
    fn network_paths_keep_their_server_prefix() {
        let path = plain(PathBuf::from(r"\\?\UNC\server\share\docs"));
        assert_eq!(path, PathBuf::from(r"\\server\share\docs"));
    }

    #[test]
    fn ordinary_paths_are_untouched() {
        let original = PathBuf::from(r"C:\Users\me\docs");
        assert_eq!(plain(original.clone()), original);
    }
}
