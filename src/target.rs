//! The directory names `byteback` knows how to sweep.

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};

/// A directory name worth reclaiming, e.g. `node_modules` or `.next`.
///
/// A newtype rather than a bare `String` so signatures read as intent
/// (`config.hide(name)`, `scan(root, &targets)`) instead of "some string".
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TargetName(String);

impl TargetName {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// True if this name is one of the built-in [`DEFAULT_TARGETS`].
    pub fn is_default(&self) -> bool {
        DEFAULT_TARGETS.contains(&self.0.as_str())
    }
}

impl fmt::Display for TargetName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for TargetName {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl From<String> for TargetName {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// Built-in names offered out of the box. Users hide any of these and add their
/// own through [`crate::config::Config`]; the two are merged into the list shown
/// in the dashboard.
pub const DEFAULT_TARGETS: &[&str] = &[
    "node_modules",
    "dist",
    "build",
    ".next",
    ".turbo",
    "target",
    ".cache",
    ".parcel-cache",
    ".svelte-kit",
    ".nuxt",
    "coverage",
    "__pycache__",
];

/// The built-in defaults as owned [`TargetName`]s, in declared order.
pub fn default_targets() -> Vec<TargetName> {
    DEFAULT_TARGETS
        .iter()
        .copied()
        .map(TargetName::from)
        .collect()
}

/// A fast lookup set of the enabled names, for matching directory names during a
/// scan.
pub fn lookup(targets: &[TargetName]) -> HashSet<String> {
    targets.iter().map(|t| t.0.clone()).collect()
}
