// Copyright (C) 2022-2023 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::OsString;
use std::io::stdin;
use std::io::stdout;

use anyhow::Result;

use notnow::run_prog;
use notnow::test::default_tasks_and_tags;
use notnow::DirCap;
use notnow::TaskState;
use notnow::UiConfig;

use tempfile::TempDir;


#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
  let (ui_config, task_state) = default_tasks_and_tags();

  let task_state = TaskState::with_serde(task_state)?;
  let tasks_dir = TempDir::new()?;
  let mut tasks_root_cap = DirCap::for_dir(tasks_dir.path().to_path_buf()).await?;
  let () = task_state.save(&mut tasks_root_cap).await?;

  let ui_config = UiConfig::with_serde(ui_config, &task_state)?;
  let ui_config_dir = TempDir::new()?;
  let ui_config_file_name = OsString::from("notnow.json");
  let ui_config_file_path = (
    ui_config_dir.path().to_path_buf(),
    ui_config_file_name.clone(),
  );
  let mut ui_config_dir_cap = DirCap::for_dir(ui_config_dir.path().to_path_buf()).await?;
  let ui_config_dir_write_guard = ui_config_dir_cap.write().await?;
  let mut ui_config_file_cap = ui_config_dir_write_guard.file_cap(&ui_config_file_name);
  let () = ui_config.save(&mut ui_config_file_cap).await?;

  let ui_state_dir = TempDir::new()?;
  let ui_state_file_name = OsString::from("ui-state.json");
  let ui_state_file_path = (ui_state_dir.path().to_path_buf(), ui_state_file_name);

  run_prog(
    stdin(),
    stdout().lock(),
    tasks_dir.path().to_path_buf(),
    ui_config_file_path,
    ui_state_file_path,
  )
  .await
}
