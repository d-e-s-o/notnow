// Copyright (C) 2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::io::sink;

use notnow::run_prog;
use notnow::state::State;
use notnow::test::default_tasks_and_tags;

use tempfile::NamedTempFile;
use tempfile::TempDir;

use tokio::test;


/// Check that we can run the program.
#[test]
async fn prog_running() {
  static KEYS: [u8; 2] = [b'w', b'q'];

  let ui_file = NamedTempFile::new().unwrap();
  let tasks_dir = TempDir::new().unwrap();
  let (ui_state, task_state) = default_tasks_and_tags();
  let state = State::with_serde(ui_state, ui_file.path(), task_state, tasks_dir.path());
  let State(ui_state, task_state) = state.unwrap();

  ui_state.save().await.unwrap();
  task_state.save().await.unwrap();

  let mut output = sink();

  run_prog(
    KEYS.as_slice(),
    &mut output,
    ui_file.path(),
    tasks_dir.path(),
  )
  .await
  .unwrap()
}
