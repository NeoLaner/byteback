//! byteback — a TUI that reclaims disk space by sweeping regenerable build and
//! dependency directories (node_modules, .next, target, ...).

mod app;
mod cli;
mod config;
mod delete;
mod report;
mod scan;
mod target;

use anyhow::{Context, Result};
use clap::Parser;

use app::App;
use cli::Cli;
use config::Config;
use report::human_size;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = cli.root().context("resolving the scan directory")?;
    if !root.is_dir() {
        anyhow::bail!("{} is not a directory", root.display());
    }

    let config = Config::load().context("loading config")?;

    let mut terminal =
        ratatui::try_init().context("byteback needs an interactive terminal to run")?;
    let outcome = App::new(config, root, cli.disposal()).run(&mut terminal);
    let _ = ratatui::try_restore();

    // Print a one-line summary on normal stdout, after leaving the alt-screen.
    if let Some(outcome) = outcome? {
        println!(
            "Reclaimed {} from {} director{}.",
            human_size(outcome.freed_bytes),
            outcome.removed,
            if outcome.removed == 1 { "y" } else { "ies" },
        );
        for (path, error) in &outcome.failures {
            eprintln!("could not remove {}: {error}", path.display());
        }
    }

    Ok(())
}
