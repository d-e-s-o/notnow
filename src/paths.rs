// Copyright (C) 2024 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;

use anyhow::anyhow;
use anyhow::Result;

use dirs::cache_dir;
use dirs::config_dir;


/// A type taking care of the program's path handling needs.
#[derive(Debug)]
pub struct Paths {
  /// The path to the configuration directory.
  config_dir: PathBuf,
  /// The path to the directory containing "ephemeral" state.
  state_dir: PathBuf,
}

impl Paths {
  /// Instantiate a new `Paths` object, optionally using `config_dir` as
  /// the directory storing configuration data (including tasks).
  pub fn new(config_dir: Option<PathBuf>) -> Result<Self> {
    let config_dir = if let Some(config_dir) = config_dir {
      config_dir
    } else {
      self::config_dir()
        .ok_or_else(|| anyhow!("unable to determine config directory"))?
        .join("notnow")
    };

    let state_dir = cache_dir()
      .ok_or_else(|| anyhow!("unable to determine cache directory"))?
      .join("notnow");

    let slf = Self {
      config_dir,
      state_dir,
    };
    Ok(slf)
  }

  /// Retrieve the path to the program's configuration directory.
  pub fn ui_config_dir(&self) -> &Path {
    &self.config_dir
  }

  /// Retrieve the file name of the program's UI configuration.
  pub fn ui_config_file(&self) -> &OsStr {
    OsStr::new("notnow.json")
  }

  /// Retrieve the path to the program's task directory.
  pub fn tasks_dir(&self) -> PathBuf {
    self.ui_config_dir().join("tasks")
  }

  /// Retrieve the path to the program's "volatile" UI state directory.
  pub fn ui_state_dir(&self) -> &Path {
    &self.state_dir
  }

  /// Retrieve the file name of the program's "volatile" UI state.
  pub fn ui_state_file(&self) -> &OsStr {
    OsStr::new("ui-state.json")
  }

  /// Retrieve the path to the program's lock file.
  pub(crate) fn lock_file(&self) -> PathBuf {
    self.state_dir.join("notnow.lock")
  }
}
