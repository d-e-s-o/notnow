// Copyright (C) 2024 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt::Debug;
use std::path::PathBuf;

use clap::Parser;


/// A terminal based task and TODO management software.
#[derive(Debug, Parser)]
#[clap(version = env!("VERSION"))]
pub struct Args {
  /// The configuration directory to use.
  ///
  /// The directory typically contains a `notnow.json` configuration
  /// alongside all tasks in the `tasks/` sub-directory.
  #[clap(short, long)]
  pub config_dir: Option<PathBuf>,
  /// Force reclamation of stale lock files in case a previous program
  /// instance terminated improperly.
  #[clap(short, long)]
  pub force: bool,
}
