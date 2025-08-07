// Copyright (C) 2017-2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

//! A terminal based task management application.

use std::process::ExitCode;

use notnow::run;


fn main() -> ExitCode {
  match run() {
    Ok(_) => ExitCode::SUCCESS,
    Err(err) => {
      eprintln!("{err:?}");
      ExitCode::FAILURE
    },
  }
}
