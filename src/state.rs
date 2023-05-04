// Copyright (C) 2017-2023 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

//! Definitions pertaining UI configuration and task state of the
//! program.

use std::cell::Cell;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::io::ErrorKind;
use std::path::Path;
use std::rc::Rc;

use anyhow::anyhow;
use anyhow::Context as _;
use anyhow::Result;

use crate::ser::backends::iCal;
use crate::ser::backends::Backend;
use crate::ser::backends::Json;

use tokio::fs::create_dir_all;
use tokio::fs::read_dir;
use tokio::fs::remove_file;
use tokio::fs::DirEntry;
use tokio::fs::File;
use tokio::fs::OpenOptions;
use tokio::fs::ReadDir;
use tokio::io::AsyncReadExt as _;
use tokio::io::AsyncWriteExt as _;

use uuid::uuid;

use crate::cap::DirCap;
use crate::cap::FileCap;
use crate::cap::WriteGuard;
use crate::colors::Colors;
use crate::ser::state::TaskState as SerTaskState;
use crate::ser::state::UiConfig as SerUiConfig;
use crate::ser::tasks::Id as SerTaskId;
use crate::ser::tasks::Task as SerTask;
use crate::ser::tasks::Tasks as SerTasks;
use crate::ser::tasks::TasksMeta as SerTasksMeta;
use crate::ser::ToSerde;
use crate::tags::Tag;
use crate::tags::Templates;
use crate::tasks::Tasks;
use crate::view::View;
use crate::view::ViewBuilder;

/// The ID we use for storing task meta data.
// We use the "special" UUID 00000000-0000-0000-0000-000000000000 for
// storing task meta data.
const TASKS_META_ID: SerTaskId = uuid!("00000000-0000-0000-0000-000000000000");


/// Load some serialized state from a file.
async fn load_state_from_file<B, T>(path: &Path) -> Result<Option<T>>
where
  B: Backend<T>,
{
  match File::open(path).await {
    Ok(mut file) => {
      let mut content = Vec::new();
      let _count = file
        .read_to_end(&mut content)
        .await
        .context("failed to read complete file content")?;
      let state = B::deserialize(&content).context("failed to decode state")?;
      Ok(Some(state))
    },
    Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
    Err(e) => Err(e).context("error opening file for reading"),
  }
}

/// Load a task from a directory entry.
async fn load_task_from_dir_entry(entry: &DirEntry) -> Result<Option<SerTask>> {
  let file_name = entry.file_name();
  let path = entry.path();
  let id = file_name
    .to_str()
    .and_then(|id| SerTaskId::try_parse(id).ok())
    .ok_or_else(|| anyhow!("filename {} is not a valid UUID", path.display()))?;

  let result = load_state_from_file::<iCal, SerTask>(&path)
    .await
    .with_context(|| format!("failed to load state from {}", path.display()))?
    .map(|mut task| {
      // TODO: Silently overwriting the ID may not be the best choice,
      //       but it depends on whether it is actually serialized and
      //       deserialized to begin with. If there is a discrepancy,
      //       that may indicate a problem. On the other hand, we want
      //       to give precedence to the file name, because it is the
      //       most user visible and it would be confusing to not honor
      //       it.
      task.id = id;
      task
    });

  Ok(result)
}

/// Load tasks by iterating over the entries of a `ReadDir` object.
#[allow(clippy::type_complexity)]
async fn load_tasks_from_read_dir(dir: ReadDir) -> Result<(Vec<SerTask>, Option<SerTasksMeta>)> {
  let mut dir = dir;
  // Ideally we'd size the `Vec` as per the number of directory entries,
  // but `fs::ReadDir` does not currently expose that number.
  let mut tasks = Vec::new();
  let mut tasks_meta = None;

  let mut buffer = SerTaskId::encode_buffer();
  let tasks_meta_uuid = TASKS_META_ID.as_hyphenated().encode_lower(&mut buffer);

  while let Some(entry) = dir
    .next_entry()
    .await
    .context("failed to iterate directory contents")?
  {
    if entry.file_name() == OsStr::new(tasks_meta_uuid) {
      debug_assert_eq!(
        tasks_meta, None,
        "encountered multiple task meta data files"
      );
      tasks_meta = load_state_from_file::<iCal, SerTasksMeta>(&entry.path()).await?;
    } else if let Some(data) = load_task_from_dir_entry(&entry).await? {
      let () = tasks.push(data);
    }
  }

  Ok((tasks, tasks_meta))
}

/// Load tasks from a directory.
///
/// The function assumes that the directory *only* contains files
/// representing tasks (along with one file for meta data).
async fn load_tasks_from_dir(root: &Path) -> Result<SerTaskState> {
  let dir = match read_dir(root).await {
    Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Default::default()),
    result => result,
  }?;

  let (tasks, tasks_meta) = load_tasks_from_read_dir(dir).await?;
  let tasks_meta = tasks_meta.unwrap_or_default();

  Ok(SerTaskState {
    tasks_meta,
    tasks: SerTasks(tasks),
  })
}

/// Save some state into a file.
async fn save_state_to_file<B, T>(file_cap: &mut FileCap<'_>, state: &T) -> Result<()>
where
  B: Backend<T>,
  T: PartialEq,
{
  let path = file_cap.path();
  if let Ok(Some(existing)) = load_state_from_file::<B, T>(path).await {
    if &existing == state {
      // If the file already contains the expected state there is no need
      // for us to write it again.
      return Ok(())
    }
  }

  if let Some(dir) = path.parent() {
    let () = create_dir_all(dir).await?;
  }

  let serialized = B::serialize(state)?;

  let () = file_cap
    .with_writeable_path(|path| async {
      let () = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .await?
        .write_all(serialized.as_ref())
        .await?;
      Ok(())
    })
    .await?;

  Ok(())
}

/// Save a task into a file in the given directory.
async fn save_task_to_file(write_guard: &mut WriteGuard<'_>, task: &SerTask) -> Result<()> {
  let mut file_cap = write_guard.file_cap(OsStr::new(&task.id.to_string()));
  // TODO: It would be better if we were create a temporary file first
  //       if one already exists and then rename atomically. But even
  //       nicer would be if we somehow wrapped all saving in a
  //       transaction of sorts. That would allow us to eliminate the
  //       chance for *any* inconsistency, e.g., when saving UI
  //       configuration before task state and the latter failing the
  //       operation.
  save_state_to_file::<iCal, _>(&mut file_cap, task).await
}

/// Save a task meta data into a file in the provided directory.
async fn save_tasks_meta_to_dir(
  write_guard: &mut WriteGuard<'_>,
  tasks_meta: &SerTasksMeta,
) -> Result<()> {
  let mut file_cap = write_guard.file_cap(OsStr::new(&TASKS_META_ID.to_string()));
  save_state_to_file::<iCal, _>(&mut file_cap, tasks_meta).await
}

/// Save tasks into files in the provided directory.
async fn save_tasks_to_dir(dir_cap: &mut DirCap, tasks: &SerTaskState) -> Result<()> {
  let mut write_guard = dir_cap.write().await?;

  for task in tasks.tasks.0.iter() {
    let () = save_task_to_file(&mut write_guard, task).await?;
  }

  let () = save_tasks_meta_to_dir(&mut write_guard, &tasks.tasks_meta).await?;
  let ids = tasks
    .tasks
    .0
    .iter()
    .map(|task| task.id)
    .collect::<HashSet<_>>();

  // Remove all files that do not correspond to an ID we just saved.
  let mut dir = read_dir(write_guard.path()).await?;
  while let Some(entry) = dir.next_entry().await? {
    let id = entry
      .file_name()
      .to_str()
      .and_then(|id| SerTaskId::try_parse(id).ok());

    let remove = if let Some(id) = id {
      id != TASKS_META_ID && ids.get(&id).is_none()
    } else {
      true
    };

    if remove {
      // Note that we purposefully do not support the case of having a
      // directory inside the root directory, as we'd never create one
      // there programmatically.
      let () = remove_file(entry.path()).await?;
    }
  }

  Ok(())
}


/// A struct encapsulating the UI's configuration.
#[derive(Debug)]
pub struct UiConfig {
  /// The configured colors.
  pub colors: Cell<Option<Colors>>,
  /// The tag to toggle on user initiated action.
  pub toggle_tag: Option<Tag>,
  /// The views used in the UI.
  pub views: Vec<(View, Option<usize>)>,
  /// The currently selected `View`.
  pub selected: Option<usize>,
}

impl UiConfig {
  /// Load `UiConfig` from a configuration file.
  pub async fn load(config: &Path, task_state: &TaskState) -> Result<Self> {
    let state = load_state_from_file::<Json, SerUiConfig>(config)
      .await
      .with_context(|| format!("failed to load UI state from {}", config.display()))?
      .unwrap_or_default();

    Self::with_serde(state, task_state)
  }

  /// Create a `UiConfig` object from serialized state.
  pub fn with_serde(state: SerUiConfig, task_state: &TaskState) -> Result<Self> {
    let templates = &task_state.templates;
    let tasks = &task_state.tasks;

    let mut views = state
      .views
      .into_iter()
      .map(|(view, selected)| {
        let name = view.name.clone();
        let view = View::with_serde(view, templates, tasks.clone())
          .with_context(|| format!("failed to instantiate view '{}'", name))?;
        Ok((view, selected))
      })
      .collect::<Result<Vec<_>>>()?;

    // For convenience for the user, we add a default view capturing
    // all tasks if no other views have been configured.
    if views.is_empty() {
      views.push((ViewBuilder::new(tasks.clone()).build("all"), None))
    }

    let toggle_tag = if let Some(toggle_tag) = state.toggle_tag {
      let toggle_tag = templates
        .instantiate(toggle_tag.id)
        .ok_or_else(|| anyhow!("encountered invalid toggle tag ID {}", toggle_tag.id))?;

      Some(toggle_tag)
    } else {
      None
    };

    let slf = Self {
      colors: Cell::new(Some(state.colors)),
      toggle_tag,
      views,
      selected: state.selected,
    };
    Ok(slf)
  }

  /// Persist the state into a file.
  pub async fn save(&self, file_cap: &mut FileCap<'_>) -> Result<()> {
    let ui_config = load_state_from_file::<Json, SerUiConfig>(file_cap.path())
      .await
      .unwrap_or_default()
      .unwrap_or_default();
    self.colors.set(Some(ui_config.colors));

    save_state_to_file::<Json, _>(file_cap, &self.to_serde()).await
  }
}

impl ToSerde for UiConfig {
  type Output = SerUiConfig;

  /// Convert this object into a serializable one.
  fn to_serde(&self) -> Self::Output {
    debug_assert!(self.selected.is_none() || self.selected.unwrap() < self.views.len());

    let views = self.views.iter().map(|(q, s)| (q.to_serde(), *s)).collect();

    SerUiConfig {
      colors: self.colors.get().unwrap_or_default(),
      toggle_tag: self.toggle_tag.as_ref().map(ToSerde::to_serde),
      views,
      selected: self.selected,
    }
  }
}


/// A struct encapsulating the task state of the program.
#[derive(Debug)]
pub struct TaskState {
  /// The shared templates usable by all tasks.
  templates: Rc<Templates>,
  /// The shared task database.
  tasks: Rc<Tasks>,
}

impl TaskState {
  /// Load `TaskState` from a directory.
  pub async fn load(tasks_root: &Path) -> Result<Self> {
    let task_state = load_tasks_from_dir(tasks_root).await.with_context(|| {
      format!(
        "failed to load tasks from directory {}",
        tasks_root.display()
      )
    })?;

    let templates = Templates::with_serde(task_state.tasks_meta.templates)
      .map_err(|id| anyhow!("encountered duplicate tag ID {}", id))?;
    let templates = Rc::new(templates);
    let tasks = Tasks::with_serde(task_state.tasks, templates.clone())
      .context("failed to instantiate task database")?;
    let tasks = Rc::new(tasks);

    let slf = Self { templates, tasks };
    Ok(slf)
  }

  /// Create a `TaskState` object from serialized state.
  pub fn with_serde(state: SerTaskState) -> Result<Self> {
    let templates = Templates::with_serde(state.tasks_meta.templates)
      .map_err(|id| anyhow!("encountered duplicate tag ID {}", id))?;
    let templates = Rc::new(templates);
    let tasks = Tasks::with_serde(state.tasks, templates.clone())
      .context("failed to instantiate task database")?;

    let slf = Self {
      templates,
      tasks: Rc::new(tasks),
    };
    Ok(slf)
  }

  /// Persist the state into a file.
  pub async fn save(&self, root_dir_cap: &mut DirCap) -> Result<()> {
    save_tasks_to_dir(root_dir_cap, &self.to_serde()).await
  }

  /// Retrieve the `Tasks` object associated with this `TaskState`
  /// object.
  pub fn tasks(&self) -> &Rc<Tasks> {
    &self.tasks
  }
}

impl ToSerde for TaskState {
  type Output = SerTaskState;

  /// Convert this object into a serializable one.
  fn to_serde(&self) -> Self::Output {
    SerTaskState {
      tasks_meta: SerTasksMeta {
        templates: self.templates.to_serde(),
      },
      tasks: self.tasks.to_serde(),
    }
  }
}


#[cfg(test)]
pub mod tests {
  use super::*;

  use std::env::temp_dir;

  use tempfile::TempDir;

  use tokio::fs::remove_dir_all;
  use tokio::test;

  use crate::ser::tags::Id as SerId;
  use crate::ser::tags::Tag as SerTag;
  use crate::ser::tags::Template as SerTemplate;
  use crate::ser::tags::Templates as SerTemplates;
  use crate::test::make_tasks;


  /// Create a `TaskState` object.
  fn make_task_state(tasks: Vec<SerTask>) -> TaskState {
    let task_state = SerTaskState {
      tasks_meta: Default::default(),
      tasks: SerTasks::from(tasks),
    };
    let task_state = TaskState::with_serde(task_state).unwrap();
    task_state
  }

  /// Create a `UiConfig` object.
  fn make_ui_config(task_count: usize) -> (UiConfig, TaskState) {
    let tasks = make_tasks(task_count);
    let task_state = make_task_state(tasks);

    let ui_config = Default::default();
    let ui_config = UiConfig::with_serde(ui_config, &task_state).unwrap();

    (ui_config, task_state)
  }

  /// Check that we can save tasks into a directory and load them back
  /// from there.
  #[test]
  async fn save_load_tasks() {
    async fn test(root: &Path, tasks: Vec<SerTask>, templates: Option<SerTemplates>) {
      let task_state = SerTaskState {
        tasks_meta: SerTasksMeta {
          templates: templates.unwrap_or_default(),
        },
        tasks: SerTasks::from(tasks),
      };
      let task_state = TaskState::with_serde(task_state).unwrap().to_serde();
      let mut tasks_root_cap = DirCap::for_dir(root.to_path_buf()).await.unwrap();
      let () = save_tasks_to_dir(&mut tasks_root_cap, &task_state)
        .await
        .unwrap();
      let mut loaded = load_tasks_from_dir(root).await.unwrap();

      // The order of tasks is undefined at this point of the loading
      // process. Sort them according to their position as is done
      // internally by `TaskState`.
      let () = loaded.tasks.0.sort_by(|first, second| {
        let first = first.position.unwrap_or(f64::MAX);
        let second = second.position.unwrap_or(f64::MAX);
        first.total_cmp(&second)
      });

      assert_eq!(loaded, task_state);
    }

    // Note that we use a single temporary directory here for all tests.
    // Doing so tests that the task saving logic removes files of tasks
    // that have been deleted.
    let root = TempDir::new().unwrap();
    let tasks = Vec::new();
    // Check that things work out even when no task is provided.
    let () = test(root.path(), tasks, None).await;

    let tasks = make_tasks(3);
    let () = test(root.path(), tasks, None).await;

    let id_tag = SerId::try_from(42).unwrap();
    let templates = SerTemplates(vec![SerTemplate {
      id: id_tag,
      name: "tag1".to_string(),
    }]);
    // Test with a task with a tag as well.
    let tasks = vec![SerTask::new("a task!").with_tags([SerTag { id: id_tag }])];
    let () = test(root.path(), tasks, Some(templates)).await;

    let tasks = make_tasks(25);
    // Make sure that directories not yet present are created.
    let () = test(
      &root.path().join("not").join("yet").join("present"),
      tasks,
      None,
    )
    .await;
  }

  /// Test that saving tasks and loading them back preserves their
  /// positions.
  #[test]
  async fn save_load_preserves_positions() {
    let root = TempDir::new().unwrap();
    let root = root.path();
    let mut task_vec = make_tasks(4);
    task_vec[0].position = Some(-42.0);
    task_vec[1].position = Some(0.0);
    task_vec[2].position = Some(10.0);
    task_vec[3].position = Some(10.001);

    let task_state = SerTaskState {
      tasks_meta: SerTasksMeta::default(),
      tasks: SerTasks::from(task_vec.clone()),
    };
    let task_state = TaskState::with_serde(task_state).unwrap();
    let mut tasks_root_cap = DirCap::for_dir(root.to_path_buf()).await.unwrap();
    let () = task_state.save(&mut tasks_root_cap).await.unwrap();

    let mut task_state = load_tasks_from_dir(root).await.unwrap();
    let () = task_state.tasks.0.sort_by(|first, second| {
      let first = first.position.unwrap_or(f64::MAX);
      let second = second.position.unwrap_or(f64::MAX);
      first.total_cmp(&second)
    });
    assert_eq!(task_state.tasks.0, task_vec);
  }

  /// Check that IDs are preserved when serializing and deserializing
  /// again.
  #[test]
  async fn loaded_tasks_use_saved_ids() {
    let root = TempDir::new().unwrap();
    let root = root.path();
    let tasks = make_tasks(15);

    let task_state = SerTaskState {
      tasks_meta: SerTasksMeta::default(),
      tasks: SerTasks::from(tasks),
    };
    let task_state = TaskState::with_serde(task_state).unwrap();

    let tasks = {
      let tasks = task_state.tasks();
      let (task1, task2, task3) = tasks.iter(|mut iter| {
        // Remove the first three tasks. If IDs were to not be preserved
        // on serialization, the IDs of these tasks would (likely) be
        // reassigned again to others on load and we would fail below.
        let task1 = iter.next().unwrap().clone();
        let task2 = iter.next().unwrap().clone();
        let task3 = iter.next().unwrap().clone();
        (task1, task2, task3)
      });

      let () = tasks.remove(task1);
      let () = tasks.remove(task2);
      let () = tasks.remove(task3);

      tasks.iter(|iter| iter.cloned().collect::<Vec<_>>())
    };

    let mut tasks_root_cap = DirCap::for_dir(root.to_path_buf()).await.unwrap();
    let () = task_state.save(&mut tasks_root_cap).await.unwrap();
    let task_state = load_tasks_from_dir(root).await.unwrap();
    let templates = Rc::new(Templates::with_serde(task_state.tasks_meta.templates).unwrap());
    let loaded = Tasks::with_serde(task_state.tasks, templates).unwrap();
    let loaded = loaded.iter(|iter| iter.cloned().collect::<Vec<_>>());
    let () = tasks
      .iter()
      .zip(loaded.iter())
      .map(|(task, loaded)| assert_eq!(task.id(), loaded.id()))
      .for_each(|_| ());
  }

  /// Check that `save_state_to_file` correctly creates non-existing
  /// directories.
  #[test]
  async fn create_dirs_for_state() {
    let base = temp_dir().join("dir1");
    let dir = base.join("dir2");
    let mut dir_cap = DirCap::for_dir(dir.clone()).await.unwrap();
    let file = OsStr::new("file");
    let write_guard = dir_cap.write().await.unwrap();
    let mut file_cap = write_guard.file_cap(file);

    let () = save_state_to_file::<Json, _>(&mut file_cap, &42)
      .await
      .unwrap();
    let mut file = File::open(dir.join(file)).await.unwrap();
    let mut content = Vec::new();
    let _count = file.read_to_end(&mut content).await.unwrap();
    let () = remove_dir_all(&base).await.unwrap();

    assert_eq!(content, b"42")
  }

  /// Check that we can save a `TaskState` and load it back.
  #[test]
  async fn save_and_load_task_state() {
    let task_vec = make_tasks(3);
    let task_state = make_task_state(task_vec.clone());

    let tasks_dir = TempDir::new().unwrap();
    let mut tasks_root_cap = DirCap::for_dir(tasks_dir.path().to_path_buf())
      .await
      .unwrap();
    let () = task_state.save(&mut tasks_root_cap).await.unwrap();

    let new_task_state = TaskState::load(tasks_dir.path()).await.unwrap();
    let new_task_vec = new_task_state.to_serde().tasks.into_task_vec();
    assert_eq!(new_task_vec, task_vec);
  }

  /// Check that we can save a `UiConfig` and load it back.
  #[test]
  async fn save_and_load_ui_config() {
    let (ui_config, task_state) = make_ui_config(3);
    let ui_file_dir = TempDir::new().unwrap();
    let ui_file_name = OsStr::new("config");
    let ui_file = ui_file_dir.path().join(ui_file_name);
    let mut ui_dir_cap = DirCap::for_dir(ui_file_dir.path().to_path_buf())
      .await
      .unwrap();
    let ui_write_guard = ui_dir_cap.write().await.unwrap();
    let mut ui_file_cap = ui_write_guard.file_cap(ui_file_name);
    let () = ui_config.save(&mut ui_file_cap).await.unwrap();

    let _new_ui_config = UiConfig::load(&ui_file, &task_state).await.unwrap();
  }

  /// Verify that loading a `TaskState` object succeeds even if the
  /// directory to load from is not present.
  #[test]
  async fn load_task_state_file_not_found() {
    let tasks_root = {
      let task_vec = make_tasks(1);
      let task_state = make_task_state(task_vec.clone());

      let tasks_dir = TempDir::new().unwrap();
      let mut tasks_root_cap = DirCap::for_dir(tasks_dir.path().to_path_buf())
        .await
        .unwrap();
      let () = task_state.save(&mut tasks_root_cap).await.unwrap();

      tasks_dir.path().to_path_buf()
    };

    // The files are removed by now, so we can test that both kinds of
    // state handle such missing files gracefully.
    let new_task_state = TaskState::load(&tasks_root).await.unwrap();
    let new_task_vec = new_task_state.to_serde().tasks.into_task_vec();
    assert_eq!(new_task_vec, Vec::new());
  }

  /// Verify that loading a `UiConfig` object succeeds
  /// even if the file to load from is not present.
  #[test]
  async fn load_ui_config_file_not_found() {
    let (ui_config, task_state) = {
      let (ui_config, task_state) = make_ui_config(1);

      let ui_file_dir = TempDir::new().unwrap();
      let ui_file_name = OsStr::new("config");
      let mut ui_dir_cap = DirCap::for_dir(ui_file_dir.path().to_path_buf())
        .await
        .unwrap();
      let ui_write_guard = ui_dir_cap.write().await.unwrap();
      let mut ui_file_cap = ui_write_guard.file_cap(ui_file_name);
      let () = ui_config.save(&mut ui_file_cap).await.unwrap();

      (ui_file_dir.path().join(ui_file_name), task_state)
    };

    let _new_ui_config = UiConfig::load(&ui_config, &task_state).await.unwrap();
  }

  /// Test that we fail `TaskState` construction when an invalid tag is
  /// encountered.
  #[test]
  async fn load_state_with_invalid_tag() {
    let tasks = vec![SerTask::new("a task!").with_tags([SerTag {
      id: SerId::try_from(42).unwrap(),
    }])];
    let task_state = SerTaskState {
      tasks_meta: Default::default(),
      tasks: SerTasks::from(tasks),
    };

    let err = TaskState::with_serde(task_state).unwrap_err();
    assert_eq!(
      err.root_cause().to_string(),
      "encountered invalid tag ID 42"
    )
  }

  /// Check that we can correctly instantiate a `TaskState` object from
  /// serialized state.
  #[test]
  async fn load_state() {
    let id_tag1 = SerId::try_from(29).unwrap();
    let id_tag2 = SerId::try_from(1337 + 42 - 1).unwrap();

    let templates = SerTemplates(vec![
      SerTemplate {
        id: id_tag1,
        name: "tag1".to_string(),
      },
      SerTemplate {
        id: id_tag2,
        name: "tag2".to_string(),
      },
    ]);

    let tasks = vec![
      SerTask::new("a task!").with_tags([SerTag { id: id_tag2 }]),
      SerTask::new("an untagged task"),
      SerTask::new("a tag1 task").with_tags([SerTag { id: id_tag1 }]),
      SerTask::new("a doubly tagged task")
        .with_tags([SerTag { id: id_tag2 }, SerTag { id: id_tag1 }]),
    ];
    let task_state = SerTaskState {
      tasks_meta: SerTasksMeta { templates },
      tasks: SerTasks::from(tasks),
    };

    let state = TaskState::with_serde(task_state).unwrap();
    let tasks = state.tasks;
    let vec = tasks.iter(|iter| iter.cloned().collect::<Vec<_>>());
    let mut it = vec.iter();

    let task1 = it.next().unwrap();
    let () = task1.tags(|mut iter| {
      assert_eq!(iter.next().unwrap().name(), "tag2");
      assert!(iter.next().is_none());
    });

    let task2 = it.next().unwrap();
    assert!(task2.tags(|mut iter| iter.next().is_none()));

    let task3 = it.next().unwrap();
    let () = task3.tags(|mut iter| {
      assert_eq!(iter.next().unwrap().name(), "tag1");
      assert!(iter.next().is_none());
    });

    let task4 = it.next().unwrap();
    let () = task4.tags(|mut iter| {
      assert!(iter.next().is_some());
      assert!(iter.next().is_some());
      assert!(iter.next().is_none());
    });
  }
}
