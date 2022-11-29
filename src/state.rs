// Copyright (C) 2017-2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

//! Definitions pertaining UI and task state of the program.

use std::cell::Cell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::create_dir_all;
use std::fs::remove_file;
use std::fs::DirEntry;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Error;
use std::io::ErrorKind;
use std::io::Result;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;

use cell::RefCell;

use serde::Deserialize;
use serde::Serialize;
use serde_json::from_reader;
use serde_json::to_string_pretty as to_json;

use crate::colors::Colors;
use crate::ser::state::TaskState as SerTaskState;
use crate::ser::state::UiState as SerUiState;
use crate::ser::tags::Templates as SerTemplates;
use crate::ser::tasks::Id as SerTaskId;
use crate::ser::tasks::Task as SerTask;
use crate::ser::tasks::Tasks as SerTasks;
use crate::ser::tasks::TasksMeta as SerTasksMeta;
use crate::ser::ToSerde;
use crate::tags::Templates;
use crate::tasks::Tasks;
use crate::view::View;
use crate::view::ViewBuilder;

/// The ID we use for storing task meta data.
// We use the reserved ID 0 for storing task meta data. Tasks are
// guaranteed to never use this ID.
const TASKS_META_ID: usize = 0;


/// Load some serialized state from a file.
fn load_state_from_file<T>(path: &Path) -> Result<T>
where
  T: Default,
  for<'de> T: Deserialize<'de>,
{
  match File::open(path) {
    Ok(file) => Ok(from_reader::<File, T>(file)?),
    Err(e) => {
      // If the file does not exist we create an empty object and work
      // with that.
      if e.kind() == ErrorKind::NotFound {
        Ok(Default::default())
      } else {
        Err(e)
      }
    },
  }
}

/// Load a task from a directory entry.
fn load_task_from_dir_entry(entry: &DirEntry) -> Result<(SerTaskId, SerTask)> {
  let file_name = entry.file_name();
  let path = entry.path();
  let id = file_name
    .to_str()
    .and_then(|id| id.parse::<SerTaskId>().ok())
    .ok_or_else(|| {
      let error = format!("Filename {} is not a valid ID", path.display());
      Error::new(ErrorKind::InvalidInput, error)
    })?;

  let file = File::open(&path)?;
  let task = from_reader(file)?;
  Ok((id, task))
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
          let error = format!("Encountered duplicate task ID {}", id);
          return Err(Error::new(ErrorKind::InvalidInput, error))
        }
        Ok(map)
      })?;

  Ok(table)
}

/// Load tasks from a directory.
///
/// The function assumes that the directory *only* contains files
/// representing tasks (along with one file for meta data).
fn load_tasks_from_dir(root: &Path) -> Result<SerTaskState> {
  let mut dir = match root.read_dir() {
    Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Default::default()),
    result => result,
  }?;

  // Ideally we'd size the `Vec` as per the number of directory entries,
  // but `fs::ReadDir` does not currently expose that number.
  let (mut tasks, tasks_meta) =
    dir.try_fold((Vec::new(), None), |(mut vec, tasks_meta), result| {
      let entry = result?;
      if entry.file_name().to_str().map(|id| id.parse::<usize>()) == Some(Ok(TASKS_META_ID)) {
        debug_assert_eq!(
          tasks_meta, None,
          "encountered multiple task meta data files"
        );
        let tasks_meta = load_state_from_file::<SerTasksMeta>(&entry.path())?;
        Result::Ok((vec, Some(tasks_meta)))
      } else {
        let data = load_task_from_dir_entry(&entry)?;
        let () = vec.push(data);
        Result::Ok((vec, tasks_meta))
      }
    })?;

  let tasks_meta = tasks_meta.unwrap_or_default();
  let table = create_task_lookup_table(&tasks_meta.ids)?;
  // If a task ID is not contained in our table we will just silently
  // sort it last.
  tasks.sort_by_key(|(id, _)| table.get(id).copied().unwrap_or(usize::MAX));

  Ok(SerTaskState {
    templates: tasks_meta.templates.clone(),
    tasks_meta,
    tasks: SerTasks(tasks.into_iter().map(|(_id, task)| task).collect()),
  })
}

/// Save some state into a file.
fn save_state_to_file<T>(path: &Path, state: &T) -> Result<()>
where
  T: Serialize,
{
  if let Some(dir) = path.parent() {
    let () = create_dir_all(dir)?;
  }

  let serialized = to_json(state)?;
  OpenOptions::new()
    .create(true)
    .truncate(true)
    .write(true)
    .open(path)?
    .write_all(serialized.as_ref())?;
  Ok(())
}

/// Save a task into a file in the given directory.
fn save_task_to_file(root: &Path, id: SerTaskId, task: &SerTask) -> Result<()> {
  let path = root.join(id.to_string());
  // TODO: It would be better if we were create a temporary file first
  //       if one already exists and then rename atomically. But even
  //       nicer would be if we somehow wrapped all saving in a
  //       transaction of sorts. That would allow us to eliminate the
  //       chance for *any* inconsistency, e.g., when saving UI state
  //       before task state and the latter failing the operation.
  save_state_to_file(&path, task)
}

/// Save a task meta data into a file in the provided directory.
fn save_tasks_meta_to_dir(root: &Path, tasks_meta: &SerTasksMeta) -> Result<()> {
  let path = root.join(TASKS_META_ID.to_string());
  save_state_to_file(&path, tasks_meta)
}

/// Save tasks into files in the provided directory.
fn save_tasks_to_dir(root: &Path, tasks: &SerTaskState) -> Result<()> {
  let task_iter = tasks.tasks.0.iter();
  let id_iter = tasks.tasks_meta.ids.iter().copied();

  let () = id_iter
    .clone()
    .zip(task_iter)
    .try_for_each(|(id, task)| save_task_to_file(root, id, task))?;

  let () = save_tasks_meta_to_dir(root, &tasks.tasks_meta)?;

  let ids = id_iter.map(|id| id.get()).collect::<HashSet<_>>();
  // Remove all files that do not correspond to an ID we just saved.
  root.read_dir()?.try_for_each(|result| {
    let entry = result?;
    let id = entry
      .file_name()
      .to_str()
      .and_then(|id| id.parse::<usize>().ok());

    let remove = if let Some(id) = id {
      id != TASKS_META_ID && ids.get(&id).is_none()
    } else {
      true
    };

    if remove {
      // Note that we purposefully do not support the case of having a
      // directory inside the root directory, as we'd never create one
      // there programmatically.
      remove_file(entry.path())
    } else {
      Ok(())
    }
  })
}


/// A struct encapsulating the UI's state.
#[derive(Debug)]
pub struct UiState {
  /// The path to the file in which to save the state.
  pub path: PathBuf,
  /// The configured colors.
  pub colors: Cell<Option<Colors>>,
  /// The views used in the UI.
  pub views: Vec<(View, Option<usize>)>,
  /// The currently selected `View`.
  pub selected: Option<usize>,
}

impl UiState {
  /// Persist the state into a file.
  pub fn save(&self) -> Result<()> {
    let ui_state = load_state_from_file::<SerUiState>(self.path.as_ref()).unwrap_or_default();
    self.colors.set(Some(ui_state.colors));

    save_state_to_file(&self.path, &self.to_serde())
  }
}

impl ToSerde<SerUiState> for UiState {
  /// Convert this object into a serializable one.
  fn to_serde(&self) -> SerUiState {
    debug_assert!(self.selected.is_none() || self.selected.unwrap() < self.views.len());

    let views = self.views.iter().map(|(q, s)| (q.to_serde(), *s)).collect();

    SerUiState {
      colors: self.colors.get().unwrap_or_default(),
      views,
      selected: self.selected,
    }
  }
}


/// A struct encapsulating the task state of the program.
#[derive(Debug)]
pub struct TaskState {
  path: PathBuf,
  templates: Rc<Templates>,
  tasks_root: PathBuf,
  tasks: Rc<RefCell<Tasks>>,
}

impl TaskState {
  /// Create a `TaskState` object from serialized state.
  ///
  /// This constructor is intended for testing purposes.
  #[allow(unused)]
  fn with_serde(root: &Path, tasks: SerTasks, templates: SerTemplates) -> Self {
    let (templates, map) = Templates::with_serde(templates);
    let templates = Rc::new(templates);
    let tasks = Tasks::with_serde(tasks, templates.clone(), &map).unwrap();
    Self {
      path: PathBuf::default(),
      templates,
      tasks_root: root.into(),
      tasks: Rc::new(RefCell::new(tasks)),
    }
  }

  /// Persist the state into a file.
  pub fn save(&self) -> Result<()> {
    save_tasks_to_dir(&self.tasks_root, &self.to_serde())
  }

  /// Retrieve the `Tasks` object associated with this `State` object.
  pub fn tasks(&self) -> Rc<RefCell<Tasks>> {
    self.tasks.clone()
  }
}

impl ToSerde<SerTaskState> for TaskState {
  /// Convert this object into a serializable one.
  fn to_serde(&self) -> SerTaskState {
    let ids = self
      .tasks
      .borrow()
      .iter()
      .map(|(id, _)| SerTaskId::new(id.get()))
      .collect();

    SerTaskState {
      tasks_meta: SerTasksMeta {
        templates: self.templates.to_serde(),
        ids,
      },
      templates: self.templates.to_serde(),
      tasks: self.tasks.borrow().to_serde(),
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
  pub fn new<P>(ui_config: P, task_config: P, tasks_root: P) -> Result<Self>
  where
    P: Into<PathBuf> + AsRef<Path>,
  {
    let ui_state = load_state_from_file::<SerUiState>(ui_config.as_ref())?;
    let task_state = load_tasks_from_dir(tasks_root.as_ref())?;

    Self::with_serde(ui_state, ui_config, task_state, task_config, tasks_root)
  }

  /// Create a new `State` object from a serializable one.
  pub fn with_serde<P>(
    ui_state: SerUiState,
    ui_config: P,
    task_state: SerTaskState,
    task_config: P,
    tasks_root: P,
  ) -> Result<Self>
  where
    P: Into<PathBuf>,
  {
    let (templates, map) = Templates::with_serde(task_state.tasks_meta.templates);
    let templates = Rc::new(templates);
    let tasks = Tasks::with_serde(task_state.tasks, templates.clone(), &map)?;
    let tasks = Rc::new(RefCell::new(tasks));
    let mut views = Vec::new();
    for (view, selected) in ui_state.views.into_iter() {
      let view = View::with_serde(view, &templates, &map, tasks.clone())?;
      views.push((view, selected))
    }
    // For convenience for the user, we add a default view capturing
    // all tasks if no other views have been configured.
    if views.is_empty() {
      views.push((ViewBuilder::new(tasks.clone()).build("all"), None))
    }

    let ui_state = UiState {
      colors: Cell::new(Some(ui_state.colors)),
      path: ui_config.into(),
      views,
      selected: ui_state.selected,
    };
    let task_state = TaskState {
      path: task_config.into(),
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
  use std::fs::remove_dir_all;
  use std::fs::File;
  use std::io::Read;

  use tempfile::NamedTempFile;
  use tempfile::TempDir;

  use crate::ser::tags::Id as SerId;
  use crate::ser::tags::Tag as SerTag;
  use crate::ser::tags::Template as SerTemplate;
  use crate::test::make_tasks;


  /// Create a `State` object based off of temporary configuration data.
  fn make_state(count: usize) -> (State, NamedTempFile, NamedTempFile, TempDir) {
    let task_state = SerTaskState {
      tasks_meta: Default::default(),
      templates: Default::default(),
      tasks: SerTasks(make_tasks(count)),
    };
    let ui_state = Default::default();
    let ui_file = NamedTempFile::new().unwrap();
    let task_file = NamedTempFile::new().unwrap();
    let tasks_dir = TempDir::new().unwrap();
    let state = State::with_serde(
      ui_state,
      ui_file.path(),
      task_state,
      task_file.path(),
      tasks_dir.path(),
    );
    (state.unwrap(), ui_file, task_file, tasks_dir)
  }

  /// Check that we can save tasks into a directory and load them back
  /// from there.
  #[test]
  fn save_load_tasks() {
    fn test(root: &Path, tasks: Vec<SerTask>, templates: Option<SerTemplates>) {
      let templates = templates.unwrap_or_default();
      let tasks = SerTasks(tasks);
      let task_state = TaskState::with_serde(root, tasks, templates).to_serde();
      let () = save_tasks_to_dir(root, &task_state).unwrap();
      let loaded = load_tasks_from_dir(root).unwrap();
      assert_eq!(loaded, task_state);
    }

    // Note that we use a single temporary directory here for all tests.
    // Doing so tests that the task saving logic removes files of tasks
    // that have been deleted.
    let root = TempDir::new().unwrap();
    let tasks = Vec::new();
    // Check that things work out even when no task is provided.
    test(root.path(), tasks, None);

    let tasks = make_tasks(3);
    test(root.path(), tasks, None);

    let id_tag = SerId::try_from(42).unwrap();
    let templates = SerTemplates(vec![SerTemplate {
      id: id_tag,
      name: "tag1".to_string(),
    }]);
    // Test with a task with a tag as well.
    let tasks = vec![SerTask::new("a task!").with_tags([SerTag { id: id_tag }])];
    test(root.path(), tasks, Some(templates));

    let tasks = make_tasks(25);
    // Make sure that directories not yet present are created.
    test(
      &root.path().join("not").join("yet").join("present"),
      tasks,
      None,
    );
  }

  #[test]
  fn create_dirs_for_state() {
    let base = temp_dir().join("dir1");
    let path = base.join("dir2").join("file");

    save_state_to_file(&path, &42).unwrap();
    let mut file = File::open(path).unwrap();
    let mut content = Vec::new();
    file.read_to_end(&mut content).unwrap();
    remove_dir_all(&base).unwrap();

    assert_eq!(content, b"42")
  }

  #[test]
  fn save_and_load_state() {
    let (state, ui_file, task_file, tasks_root) = make_state(3);
    state.0.save().unwrap();
    state.1.save().unwrap();

    let new_state = State::new(ui_file.path(), task_file.path(), tasks_root.path()).unwrap();
    let new_task_vec = new_state
      .1
      .tasks
      .borrow()
      .iter()
      .map(|(_, task)| task.to_serde())
      .collect::<Vec<_>>();
    assert_eq!(new_task_vec, make_tasks(3));
  }

  #[test]
  fn load_state_file_not_found() {
    let (ui_config, task_config, tasks_root) = {
      let (state, ui_file, task_file, tasks_dir) = make_state(1);
      state.0.save().unwrap();
      state.1.save().unwrap();

      (
        ui_file.path().to_path_buf(),
        task_file.path().to_path_buf(),
        tasks_dir.path().to_path_buf(),
      )
    };

    // The files are removed by now, so we can test that `State` handles
    // such missing files gracefully.
    let new_state = State::new(ui_config, task_config, tasks_root).unwrap();
    let new_task_vec = new_state
      .1
      .tasks
      .borrow()
      .iter()
      .map(|(_, task)| task.to_serde())
      .collect::<Vec<_>>();
    assert_eq!(new_task_vec, make_tasks(0));
  }

  #[test]
  fn load_state_with_invalid_tag() {
    let tasks = SerTasks(vec![SerTask::new("a task!").with_tags([SerTag {
      id: SerId::try_from(42).unwrap(),
    }])]);
    let ui_state = Default::default();
    let ui_config = PathBuf::default();
    let task_state = SerTaskState {
      tasks_meta: Default::default(),
      templates: Default::default(),
      tasks,
    };
    let task_config = PathBuf::default();
    let tasks_root = PathBuf::default();

    let err =
      State::with_serde(ui_state, ui_config, task_state, task_config, tasks_root).unwrap_err();
    assert_eq!(err.to_string(), "Encountered invalid tag Id 42")
  }

  #[test]
  fn load_state() {
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

    let tasks = SerTasks(vec![
      SerTask::new("a task!").with_tags([SerTag { id: id_tag2 }]),
      SerTask::new("an untagged task"),
      SerTask::new("a tag1 task").with_tags([SerTag { id: id_tag1 }]),
      SerTask::new("a doubly tagged task")
        .with_tags([SerTag { id: id_tag2 }, SerTag { id: id_tag1 }]),
    ]);
    let task_state = SerTaskState {
      tasks_meta: SerTasksMeta {
        templates: templates.clone(),
        ids: Default::default(),
      },
      templates,
      tasks,
    };
    let task_config = PathBuf::default();
    let tasks_root = PathBuf::default();

    let state =
      State::with_serde(ui_state, ui_config, task_state, task_config, tasks_root).unwrap();
    let tasks = state.1.tasks.borrow();
    let mut it = tasks.iter().map(|(_, task)| task);

    let task1 = it.next().unwrap();
    let mut tags = task1.tags();
    assert_eq!(tags.next().unwrap().name(), "tag2");
    assert!(tags.next().is_none());

    let task2 = it.next().unwrap();
    assert!(task2.tags().next().is_none());

    let task3 = it.next().unwrap();
    let mut tags = task3.tags();
    assert_eq!(tags.next().unwrap().name(), "tag1");
    assert!(tags.next().is_none());

    let task4 = it.next().unwrap();
    let mut tags = task4.tags();
    assert!(tags.next().is_some());
    assert!(tags.next().is_some());
    assert!(tags.next().is_none());
  }
}
