// Copyright (C) 2017-2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

//! A terminal based task management application.

use std::process::exit;

use notnow::run;


fn main() {
  exit(run());
}
