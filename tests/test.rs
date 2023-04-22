// Copyright (C) 2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::OsString;
use std::io::sink;

use notnow::run_prog;
use notnow::state::TaskState;
use notnow::state::UiState;
use notnow::test::default_tasks_and_tags;

use tempfile::TempDir;

use tokio::test;


/// Check that we can run the program.
#[test]
async fn prog_running() {
  static KEYS: [u8; 2] = [b'w', b'q'];

  let (ui_state, task_state) = default_tasks_and_tags();
  let tasks_dir = TempDir::new().unwrap();
  let task_state = TaskState::with_serde(task_state).unwrap();
  let ui_dir = TempDir::new().unwrap();
  let ui_state = UiState::with_serde(ui_state, &task_state).unwrap();
  let ui_file_name = OsString::from("notnow.json");
  let ui_file_path = (ui_dir.path().to_path_buf(), ui_file_name);

  ui_state.save(&ui_file_path).await.unwrap();
  task_state.save(tasks_dir.path()).await.unwrap();

  let mut output = sink();

  run_prog(
    KEYS.as_slice(),
    &mut output,
    ui_file_path,
    tasks_dir.path().to_path_buf(),
  )
  .await
  .unwrap()
}
