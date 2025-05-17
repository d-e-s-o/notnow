// Copyright (C) 2022-2024 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::io::stdin;
use std::io::stdout;

use anyhow::Result;

use notnow::run_prog;
use notnow::test::default_tasks_and_tags;
use notnow::DirCap;
use notnow::Paths;
use notnow::TaskState;
use notnow::UiConfig;

use tempfile::TempDir;


#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
  let (ui_config, task_state) = default_tasks_and_tags();

  let config_dir = TempDir::new()?;
  let paths = Paths::new(Some(config_dir.path().to_path_buf()))?;

  let task_state = TaskState::with_serde(task_state)?;
  let mut tasks_root_cap = DirCap::for_dir(paths.tasks_dir()).await?;
  let () = task_state.save(&mut tasks_root_cap).await?;

  let ui_config = UiConfig::with_serde(ui_config, &task_state)?;
  let mut ui_config_dir_cap = DirCap::for_dir(paths.ui_config_dir().to_path_buf()).await?;
  let ui_config_dir_write_guard = ui_config_dir_cap.write().await?;
  let mut ui_config_file_cap = ui_config_dir_write_guard.file_cap(paths.ui_config_file());
  let () = ui_config.save(&mut ui_config_file_cap).await?;

  run_prog(stdin(), stdout().lock(), paths).await
}
