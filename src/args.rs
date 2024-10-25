// Copyright (C) 2024 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt::Debug;

use clap::Parser;


/// A terminal based task and TODO management software.
#[derive(Debug, Parser)]
#[clap(version = env!("VERSION"))]
pub struct Args {
  /// Force reclamation of stale lock files in case a previous program
  /// instance terminated improperly.
  #[clap(short, long)]
  pub force: bool,
}
