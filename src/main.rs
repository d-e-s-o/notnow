// Copyright (C) 2017-2026 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

//! A terminal based task management application.

use std::env::args_os;
use std::process::ExitCode;

use clap::Parser as _;

use notnow::run;
use notnow::Args;


fn main() -> ExitCode {
  let args = match Args::try_parse_from(args_os()) {
    Ok(args) => args,
    Err(err) => {
      let _result = err.print();
      return u8::try_from(err.exit_code())
        .map(ExitCode::from)
        .unwrap_or(ExitCode::FAILURE)
    },
  };

  run(args)
    .map(|_| ExitCode::SUCCESS)
    .map_err(|e| eprintln!("{e:?}"))
    .unwrap_or(ExitCode::FAILURE)
}
