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
    /// all errors here — see [`Config::load_or_default`] for the tolerant
    /// version.
    pub fn load_from(path: impl AsRef<Path>) -> Result<Config, Error> {
        let path = path.as_ref();
        let text =
            std::fs::read_to_string(path).map_err(|e| Error(format!("{}: {e}", path.display())))?;
        Config::parse(&text).map_err(|e| Error(format!("{}: {e}", path.display())))
    }

    /// Read a config file if it exists, otherwise use the defaults.
    ///
    /// A file that is not there is the normal case. A file that *is* there
    /// but is broken is still an error — see [`Config::load`].
    pub fn load_or_default(path: impl AsRef<Path>) -> Config {
        let path = path.as_ref();
        if !path.exists() {
            return Config::default();
        }
        Config::load_from(path).unwrap_or_default()
    }

    /// Load from the standard location, honouring `XDG_CONFIG_HOME`.
    ///
    /// A missing file yields the defaults; a malformed one is an error the
    /// caller should report rather than paper over.
    pub fn load() -> Result<Config, Error> {
        let path = default_path();
        if !path.exists() {
            return Ok(Config::default());
        }
        Config::load_from(path)
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
