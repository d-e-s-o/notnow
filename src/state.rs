// Copyright (C) 2017-2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

//! Definitions pertaining UI and task state of the program.

use std::cell::Cell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::anyhow;
use anyhow::bail;
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

use crate::colors::Colors;
use crate::ser::state::TaskState as SerTaskState;
use crate::ser::state::UiState as SerUiState;
use crate::ser::tags::Templates as SerTemplates;
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

/// Load task meta data from the provided file.
fn create_task_lookup_table(ids: &[SerTaskId]) -> Result<HashMap<SerTaskId, usize>> {
  let len = ids.len();
  let table =
    ids
      .iter()
      .enumerate()
      .try_fold(HashMap::with_capacity(len), |mut map, (idx, id)| {
        if map.insert(*id, idx).is_some() {
          bail!("encountered duplicate task ID {}", id)
        }
        Ok(map)
      })?;

  Ok(table)
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

  let (mut tasks, tasks_meta) = load_tasks_from_read_dir(dir).await?;
  let tasks_meta = tasks_meta.unwrap_or_default();
  let table = create_task_lookup_table(&tasks_meta.ids)?;
  // If a task ID is not contained in our table we will just silently
  // sort it last.
  tasks.sort_by_key(|task| table.get(&task.id).copied().unwrap_or(usize::MAX));

  Ok(SerTaskState {
    tasks_meta,
    tasks: SerTasks(tasks),
  })
}

/// Save some state into a file.
async fn save_state_to_file<B, T>(path: &Path, state: &T) -> Result<()>
where
  B: Backend<T>,
  T: PartialEq,
{
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
  let () = OpenOptions::new()
    .create(true)
    .truncate(true)
    .write(true)
    .open(path)
    .await?
    .write_all(serialized.as_ref())
    .await?;
  Ok(())
}

/// Save a task into a file in the given directory.
async fn save_task_to_file(root: &Path, task: &SerTask) -> Result<()> {
  let path = root.join(task.id.to_string());
  // TODO: It would be better if we were create a temporary file first
  //       if one already exists and then rename atomically. But even
  //       nicer would be if we somehow wrapped all saving in a
  //       transaction of sorts. That would allow us to eliminate the
  //       chance for *any* inconsistency, e.g., when saving UI state
  //       before task state and the latter failing the operation.
  save_state_to_file::<iCal, _>(&path, task).await
}

/// Save a task meta data into a file in the provided directory.
async fn save_tasks_meta_to_dir(root: &Path, tasks_meta: &SerTasksMeta) -> Result<()> {
  let path = root.join(TASKS_META_ID.to_string());
  save_state_to_file::<iCal, _>(&path, tasks_meta).await
}

/// Save tasks into files in the provided directory.
async fn save_tasks_to_dir(root: &Path, tasks: &SerTaskState) -> Result<()> {
  for task in tasks.tasks.0.iter() {
    let () = save_task_to_file(root, task).await?;
  }

  let () = save_tasks_meta_to_dir(root, &tasks.tasks_meta).await?;
  let ids = tasks.tasks_meta.ids.iter().collect::<HashSet<_>>();

  // Remove all files that do not correspond to an ID we just saved.
  let mut dir = read_dir(root).await?;
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


/// A struct encapsulating the UI's state.
#[derive(Debug)]
pub struct UiState {
  /// The path to the file in which to save the state.
  pub path: PathBuf,
  /// The configured colors.
  pub colors: Cell<Option<Colors>>,
  /// The tag to toggle on user initiated action.
  pub toggle_tag: Option<Tag>,
  /// The views used in the UI.
  pub views: Vec<(View, Option<usize>)>,
  /// The currently selected `View`.
  pub selected: Option<usize>,
}

impl UiState {
  /// Persist the state into a file.
  pub async fn save(&self) -> Result<()> {
    let ui_state = load_state_from_file::<Json, SerUiState>(self.path.as_ref())
      .await
      .unwrap_or_default()
      .unwrap_or_default();
    self.colors.set(Some(ui_state.colors));

    save_state_to_file::<Json, _>(&self.path, &self.to_serde()).await
  }
}

impl ToSerde<SerUiState> for UiState {
  /// Convert this object into a serializable one.
  fn to_serde(&self) -> SerUiState {
    debug_assert!(self.selected.is_none() || self.selected.unwrap() < self.views.len());

    let views = self.views.iter().map(|(q, s)| (q.to_serde(), *s)).collect();

    SerUiState {
      colors: self.colors.get().unwrap_or_default(),
      toggle_tag: self.toggle_tag.as_ref().map(|x| x.to_serde()),
      views,
      selected: self.selected,
    }
  }
}


/// A struct encapsulating the task state of the program.
#[derive(Debug)]
pub struct TaskState {
  templates: Rc<Templates>,
  tasks_root: PathBuf,
  tasks: Rc<Tasks>,
}

impl TaskState {
  /// Create a `TaskState` object from serialized state.
  ///
  /// This constructor is intended for testing purposes.
  #[allow(unused)]
  fn with_serde(root: &Path, tasks: SerTasks, templates: SerTemplates) -> Self {
    let templates = Rc::new(Templates::with_serde(templates).unwrap());
    let tasks = Tasks::with_serde(tasks, templates.clone()).unwrap();
    Self {
      templates,
      tasks_root: root.into(),
      tasks: Rc::new(tasks),
    }
  }

  /// Persist the state into a file.
  pub async fn save(&self) -> Result<()> {
    save_tasks_to_dir(&self.tasks_root, &self.to_serde()).await
  }

  /// Retrieve the `Tasks` object associated with this `State` object.
  pub fn tasks(&self) -> Rc<Tasks> {
    self.tasks.clone()
  }
}

impl ToSerde<SerTaskState> for TaskState {
  /// Convert this object into a serializable one.
  fn to_serde(&self) -> SerTaskState {
    let tasks = self.tasks.to_serde();
    let ids = tasks.0.iter().map(|task| task.id).collect();

    SerTaskState {
      tasks_meta: SerTasksMeta {
        templates: self.templates.to_serde(),
        ids,
      },
      tasks,
    }
  }
}


/// A struct combining the UI and task state.
///
/// The struct exists mainly to express the dependency between the
/// `UiState` and `TaskState` structs in terms of their creation. Most
/// of the time the object will be destructed later on and the
/// individual state objects be used on their own.
#[derive(Debug)]
pub struct State(pub UiState, pub TaskState);

impl State {
  /// Create a new `State` object, loaded from files.
  pub async fn new<P>(ui_config: P, tasks_root: P) -> Result<Self>
  where
    P: Into<PathBuf> + AsRef<Path>,
  {
    let ui_state = load_state_from_file::<Json, SerUiState>(ui_config.as_ref())
      .await
      .with_context(|| {
        format!(
          "failed to load UI state from {}",
          ui_config.as_ref().display()
        )
      })?
      .unwrap_or_default();
    let task_state = load_tasks_from_dir(tasks_root.as_ref())
      .await
      .with_context(|| {
        format!(
          "failed to load tasks from directory {}",
          tasks_root.as_ref().display()
        )
      })?;

    Self::with_serde(ui_state, ui_config, task_state, tasks_root)
  }

  /// Create a new `State` object from a serializable one.
  pub fn with_serde<P>(
    ui_state: SerUiState,
    ui_config: P,
    task_state: SerTaskState,
    tasks_root: P,
  ) -> Result<Self>
  where
    P: Into<PathBuf>,
  {
    let templates = Templates::with_serde(task_state.tasks_meta.templates)
      .map_err(|id| anyhow!("encountered duplicate tag ID {}", id))?;
    let templates = Rc::new(templates);
    let tasks = Tasks::with_serde(task_state.tasks, templates.clone())
      .context("failed to instantiate task database")?;
    let tasks = Rc::new(tasks);
    let mut views = ui_state
      .views
      .into_iter()
      .map(|(view, selected)| {
        let name = view.name.clone();
        let view = View::with_serde(view, &templates, tasks.clone())
          .with_context(|| format!("failed to instantiate view '{}'", name))?;
        Ok((view, selected))
      })
      .collect::<Result<Vec<_>>>()?;

    // For convenience for the user, we add a default view capturing
    // all tasks if no other views have been configured.
    if views.is_empty() {
      views.push((ViewBuilder::new(tasks.clone()).build("all"), None))
    }

    let toggle_tag = if let Some(toggle_tag) = ui_state.toggle_tag {
      let toggle_tag = templates
        .instantiate(toggle_tag.id)
        .ok_or_else(|| anyhow!("encountered invalid toggle tag ID {}", toggle_tag.id))?;

      Some(toggle_tag)
    } else {
      None
    };

    let ui_state = UiState {
      colors: Cell::new(Some(ui_state.colors)),
      toggle_tag,
      path: ui_config.into(),
      views,
      selected: ui_state.selected,
    };
    let task_state = TaskState {
      templates,
      tasks_root: tasks_root.into(),
      tasks,
    };
    Ok(Self(ui_state, task_state))
  }
}


#[cfg(test)]
pub mod tests {
  use super::*;

  use std::env::temp_dir;

  use tempfile::NamedTempFile;
  use tempfile::TempDir;

  use tokio::fs::remove_dir_all;
  use tokio::test;

  use crate::ser::tags::Id as SerId;
  use crate::ser::tags::Tag as SerTag;
  use crate::ser::tags::Template as SerTemplate;
  use crate::test::make_tasks;


  /// Create a `State` object based off of temporary configuration data.
  fn make_state(tasks: Vec<SerTask>) -> (State, NamedTempFile, TempDir) {
    let task_state = SerTaskState {
      tasks_meta: Default::default(),
      tasks: SerTasks::from(tasks),
    };
    let ui_state = Default::default();
    let ui_file = NamedTempFile::new().unwrap();
    let tasks_dir = TempDir::new().unwrap();
    let state = State::with_serde(ui_state, ui_file.path(), task_state, tasks_dir.path());
    (state.unwrap(), ui_file, tasks_dir)
  }

  /// Check that we can save tasks into a directory and load them back
  /// from there.
  #[test]
  async fn save_load_tasks() {
    async fn test(root: &Path, tasks: Vec<SerTask>, templates: Option<SerTemplates>) {
      let templates = templates.unwrap_or_default();
      let tasks = SerTasks::from(tasks);
      let task_state = TaskState::with_serde(root, tasks, templates).to_serde();
      let () = save_tasks_to_dir(root, &task_state).await.unwrap();
      let loaded = load_tasks_from_dir(root).await.unwrap();
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

  /// Check that IDs are preserved when serializing and deserializing
  /// again.
  #[test]
  async fn loaded_tasks_use_saved_ids() {
    let root = TempDir::new().unwrap();
    let root = root.path();
    let tasks = make_tasks(15);
    let templates = SerTemplates::default();
    let tasks = SerTasks::from(tasks);
    let task_state = TaskState::with_serde(root, tasks, templates);

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

    let () = task_state.save().await.unwrap();
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
    let path = base.join("dir2").join("file");

    let () = save_state_to_file::<Json, _>(&path, &42).await.unwrap();
    let mut file = File::open(path).await.unwrap();
    let mut content = Vec::new();
    let _count = file.read_to_end(&mut content).await.unwrap();
    let () = remove_dir_all(&base).await.unwrap();

    assert_eq!(content, b"42")
  }

  /// Check that we can save `State` and load it back.
  #[test]
  async fn save_and_load_state() {
    let task_vec = make_tasks(3);
    let (state, ui_file, tasks_root) = make_state(task_vec.clone());
    state.0.save().await.unwrap();
    state.1.save().await.unwrap();

    let new_state = State::new(ui_file.path(), tasks_root.path()).await.unwrap();
    let new_task_vec = new_state.1.to_serde().tasks.into_task_vec();
    assert_eq!(new_task_vec, task_vec);
  }

  /// Verify that loading `State` succeeds even if the file to load from
  /// is not present.
  #[test]
  async fn load_state_file_not_found() {
    let (ui_config, tasks_root) = {
      let task_vec = make_tasks(1);
      let (state, ui_file, tasks_dir) = make_state(task_vec);
      state.0.save().await.unwrap();
      state.1.save().await.unwrap();

      (ui_file.path().to_path_buf(), tasks_dir.path().to_path_buf())
    };

    // The files are removed by now, so we can test that `State` handles
    // such missing files gracefully.
    let new_state = State::new(ui_config, tasks_root).await.unwrap();
    let new_task_vec = new_state.1.to_serde().tasks.into_task_vec();
    assert_eq!(new_task_vec, Vec::new());
  }

  /// Test that we fail `State` construction when an invalid tag is
  /// encountered.
  #[test]
  async fn load_state_with_invalid_tag() {
    let tasks = vec![SerTask::new("a task!").with_tags([SerTag {
      id: SerId::try_from(42).unwrap(),
    }])];
    let ui_state = Default::default();
    let ui_config = PathBuf::default();
    let task_state = SerTaskState {
      tasks_meta: Default::default(),
      tasks: SerTasks::from(tasks),
    };
    let tasks_root = PathBuf::default();

    let err = State::with_serde(ui_state, ui_config, task_state, tasks_root).unwrap_err();
    assert_eq!(
      err.root_cause().to_string(),
      "encountered invalid tag ID 42"
    )
  }

  /// Check that we can correctly instantiate a `State` object from
  /// serialized state.
  #[test]
  async fn load_state() {
    let ui_state = Default::default();
    let ui_config = PathBuf::default();

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
      tasks_meta: SerTasksMeta {
        templates,
        ids: Default::default(),
      },
      tasks: SerTasks::from(tasks),
    };
    let tasks_root = PathBuf::default();

    let state = State::with_serde(ui_state, ui_config, task_state, tasks_root).unwrap();
    let tasks = state.1.tasks;
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
