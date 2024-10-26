// Copyright (C) 2022-2024 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::io::sink;

use notnow::run_prog;
use notnow::test::default_tasks_and_tags;
use notnow::DirCap;
use notnow::Paths;
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

  let config_dir = TempDir::new().unwrap();
  let paths = Paths::new(Some(config_dir.path().to_path_buf())).unwrap();

  let task_state = TaskState::with_serde(task_state).unwrap();
  let mut tasks_root_cap = DirCap::for_dir(paths.tasks_dir()).await.unwrap();
  let () = task_state.save(&mut tasks_root_cap).await.unwrap();

  let ui_config = UiConfig::with_serde(ui_config, &task_state).unwrap();
  let mut ui_config_dir_cap = DirCap::for_dir(paths.ui_config_dir().to_path_buf())
    .await
    .unwrap();
  let ui_config_dir_write_guard = ui_config_dir_cap.write().await.unwrap();
  let mut ui_config_file_cap = ui_config_dir_write_guard.file_cap(paths.ui_config_file());
  let () = ui_config.save(&mut ui_config_file_cap).await.unwrap();

  let mut output = sink();

  run_prog(KEYS.as_slice(), &mut output, paths).await.unwrap()
}
