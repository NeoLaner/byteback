//! Finding and measuring the target directories under a root.
//!
//! Two passes, both parallel via `jwalk` (rayon-backed):
//! 1. **Discover** — walk the tree but prune at every match, so we never descend
//!    into a `node_modules` looking for more targets. This makes the discovery
//!    walk proportional to the *project* tree, not its dependencies.
//! 2. **Measure** — sum file sizes and counts inside each found dir, in parallel
//!    across dirs. Metadata only; file contents are never read.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use jwalk::WalkDir;
use rayon::prelude::*;

use crate::report::{FoundDir, ScanReport};
use crate::target::{self, TargetName};

/// Progress streamed from a background scan so the dashboard stays responsive.
pub enum ScanEvent {
    /// Discovery finished; this many target dirs will be measured.
    Found(usize),
    /// One more directory finished measuring (for the progress counter).
    Measured,
    /// Everything is measured and grouped.
    Done(ScanReport),
}

/// Start a scan on a background thread, returning a receiver of progress events.
pub fn spawn(root: PathBuf, targets: Vec<TargetName>) -> Receiver<ScanEvent> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let lookup = target::lookup(&targets);
        let found = discover(&root, &lookup);
        let _ = tx.send(ScanEvent::Found(found.len()));

        let dirs: Vec<FoundDir> = found
            .into_par_iter()
            .map_with(tx.clone(), |tx, (path, target)| {
                let dir = measure(path, target);
                let _ = tx.send(ScanEvent::Measured);
                dir
            })
            .collect();

        let _ = tx.send(ScanEvent::Done(ScanReport::from_dirs(dirs)));
    });
    rx
}

/// Run a full scan synchronously and return the grouped report. The
/// non-interactive core, exercised by the tests below; reserved for a future
/// scriptable mode.
#[allow(dead_code)]
pub fn scan_blocking(root: &Path, targets: &[TargetName]) -> ScanReport {
    let lookup = target::lookup(targets);
    let dirs = discover(root, &lookup)
        .into_par_iter()
        .map(|(path, target)| measure(path, target))
        .collect();
    ScanReport::from_dirs(dirs)
}

/// Find every directory under `root` whose name is a target, without descending
/// into matched dirs. The root itself is never returned.
fn discover(root: &Path, targets: &HashSet<String>) -> Vec<(PathBuf, TargetName)> {
    let prune = Arc::new(targets.clone());
    let walker = WalkDir::new(root)
        .skip_hidden(false) // we want .next, .turbo, .cache, ...
        .follow_links(false)
        .process_read_dir(move |_depth, _path, _state, children| {
            for child in children.iter_mut().flatten() {
                let is_target_dir = child.file_type().is_dir()
                    && child
                        .file_name()
                        .to_str()
                        .is_some_and(|name| prune.contains(name));
                if is_target_dir {
                    // Record it (the entry is still yielded), but don't recurse in.
                    child.read_children_path = None;
                }
            }
        });

    let mut found = Vec::new();
    for entry in walker.into_iter().flatten() {
        if !entry.file_type().is_dir() {
            continue;
        }
        let path = entry.path();
        if path == root {
            continue;
        }
        if let Some(name) = entry.file_name().to_str()
            && targets.contains(name)
        {
            found.push((path, TargetName::from(name)));
        }
    }
    found
}

/// Sum the apparent size and file count of everything under `path`.
fn measure(path: PathBuf, target: TargetName) -> FoundDir {
    let mut size = 0u64;
    let mut file_count = 0u64;
    for entry in WalkDir::new(&path)
        .skip_hidden(false)
        .follow_links(false)
        .into_iter()
        .flatten()
    {
        if entry.file_type().is_file()
            && let Ok(meta) = entry.metadata()
        {
            size += meta.len();
            file_count += 1;
        }
    }
    FoundDir {
        path,
        target,
        size,
        file_count,
        selected: true,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![b'x'; bytes]).unwrap();
    }

    #[test]
    fn finds_targets_and_sums_size_and_files() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        write(&root.join("app/node_modules/a.js"), 100);
        write(&root.join("app/node_modules/b.js"), 50);
        write(&root.join("web/.next/cache.bin"), 200);
        write(&root.join("web/src/index.ts"), 10); // not a target, untouched

        let report = scan_blocking(
            root,
            &[TargetName::from("node_modules"), TargetName::from(".next")],
        );

        assert_eq!(report.dir_count(), 2);
        assert_eq!(report.total_files(), 3);
        assert_eq!(report.total_size(), 350);
    }

    #[test]
    fn does_not_descend_into_a_matched_target() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        // A nested node_modules inside another must not be counted separately.
        write(&root.join("node_modules/pkg/node_modules/dep.js"), 100);
        write(&root.join("node_modules/top.js"), 10);

        let report = scan_blocking(root, &[TargetName::from("node_modules")]);

        assert_eq!(
            report.dir_count(),
            1,
            "nested target must be pruned, not re-found"
        );
        assert_eq!(
            report.total_size(),
            110,
            "nested files still counted via the outer dir"
        );
    }

    #[test]
    fn ignores_the_root_even_if_it_matches() {
        let root = tempfile::tempdir().unwrap();
        let nm = root.path().join("node_modules");
        write(&nm.join("a.js"), 10);

        let report = scan_blocking(&nm, &[TargetName::from("node_modules")]);
        assert!(report.is_empty(), "scan root is never offered for deletion");
    }
}
