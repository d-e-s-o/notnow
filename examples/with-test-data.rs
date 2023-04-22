// Copyright (C) 2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::OsString;
use std::io::stdin;
use std::io::stdout;

use anyhow::Result;

use notnow::run_prog;
use notnow::state::TaskState;
use notnow::state::UiState;
use notnow::test::default_tasks_and_tags;

use tempfile::TempDir;


#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
  let (ui_state, task_state) = default_tasks_and_tags();
  let task_state = TaskState::with_serde(task_state)?;
  let tasks_dir = TempDir::new()?;
  let ui_state = UiState::with_serde(ui_state, &task_state)?;
  let ui_dir = TempDir::new()?;
  let ui_file_name = OsString::from("notnow.json");
  let ui_file_path = (ui_dir.path().to_path_buf(), ui_file_name);

  ui_state.save(&ui_file_path).await?;
  task_state.save(tasks_dir.path()).await?;

  run_prog(
    stdin(),
    stdout().lock(),
    ui_file_path,
    tasks_dir.path().to_path_buf(),
  )
  .await
}
