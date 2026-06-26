//! Command-line arguments. The tool is a TUI, so this is intentionally small:
//! an optional directory and a switch for the default disposal mode.

use std::path::PathBuf;

use clap::Parser;

use crate::delete::Disposal;

/// byteback — reclaim disk space by sweeping regenerable build directories
/// (node_modules, .next, target, ...).
#[derive(Debug, Parser)]
#[command(name = "byteback", version, about)]
pub struct Cli {
    /// Directory to scan. Defaults to the current directory.
    pub path: Option<PathBuf>,

    /// Delete permanently instead of moving to the trash.
    #[arg(short, long)]
    pub permanent: bool,
}

impl Cli {
    /// The scan root: the given path, or the current working directory.
    pub fn root(&self) -> std::io::Result<PathBuf> {
        match &self.path {
            Some(path) => Ok(path.clone()),
            None => std::env::current_dir(),
        }
    }

    /// The disposal mode to start in.
    pub fn disposal(&self) -> Disposal {
        if self.permanent {
            Disposal::Permanent
        } else {
            Disposal::Trash
        }
    }
}
