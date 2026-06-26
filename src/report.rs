//! The result of a scan: the directories found, grouped into categories with
//! size and file-count totals. This is the model the dashboard renders.

use std::path::PathBuf;

use humansize::{DECIMAL, format_size};

use crate::target::TargetName;

/// One reclaimable directory found on disk.
#[derive(Clone, Debug)]
pub struct FoundDir {
    pub path: PathBuf,
    pub target: TargetName,
    pub size: u64,
    pub file_count: u64,
    /// Whether the user has it marked for deletion. Found dirs start selected.
    pub selected: bool,
}

/// A run of contiguous [`FoundDir`]s sharing a target name, addressed as a slice
/// of [`ScanReport::dirs`]. Keeping the dirs in one flat vec (rather than nested)
/// keeps cursor handling and toggling in the UI simple.
#[derive(Clone, Debug)]
pub struct Category {
    pub target: TargetName,
    pub start: usize,
    pub len: usize,
}

/// All directories found in a scan, in display order: categories sorted by total
/// size (largest first), and dirs within a category likewise.
#[derive(Clone, Debug, Default)]
pub struct ScanReport {
    pub dirs: Vec<FoundDir>,
    pub categories: Vec<Category>,
}

impl ScanReport {
    /// Group loose dirs into the ordered, flattened report.
    pub fn from_dirs(dirs: Vec<FoundDir>) -> Self {
        // Bucket by target, preserving each bucket so we can sort independently.
        let mut buckets: Vec<(TargetName, Vec<FoundDir>)> = Vec::new();
        for dir in dirs {
            match buckets.iter_mut().find(|(name, _)| *name == dir.target) {
                Some((_, bucket)) => bucket.push(dir),
                None => buckets.push((dir.target.clone(), vec![dir])),
            }
        }

        for (_, bucket) in &mut buckets {
            bucket.sort_by_key(|dir| std::cmp::Reverse(dir.size));
        }
        buckets.sort_by_key(|(_, bucket)| std::cmp::Reverse(total_size(bucket)));

        let mut flat = Vec::new();
        let mut categories = Vec::new();
        for (target, bucket) in buckets {
            categories.push(Category {
                target,
                start: flat.len(),
                len: bucket.len(),
            });
            flat.extend(bucket);
        }

        Self {
            dirs: flat,
            categories,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.dirs.is_empty()
    }

    pub fn dir_count(&self) -> usize {
        self.dirs.len()
    }

    pub fn total_size(&self) -> u64 {
        self.dirs.iter().map(|d| d.size).sum()
    }

    pub fn total_files(&self) -> u64 {
        self.dirs.iter().map(|d| d.file_count).sum()
    }

    /// Dirs the user still has marked for deletion.
    pub fn selected(&self) -> Vec<&FoundDir> {
        self.dirs.iter().filter(|d| d.selected).collect()
    }

    pub fn selected_count(&self) -> usize {
        self.dirs.iter().filter(|d| d.selected).count()
    }

    pub fn selected_size(&self) -> u64 {
        self.dirs
            .iter()
            .filter(|d| d.selected)
            .map(|d| d.size)
            .sum()
    }

    /// Flip the selection of the dir at a flat index. Out-of-range is ignored.
    pub fn toggle(&mut self, index: usize) {
        if let Some(dir) = self.dirs.get_mut(index) {
            dir.selected = !dir.selected;
        }
    }

    /// Select or deselect every dir at once.
    pub fn set_all(&mut self, selected: bool) {
        for dir in &mut self.dirs {
            dir.selected = selected;
        }
    }

    /// The dirs of one category, in display order.
    pub fn category_dirs(&self, category: &Category) -> &[FoundDir] {
        &self.dirs[category.start..category.start + category.len]
    }
}

fn total_size(dirs: &[FoundDir]) -> u64 {
    dirs.iter().map(|d| d.size).sum()
}

/// Format a byte count for display, e.g. `1.23 GB`.
pub fn human_size(bytes: u64) -> String {
    format_size(bytes, DECIMAL)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(target: &str, size: u64, files: u64) -> FoundDir {
        FoundDir {
            path: PathBuf::from(format!("/tmp/{target}/{size}")),
            target: TargetName::from(target),
            size,
            file_count: files,
            selected: true,
        }
    }

    #[test]
    fn groups_by_category_largest_first() {
        let report = ScanReport::from_dirs(vec![
            dir("node_modules", 100, 10),
            dir(".next", 500, 5),
            dir("node_modules", 50, 4),
        ]);

        // .next category (500) outweighs node_modules (150), so it comes first.
        assert_eq!(report.categories[0].target, TargetName::from(".next"));
        assert_eq!(
            report.categories[1].target,
            TargetName::from("node_modules")
        );
        // node_modules dirs sorted by size desc within the category.
        let nm = &report.categories[1];
        assert_eq!(report.category_dirs(nm)[0].size, 100);
        assert_eq!(report.category_dirs(nm)[1].size, 50);

        assert_eq!(report.total_size(), 650);
        assert_eq!(report.total_files(), 19);
        assert_eq!(report.dir_count(), 3);
    }

    #[test]
    fn selection_totals_track_toggles() {
        let mut report = ScanReport::from_dirs(vec![dir("dist", 200, 2), dir("dist", 300, 3)]);
        assert_eq!(report.selected_count(), 2);
        assert_eq!(report.selected_size(), 500);

        report.toggle(0);
        assert_eq!(report.selected_count(), 1);

        report.set_all(false);
        assert_eq!(report.selected_count(), 0);
        report.set_all(true);
        assert_eq!(report.selected_size(), 500);
    }
}
