// Copyright (C) 2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::OsString;
use std::io::sink;

use notnow::cap::DirCap;
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

  let task_state = TaskState::with_serde(task_state).unwrap();
  let tasks_dir = TempDir::new().unwrap();
  let mut tasks_root_cap = DirCap::for_dir(tasks_dir.path().to_path_buf())
    .await
    .unwrap();
  let () = task_state.save(&mut tasks_root_cap).await.unwrap();

  let ui_state = UiState::with_serde(ui_state, &task_state).unwrap();
  let ui_dir = TempDir::new().unwrap();
  let ui_file_name = OsString::from("notnow.json");
  let ui_file_path = (ui_dir.path().to_path_buf(), ui_file_name.clone());
  let mut ui_dir_cap = DirCap::for_dir(ui_dir.path().to_path_buf()).await.unwrap();
  let ui_dir_write_guard = ui_dir_cap.write().await.unwrap();
  let mut ui_file_cap = ui_dir_write_guard.file_cap(&ui_file_name);
  let () = ui_state.save(&mut ui_file_cap).await.unwrap();

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
