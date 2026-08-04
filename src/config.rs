//! `~/.config/rtop/config.toml`.
//!
//! Unknown keys are rejected rather than ignored. Silently skipping
//! `refresh_msec` means the setting appears not to work with nothing
//! anywhere saying why, which is a worse experience than refusing to start.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::sort::SortKey;

/// Sampling costs several milliseconds, so a refresh below this would spend
/// a meaningful slice of the machine watching itself.
pub const MIN_REFRESH_MS: u64 = 100;

/// Everything configurable from the file.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub refresh_ms: u64,
    pub theme: String,
    pub tree_view: bool,
    pub hide_kernel_threads: bool,
    pub processes: ProcessConfig,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct ProcessConfig {
    pub sort_by: SortKey,
    pub sort_desc: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            refresh_ms: 1500,
            theme: String::from("default"),
            tree_view: false,
            hide_kernel_threads: false,
            processes: ProcessConfig::default(),
        }
    }
}

impl Default for ProcessConfig {
    fn default() -> Self {
        ProcessConfig {
            sort_by: SortKey::Cpu,
            sort_desc: true,
        }
    }
}

impl Config {
    /// Parse config text. An empty document yields the defaults.
    pub fn parse(text: &str) -> Result<Config, Error> {
        let config: Config = toml::from_str(text).map_err(|e| Error(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), Error> {
        if self.refresh_ms < MIN_REFRESH_MS {
            return Err(Error(format!(
                "refresh_ms must be at least {MIN_REFRESH_MS}, got {}",
                self.refresh_ms
            )));
        }
        Ok(())
    }

    /// Read and parse a config file. Missing, unreadable, and malformed are
    /// all errors here — see [`Config::load_from_optional`] when the file is
    /// allowed not to exist.
    pub fn load_from(path: impl AsRef<Path>) -> Result<Config, Error> {
        let path = path.as_ref();
        let text =
            std::fs::read_to_string(path).map_err(|e| Error(format!("{}: {e}", path.display())))?;
        Config::parse(&text).map_err(|e| Error(format!("{}: {e}", path.display())))
    }

    /// Read a config file that is allowed not to exist.
    ///
    /// A missing file yields the defaults. Everything else — an unreadable
    /// file, a malformed one, an unknown key — is an error, because a
    /// setting that appears not to work with nothing anywhere saying why is
    /// worse than a refusal to start.
    ///
    /// "Missing" is decided by the read's own error kind rather than by
    /// [`Path::exists`], which reports `false` for a permission error and
    /// would quietly take the defaults branch for a file that is right there.
    pub fn load_from_optional(path: impl AsRef<Path>) -> Result<Config, Error> {
        let path = path.as_ref();
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
            Err(e) => return Err(Error(format!("{}: {e}", path.display()))),
        };
        Config::parse(&text).map_err(|e| Error(format!("{}: {e}", path.display())))
    }

    /// Load from the standard location, honouring `XDG_CONFIG_HOME`.
    pub fn load() -> Result<Config, Error> {
        Config::load_from_optional(default_path())
    }
}

/// `$XDG_CONFIG_HOME/rtop/config.toml`, falling back to `~/.config`.
pub fn default_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("rtop").join("config.toml")
}

#[derive(Debug)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}
