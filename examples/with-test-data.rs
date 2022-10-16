// Copyright (C) 2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::io::stdout;
use std::io::Result;

use notnow::run_prog;
use notnow::state::State;
use notnow::test::default_tasks_and_tags;

use tempfile::NamedTempFile;


fn main() -> Result<()> {
  let ui_file = NamedTempFile::new()?;
  let task_file = NamedTempFile::new()?;
  let (ui_state, task_state) = default_tasks_and_tags();
  let state = State::with_serde(ui_state, ui_file.path(), task_state, task_file.path());
  let State(ui_state, task_state) = state?;

  ui_state.save()?;
  task_state.save()?;

  run_prog(stdout().lock(), ui_file.path(), task_file.path())
}
