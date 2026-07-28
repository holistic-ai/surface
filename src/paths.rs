//! Platform directory resolution.
//!
//! | | state | config |
//! |---|---|---|
//! | macOS | `~/Library/Application Support/ai.holistic.surface` | same as state |
//! | Linux | `~/.local/share/surface` | `~/.config/surface` |
//! | Windows | `%LOCALAPPDATA%\holistic\surface\data` | `%APPDATA%\holistic\surface\config` |
//!
//! `surface` writes exactly two things, both to the state directory: the usage
//! ledger (so the next scan reads only what was appended) and the cached model
//! price table. Both are overridable with `SURFACE_{STATE,CONFIG}_DIR`, which is
//! what tests use to stay out of the real profile.

use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;

pub const APP_NAME: &str = "surface";

#[derive(Debug, Clone)]
pub struct Paths {
    pub state_dir: PathBuf,
    pub config_dir: PathBuf,
}

impl Paths {
    pub fn resolve() -> Result<Self> {
        let dirs = ProjectDirs::from("ai", "holistic", APP_NAME);

        Ok(Self {
            state_dir: env_dir("SURFACE_STATE_DIR")
                .unwrap_or_else(|| default_state_dir(dirs.as_ref())),
            config_dir: env_dir("SURFACE_CONFIG_DIR")
                .unwrap_or_else(|| default_config_dir(dirs.as_ref())),
        })
    }

    /// Rooted at a single directory, for tests.
    #[cfg(test)]
    pub fn rooted_at(root: impl AsRef<std::path::Path>) -> Self {
        let root = root.as_ref();
        Self {
            state_dir: root.join("state"),
            config_dir: root.join("config"),
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("surface.toml")
    }

    /// Create the state directory. The config directory is only ever read, so a
    /// user who never writes a config never gets an empty directory made for
    /// them.
    pub fn ensure(&self) -> Result<()> {
        std::fs::create_dir_all(&self.state_dir)
            .with_context(|| format!("creating directory {}", self.state_dir.display()))
    }
}

/// The user's home directory.
///
/// `HOME` on unix, `USERPROFILE` on Windows. Lives here rather than in
/// [`crate::browser`] because the transcript scan needs it too, and that works
/// without the `sqlite` feature.
pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn env_dir(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn default_state_dir(dirs: Option<&ProjectDirs>) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(d) = dirs {
            return d.data_local_dir().to_path_buf();
        }
    }

    dirs.map(|d| d.data_local_dir().to_path_buf())
        .unwrap_or_else(|| fallback_dir().join("state"))
}

fn default_config_dir(dirs: Option<&ProjectDirs>) -> PathBuf {
    dirs.map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| fallback_dir().join("config"))
}

/// Last resort when the platform has no usable home directory (rare: service
/// accounts, minimal containers). Keeps the scan running rather than exiting.
fn fallback_dir() -> PathBuf {
    std::env::temp_dir().join(APP_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rooted_paths_are_all_under_the_root() {
        let paths = Paths::rooted_at("/tmp/surface-test");
        assert!(paths.state_dir.starts_with("/tmp/surface-test"));
        assert!(paths.config_file().starts_with("/tmp/surface-test"));
        assert_eq!(paths.config_file().file_name().unwrap(), "surface.toml");
    }

    #[test]
    fn resolve_produces_absolute_directories() {
        let paths = Paths::resolve().unwrap();
        assert!(paths.state_dir.is_absolute());
        assert!(paths.config_dir.is_absolute());
    }
}
