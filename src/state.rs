// state.rs

// *************************************************************************
// * Copyright (C) 2017-2018 Daniel Mueller (deso@posteo.net)              *
// *                                                                       *
// * This program is free software: you can redistribute it and/or modify  *
// * it under the terms of the GNU General Public License as published by  *
// * the Free Software Foundation, either version 3 of the License, or     *
// * (at your option) any later version.                                   *
// *                                                                       *
// * This program is distributed in the hope that it will be useful,       *
// * but WITHOUT ANY WARRANTY; without even the implied warranty of        *
// * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the         *
// * GNU General Public License for more details.                          *
// *                                                                       *
// * You should have received a copy of the GNU General Public License     *
// * along with this program.  If not, see <http://www.gnu.org/licenses/>. *
// *************************************************************************

use std::fs::File;
use std::fs::OpenOptions;
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

use crate::query::Query;
use crate::query::QueryBuilder;
use crate::ser::state::TaskState as SerTaskState;
use crate::ser::state::UiState as SerUiState;
use crate::ser::ToSerde;
use crate::tags::Templates;
use crate::tasks::Tasks;


/// Load some serialized state from a file.
fn load_state<T>(path: &Path) -> Result<T>
where
  T: Default,
  for<'de> T: Deserialize<'de>,
{
  match File::open(&path) {
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

/// Save some state into a file.
fn save_state<T>(path: &Path, state: T) -> Result<()>
where
  T: Serialize,
{
  let serialized = to_json(&state)?;
  OpenOptions::new()
    .create(true)
    .truncate(true)
    .write(true)
    .open(path)?
    .write_all(serialized.as_ref())?;
  Ok(())
}


/// A struct encapsulating the task state of the program.
#[derive(Debug)]
pub struct TaskState {
  path: PathBuf,
  templates: Rc<Templates>,
  tasks: Rc<RefCell<Tasks>>,
}

impl TaskState {
  /// Persist the state into a file.
  pub fn save(&self) -> Result<()> {
    save_state(&self.path, self.to_serde())
  }

  /// Retrieve the `Tasks` object associated with this `State` object.
  pub fn tasks(&self) -> Rc<RefCell<Tasks>> {
    self.tasks.clone()
  }
}

impl ToSerde<SerTaskState> for TaskState {
  /// Convert this object into a serializable one.
  fn to_serde(&self) -> SerTaskState {
    SerTaskState {
      templates: self.templates.to_serde(),
      tasks: self.tasks.borrow().to_serde(),
    }
  }
}


/// A struct encapsulating the UI's state.
#[derive(Debug)]
pub struct UiState {
  path: PathBuf,
  queries: Vec<Query>,
}

impl UiState {
  /// Persist the state into a file.
  pub fn save(&self) -> Result<()> {
    save_state(&self.path, self.to_serde())
  }

  /// Retrieve the queries to use.
  pub fn queries(&self) -> impl Iterator<Item=&Query> {
    self.queries.iter()
  }
}

impl ToSerde<SerUiState> for UiState {
  /// Convert this object into a serializable one.
  fn to_serde(&self) -> SerUiState {
    let queries = self
      .queries
      .iter()
      .map(|x| x.to_serde())
      .collect();

    SerUiState {
      queries: queries,
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
  pub fn new<P>(ui_path: P, task_path: P) -> Result<Self>
  where
    P: Into<PathBuf> + AsRef<Path>,
  {
    let ui_state = load_state::<SerUiState>(ui_path.as_ref())?;
    let task_state = load_state::<SerTaskState>(task_path.as_ref())?;

    Self::with_serde(ui_state, ui_path, task_state, task_path)
  }

  /// Create a new `State` object from a serializable one.
  pub fn with_serde<P>(ui_state: SerUiState, ui_path: P,
                       task_state: SerTaskState, task_path: P) -> Result<Self>
  where
    P: Into<PathBuf>,
  {
    let (templates, map) = Templates::with_serde(task_state.templates);
    let templates = Rc::new(templates);
    let tasks = Tasks::with_serde(task_state.tasks, templates.clone(), &map)?;
    let tasks = Rc::new(RefCell::new(tasks));
    let mut queries = Vec::new();
    for query in ui_state.queries.into_iter() {
      queries.push(Query::with_serde(query, &templates, &map, tasks.clone())?)
    }
    // For convenience for the user, we add a default query capturing
    // all tasks if no other queries have been configured.
    if queries.is_empty() {
      queries.push(QueryBuilder::new(tasks.clone()).build("all"))
    }

    let task_state = TaskState {
      path: task_path.into(),
      templates: templates,
      tasks: tasks,
    };
    let ui_state = UiState {
      path: ui_path.into(),
      queries: queries,
    };
    Ok(State(ui_state, task_state))
  }
}


#[cfg(test)]
pub mod tests {
  use super::*;

  use crate::ser::tags::Id as SerId;
  use crate::ser::tags::Tag as SerTag;
  use crate::ser::tags::Template as SerTemplate;
  use crate::ser::tags::Templates as SerTemplates;
  use crate::ser::tasks::Task as SerTask;
  use crate::ser::tasks::Tasks as SerTasks;
  use crate::test::make_tasks;
  use crate::test::NamedTempFile;


  /// Create a state object based off of two temporary configuration files.
  fn make_state(count: usize) -> (State, NamedTempFile, NamedTempFile) {
    let ui_state = Default::default();
    let task_state = SerTaskState {
      templates: Default::default(),
      tasks: SerTasks(make_tasks(count)),
    };
    let ui_file = NamedTempFile::new();
    let task_file = NamedTempFile::new();
    let state = State::with_serde(ui_state, ui_file.path(), task_state, task_file.path());
    (state.unwrap(), ui_file, task_file)
  }

  #[test]
  fn save_and_load_state() {
    let (state, ui_file, task_file) = make_state(3);
    state.0.save().unwrap();
    state.1.save().unwrap();

    let new_state = State::new(ui_file.path(), task_file.path()).unwrap();
    let new_task_vec = new_state
      .1
      .tasks
      .borrow()
      .iter()
      .map(|x| x.to_serde())
      .collect::<Vec<_>>();
    assert_eq!(new_task_vec, make_tasks(3));
  }

  #[test]
  fn load_state_file_not_found() {
    let (ui_path, task_path) = {
      let (state, ui_file, task_file) = make_state(1);
      state.0.save().unwrap();
      state.1.save().unwrap();

      (ui_file.path().clone(), task_file.path().clone())
    };

    // The files are removed by now, so we can test that `State` handles
    // such missing files gracefully.
    let new_state = State::new(ui_path, task_path).unwrap();
    let new_task_vec = new_state
      .1
      .tasks
      .borrow()
      .iter()
      .map(|x| x.to_serde())
      .collect::<Vec<_>>();
    assert_eq!(new_task_vec, make_tasks(0));
  }

  #[test]
  fn load_state_with_invalid_tag() {
    let ui_state = Default::default();
    let ui_path = PathBuf::default();
    let templates = SerTemplates(Default::default());
    let tasks = SerTasks(vec![
      SerTask {
        summary: "a task!".to_string(),
        tags: vec![
          SerTag {
            id: SerId::new(42),
          },
        ],
      },
    ]);
    let task_state = SerTaskState {
      templates: templates,
      tasks: tasks,
    };
    let task_path = PathBuf::default();

    let err = State::with_serde(ui_state, ui_path, task_state, task_path).unwrap_err();
    assert_eq!(err.to_string(), "Encountered invalid tag Id 42")
  }

  #[test]
  fn load_state() {
    let ui_state = Default::default();
    let ui_path = PathBuf::default();

    let id_tag1 = SerId::new(29);
    let id_tag2 = SerId::new(1337 + 42 - 1);

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
      SerTask {
        summary: "a task!".to_string(),
        tags: vec![
          SerTag {
            id: id_tag2,
          },
        ],
      },
      SerTask {
        summary: "an untagged task".to_string(),
        tags: Default::default(),
      },
      SerTask {
        summary: "a tag1 task".to_string(),
        tags: vec![
          SerTag {
            id: id_tag1,
          },
        ],
      },
      SerTask {
        summary: "a doubly tagged task".to_string(),
        tags: vec![
          SerTag {
            id: id_tag2,
          },
          SerTag {
            id: id_tag1,
          },
        ],
      },
    ]);
    let task_state = SerTaskState {
      templates: templates,
      tasks: tasks,
    };
    let task_path = PathBuf::default();

    let state = State::with_serde(ui_state, ui_path, task_state, task_path).unwrap();
    let tasks = state.1.tasks.borrow();
    let mut it = tasks.iter();

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
