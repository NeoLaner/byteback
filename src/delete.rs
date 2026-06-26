//! Removing the selected directories — to the system trash by default, or
//! permanently when the user opts in. One failure never aborts the batch.

use std::fs;
use std::path::{Path, PathBuf};

use crate::report::FoundDir;

/// How a deletion is carried out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Disposal {
    /// Move to the OS trash/recycle bin so it can be restored.
    #[default]
    Trash,
    /// Remove for good with no recovery.
    Permanent,
}

impl Disposal {
    pub fn label(self) -> &'static str {
        match self {
            Disposal::Trash => "trash",
            Disposal::Permanent => "permanent",
        }
    }

    /// Dispose of `dirs`, accumulating freed space and per-dir failures. `root`
    /// guards against ever removing the scan root or any ancestor of it.
    pub fn dispose(self, dirs: &[&FoundDir], root: &Path) -> Outcome {
        let mut outcome = Outcome::default();
        for dir in dirs {
            if guards_root(&dir.path, root) {
                outcome.fail(&dir.path, "refusing to delete the scan root");
                continue;
            }
            match self.remove(&dir.path) {
                Ok(()) => outcome.succeed(dir.size),
                Err(message) => outcome.fail(&dir.path, &message),
            }
        }
        outcome
    }

    fn remove(self, path: &Path) -> Result<(), String> {
        match self {
            Disposal::Trash => trash::delete(path).map_err(|e| e.to_string()),
            Disposal::Permanent => fs::remove_dir_all(path).map_err(|e| e.to_string()),
        }
    }
}

/// What happened during a [`Disposal::dispose`] run.
#[derive(Debug, Default)]
pub struct Outcome {
    pub freed_bytes: u64,
    pub removed: usize,
    pub failures: Vec<(PathBuf, String)>,
}

impl Outcome {
    fn succeed(&mut self, size: u64) {
        self.freed_bytes += size;
        self.removed += 1;
    }

    fn fail(&mut self, path: &Path, message: &str) {
        self.failures.push((path.to_path_buf(), message.to_owned()));
    }
}

/// True if removing `path` would also remove the scan root (it equals the root
/// or is one of its ancestors).
fn guards_root(path: &Path, root: &Path) -> bool {
    root.starts_with(path)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::target::TargetName;

    fn found(path: PathBuf) -> FoundDir {
        FoundDir {
            path,
            target: TargetName::from("node_modules"),
            size: 42,
            file_count: 1,
            selected: true,
        }
    }

    #[test]
    fn permanent_removes_dirs_and_reports_freed_space() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("pkg/node_modules");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("a.js"), b"hello").unwrap();

        let dir = found(target.clone());
        let outcome = Disposal::Permanent.dispose(&[&dir], root.path());

        assert_eq!(outcome.removed, 1);
        assert_eq!(outcome.freed_bytes, 42);
        assert!(outcome.failures.is_empty());
        assert!(!target.exists());
    }

    #[test]
    fn refuses_to_delete_the_scan_root() {
        let root = tempfile::tempdir().unwrap();
        let dir = found(root.path().to_path_buf());

        let outcome = Disposal::Permanent.dispose(&[&dir], root.path());

        assert_eq!(outcome.removed, 0);
        assert_eq!(outcome.failures.len(), 1);
        assert!(root.path().exists());
    }

    #[test]
    fn one_failure_does_not_abort_the_batch() {
        let root = tempfile::tempdir().unwrap();
        let missing = found(root.path().join("does-not-exist"));
        let real = root.path().join("real/node_modules");
        fs::create_dir_all(&real).unwrap();
        let real = found(real);

        let outcome = Disposal::Permanent.dispose(&[&missing, &real], root.path());

        assert_eq!(outcome.removed, 1);
        assert_eq!(outcome.failures.len(), 1);
    }
}
