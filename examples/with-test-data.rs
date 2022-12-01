// Copyright (C) 2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::io::stdout;
use std::io::Result;

use notnow::run_prog;
use notnow::state::State;
use notnow::test::default_tasks_and_tags;

use tempfile::NamedTempFile;
use tempfile::TempDir;


fn main() -> Result<()> {
  let ui_file = NamedTempFile::new()?;
  let tasks_dir = TempDir::new()?;
  let (ui_state, task_state) = default_tasks_and_tags();
  let state = State::with_serde(ui_state, ui_file.path(), task_state, tasks_dir.path());
  let State(ui_state, task_state) = state?;

  ui_state.save()?;
  task_state.save()?;

  run_prog(stdout().lock(), ui_file.path(), tasks_dir.path())
}
