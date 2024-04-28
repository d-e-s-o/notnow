// Copyright (C) 2022-2024 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::OsString;
use std::io::sink;

use notnow::run_prog;
use notnow::test::default_tasks_and_tags;
use notnow::DirCap;
use notnow::TaskState;
use notnow::UiConfig;

use tempfile::TempDir;

use tokio::test;


/// Check that we can run the program.
#[test]
async fn prog_running() {
  // Open a bunch of dialogs to exercise as many code paths as possible.
  static KEYS: [u8; 6] = [b't', b'\n', b'\n', b'\n', b'w', b'q'];

  let (ui_config, task_state) = default_tasks_and_tags();

  let task_state = TaskState::with_serde(task_state).unwrap();
  let tasks_dir = TempDir::new().unwrap();
  let mut tasks_root_cap = DirCap::for_dir(tasks_dir.path().to_path_buf())
    .await
    .unwrap();
  let () = task_state.save(&mut tasks_root_cap).await.unwrap();

  let ui_config = UiConfig::with_serde(ui_config, &task_state).unwrap();
  let ui_config_dir = TempDir::new().unwrap();
  let ui_config_file_name = OsString::from("notnow.json");
  let ui_config_file_path = (
    ui_config_dir.path().to_path_buf(),
    ui_config_file_name.clone(),
  );
  let mut ui_config_dir_cap = DirCap::for_dir(ui_config_dir.path().to_path_buf())
    .await
    .unwrap();
  let ui_config_dir_write_guard = ui_config_dir_cap.write().await.unwrap();
  let mut ui_config_file_cap = ui_config_dir_write_guard.file_cap(&ui_config_file_name);
  let () = ui_config.save(&mut ui_config_file_cap).await.unwrap();

  let ui_state_dir = TempDir::new().unwrap();
  let ui_state_file_name = OsString::from("ui-state.json");
  let ui_state_file_path = (ui_state_dir.path().to_path_buf(), ui_state_file_name);

  let mut output = sink();

  run_prog(
    KEYS.as_slice(),
    &mut output,
    tasks_dir.path().to_path_buf(),
    ui_config_file_path,
    ui_state_file_path,
  )
  .await
  .unwrap()
}
