// Copyright (C) 2023-2024 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;
use std::rc::Rc;

use anyhow::anyhow;
use anyhow::Context as _;
use anyhow::Result;

use crate::cap::FileCap;
use crate::colors::Colors;
use crate::ser::backends::Json;
use crate::ser::state::UiConfig as SerUiConfig;
use crate::ser::ToSerde;
use crate::state::load_state_from_file;
use crate::state::save_state_to_file;
use crate::state::should_save_state;
use crate::state::TaskState;
use crate::tags::Tag;
use crate::view::View;
use crate::view::ViewBuilder;


/// A struct encapsulating the UI's configuration.
#[derive(Debug)]
pub struct Config {
  /// The configured colors.
  pub colors: Colors,
  /// The tag to toggle on user initiated action.
  pub toggle_tag: Option<Tag>,
  /// The views used in the UI.
  pub views: Vec<View>,
}

impl Config {
  /// Load a `Config` object from a file.
  pub async fn load(config_path: &Path, task_state: &TaskState) -> Result<Self> {
    let config = load_state_from_file::<Json, SerUiConfig>(config_path)
      .await
      .with_context(|| {
        format!(
          "failed to load UI configuration from {}",
          config_path.display()
        )
      })?
      .unwrap_or_default();

    Self::with_serde(config, task_state)
  }

  /// Create a `Config` object from a serialized configuration.
  pub fn with_serde(config: SerUiConfig, task_state: &TaskState) -> Result<Self> {
    let SerUiConfig {
      colors,
      toggle_tag,
      views,
    } = config;
    let templates = task_state.templates();
    let tasks = task_state.tasks();

    let mut views = views
      .into_iter()
      .map(|view| {
        let name = view.name.clone();
        let view = View::with_serde(view, templates, Rc::clone(tasks))
          .with_context(|| format!("failed to instantiate view '{}'", name))?;
        Ok(view)
      })
      .collect::<Result<Vec<_>>>()?;

    // For convenience for the user, we add a default view capturing
    // all tasks if no other views have been configured.
    if views.is_empty() {
      views.push(ViewBuilder::new(Rc::clone(tasks)).build("all"))
    }

    let toggle_tag = if let Some(toggle_tag) = toggle_tag {
      let toggle_tag = templates
        .instantiate(toggle_tag.id)
        .ok_or_else(|| anyhow!("encountered invalid toggle tag ID {}", toggle_tag.id))?;

      Some(toggle_tag)
    } else {
      None
    };

    let slf = Self {
      colors,
      toggle_tag,
      views,
    };
    Ok(slf)
  }

  /// Check whether the configuration is changed from what is in the
  /// provided `file`.
  pub async fn is_changed(&self, file: &Path) -> bool {
    should_save_state::<Json, _>(file, &self.to_serde())
      .await
      .unwrap_or(true)
  }

  /// Persist the configuration into a file.
  pub async fn save(&self, file_cap: &mut FileCap<'_>) -> Result<()> {
    save_state_to_file::<Json, _>(file_cap, &self.to_serde()).await
  }
}

impl ToSerde for Config {
  type Output = SerUiConfig;

  /// Convert this object into a serializable one.
  fn to_serde(&self) -> Self::Output {
    let views = self.views.iter().map(View::to_serde).collect();

    let config = SerUiConfig {
      colors: self.colors,
      toggle_tag: self.toggle_tag.as_ref().map(ToSerde::to_serde),
      views,
    };
    config
  }
}


#[cfg(test)]
pub mod tests {
  use super::*;

  use std::ffi::OsStr;

  use tempfile::TempDir;

  use tokio::test;

  use crate::cap::DirCap;
  use crate::ser::state::TaskState as SerTaskState;
  use crate::ser::tasks::Tasks as SerTasks;
  use crate::test::make_tasks;


  /// Create a `Config` object.
  fn make_config(task_count: usize) -> (Config, TaskState) {
    let tasks = make_tasks(task_count);
    let task_state = SerTaskState {
      tasks_meta: Default::default(),
      tasks: SerTasks::from(tasks),
    };
    let task_state = TaskState::with_serde(task_state).unwrap();

    let config = Default::default();
    let config = Config::with_serde(config, &task_state).unwrap();

    (config, task_state)
  }

  /// Check that we can save a `Config` and load it back.
  #[test]
  async fn save_and_load_config() {
    let (config, task_state) = make_config(3);
    let ui_file_dir = TempDir::new().unwrap();
    let ui_file_name = OsStr::new("config");
    let ui_file = ui_file_dir.path().join(ui_file_name);
    let mut ui_dir_cap = DirCap::for_dir(ui_file_dir.path().to_path_buf())
      .await
      .unwrap();
    let ui_write_guard = ui_dir_cap.write().await.unwrap();
    let mut ui_file_cap = ui_write_guard.file_cap(ui_file_name);
    let () = config.save(&mut ui_file_cap).await.unwrap();

    let _new_config = Config::load(&ui_file, &task_state).await.unwrap();
  }

  /// Verify that loading a `Config` object succeeds even if the file to
  /// load from is not present.
  #[test]
  async fn load_config_file_not_found() {
    let (config, task_state) = {
      let (config, task_state) = make_config(1);

      let ui_file_dir = TempDir::new().unwrap();
      let ui_file_name = OsStr::new("config");
      let mut ui_dir_cap = DirCap::for_dir(ui_file_dir.path().to_path_buf())
        .await
        .unwrap();
      let ui_write_guard = ui_dir_cap.write().await.unwrap();
      let mut ui_file_cap = ui_write_guard.file_cap(ui_file_name);
      let () = config.save(&mut ui_file_cap).await.unwrap();

      (ui_file_dir.path().join(ui_file_name), task_state)
    };

    let _new_config = Config::load(&config, &task_state).await.unwrap();
  }
}
