//! Persistent user configuration: which target names exist and which are
//! selected. Stored as TOML under the OS config directory so customisations
//! survive across runs ("save forever").

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::target::{DEFAULT_TARGETS, TargetName, default_targets};

/// User settings, backed by a TOML file (e.g. `~/.config/byteback/config.toml`).
///
/// The visible target list is `(defaults ∪ custom) − hidden`. Mutating methods
/// update the in-memory state; call [`Config::save`] to persist.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    targets: TargetSettings,

    /// Where this config loads from / saves to. Not serialised.
    #[serde(skip)]
    path: Option<PathBuf>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TargetSettings {
    /// Names the user added; kept forever.
    #[serde(default)]
    custom: Vec<TargetName>,
    /// Built-in defaults the user removed; never shown again (until re-added).
    #[serde(default)]
    hidden: Vec<TargetName>,
    /// Names that were checked in the last run; restored on the next launch.
    #[serde(default)]
    enabled: Vec<TargetName>,
}

impl Config {
    /// Load from the standard config path, or return defaults if none exists yet.
    pub fn load() -> Result<Self> {
        let path = config_path()?;
        Self::load_from(path)
    }

    /// Persist to the config path. No-op for an in-memory config (no path).
    pub fn save(&self) -> Result<()> {
        let Some(path) = self.path.as_ref() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating config dir {}", parent.display()))?;
        }
        let text = toml::to_string_pretty(self).context("serialising config")?;
        fs::write(path, text).with_context(|| format!("writing config {}", path.display()))?;
        Ok(())
    }

    /// The names to show in the dashboard: defaults then custom, minus hidden,
    /// de-duplicated, in a stable, human-friendly order.
    pub fn available(&self) -> Vec<TargetName> {
        let hidden: HashSet<&str> = self.targets.hidden.iter().map(TargetName::as_str).collect();
        let mut seen: HashSet<String> = HashSet::new();
        default_targets()
            .into_iter()
            .chain(self.targets.custom.iter().cloned())
            .filter(|name| !hidden.contains(name.as_str()))
            .filter(|name| seen.insert(name.as_str().to_owned()))
            .collect()
    }

    /// The names that should start checked: the remembered selection, or every
    /// available name on first run.
    pub fn initial_enabled(&self) -> HashSet<TargetName> {
        let available = self.available();
        if self.targets.enabled.is_empty() {
            return available.into_iter().collect();
        }
        let available: HashSet<TargetName> = available.into_iter().collect();
        self.targets
            .enabled
            .iter()
            .filter(|name| available.contains(*name))
            .cloned()
            .collect()
    }

    /// Add a user name kept across runs. Re-adding a hidden default just unhides
    /// it; duplicates are ignored.
    pub fn add_custom(&mut self, name: TargetName) {
        self.targets.hidden.retain(|n| n != &name);
        if !name.is_default() && !self.targets.custom.contains(&name) {
            self.targets.custom.push(name);
        }
    }

    /// Remove a name from the visible list. A default is hidden (re-addable); a
    /// custom name is dropped entirely.
    pub fn remove(&mut self, name: &TargetName) {
        self.targets.enabled.retain(|n| n != name);
        if let Some(pos) = self.targets.custom.iter().position(|n| n == name) {
            self.targets.custom.remove(pos);
            return;
        }
        if DEFAULT_TARGETS.contains(&name.as_str()) && !self.targets.hidden.contains(name) {
            self.targets.hidden.push(name.clone());
        }
    }

    /// Remember which names the user has checked for next time.
    pub fn set_enabled(&mut self, enabled: impl IntoIterator<Item = TargetName>) {
        self.targets.enabled = enabled.into_iter().collect();
    }

    fn load_from(path: PathBuf) -> Result<Self> {
        if !path.exists() {
            return Ok(Self {
                path: Some(path),
                ..Default::default()
            });
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("reading config {}", path.display()))?;
        let mut config: Config =
            toml::from_str(&text).with_context(|| format!("parsing config {}", path.display()))?;
        config.path = Some(path);
        Ok(config)
    }
}

fn config_path() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("", "", "byteback")
        .context("could not locate a config directory for this platform")?;
    Ok(dirs.config_dir().join("config.toml"))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn at(path: &Path) -> Config {
        Config {
            path: Some(path.to_path_buf()),
            ..Default::default()
        }
    }

    #[test]
    fn available_is_defaults_plus_custom_minus_hidden() {
        let mut config = Config::default();
        config.add_custom(TargetName::from(".my-cache"));
        config.remove(&TargetName::from("dist"));

        let names: Vec<String> = config.available().iter().map(|n| n.to_string()).collect();
        assert!(names.contains(&"node_modules".to_string()));
        assert!(names.contains(&".my-cache".to_string()));
        assert!(
            !names.contains(&"dist".to_string()),
            "hidden default must be gone"
        );
        // custom names come after defaults
        assert_eq!(names.last().unwrap(), ".my-cache");
    }

    #[test]
    fn adding_a_known_default_does_not_duplicate_into_custom() {
        let mut config = Config::default();
        config.add_custom(TargetName::from("node_modules"));
        let count = config
            .available()
            .iter()
            .filter(|n| n.as_str() == "node_modules")
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn re_adding_a_hidden_default_unhides_it() {
        let mut config = Config::default();
        config.remove(&TargetName::from(".next"));
        assert!(!config.available().contains(&TargetName::from(".next")));
        config.add_custom(TargetName::from(".next"));
        assert!(config.available().contains(&TargetName::from(".next")));
    }

    #[test]
    fn initial_enabled_defaults_to_everything_then_remembers() {
        let mut config = Config::default();
        assert_eq!(config.initial_enabled().len(), config.available().len());

        config.set_enabled([TargetName::from("node_modules")]);
        let enabled = config.initial_enabled();
        assert_eq!(enabled.len(), 1);
        assert!(enabled.contains(&TargetName::from("node_modules")));
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let mut config = at(&path);
        config.add_custom(TargetName::from(".my-cache"));
        config.remove(&TargetName::from("dist"));
        config.set_enabled([TargetName::from("node_modules")]);
        config.save().unwrap();

        let loaded = Config::load_from(path).unwrap();
        assert!(loaded.available().contains(&TargetName::from(".my-cache")));
        assert!(!loaded.available().contains(&TargetName::from("dist")));
        assert!(
            loaded
                .initial_enabled()
                .contains(&TargetName::from("node_modules"))
        );
    }
}
