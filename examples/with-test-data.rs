// Copyright (C) 2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::io::stdout;
use std::io::Result;

use notnow::run_prog;
use notnow::state::State;
use notnow::test::default_tasks_and_tags;

use tempfile::NamedTempFile;


fn main() -> Result<()> {
  let task_file = NamedTempFile::new()?;
  let ui_file = NamedTempFile::new()?;
  let (task_state, ui_state) = default_tasks_and_tags();
  let state = State::with_serde(task_state, task_file.path(), ui_state, ui_file.path());
  let State(task_state, ui_state) = state?;

  task_state.save()?;
  ui_state.save()?;

  run_prog(stdout().lock(), ui_file.path(), task_file.path())
}
