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

use std::cell::RefCell;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::io::Result;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;

use serde_json::from_reader;
use serde_json::to_string_pretty as to_json;

use query::Query;
use query::QueryBuilder;
use ser::state::State as SerState;
use tags::Templates;
use tasks::Id as TaskId;
use tasks::Task;
use tasks::Tasks;


/// An object encapsulating the program's relevant state.
#[derive(Debug)]
pub struct State {
  path: PathBuf,
  templates: Rc<Templates>,
  tasks: Rc<RefCell<Tasks>>,
}

impl State {
  /// Create a new `State` object, loaded from a file.
  pub fn new<P>(path: P) -> Result<Self>
  where
    P: Into<PathBuf> + AsRef<Path>,
  {
    match File::open(&path) {
      Ok(file) => {
        let state = from_reader::<File, SerState>(file)?;
        Self::with_serde(state, path.into())
      },
      Err(e) => {
        // If the file does not exist we create an empty object and work
        // with that.
        if e.kind() == ErrorKind::NotFound {
          let templates = Rc::new(Templates::new());
          let tasks = Rc::new(RefCell::new(Tasks::new(templates.clone())));

          Ok(State {
            path: path.into(),
            templates: templates,
            tasks: tasks,
          })
        } else {
          Err(e)
        }
      },
    }
  }

  /// Create a new `State` object from a serializable one.
  fn with_serde(state: SerState, path: PathBuf) -> Result<Self> {
    let (templates, map) = Templates::with_serde(state.templates);
    let templates = Rc::new(templates);
    let tasks = Tasks::with_serde(state.tasks, templates.clone(), &map)?;

    Ok(State {
      path: path,
      templates: templates,
      tasks: Rc::new(RefCell::new(tasks)),
    })
  }

  /// Create a new `State` object with the given `Tasks` object, with
  /// all future `save` operations happening into the provided path.
  #[cfg(test)]
  pub fn with_tasks_and_path<P>(tasks: Tasks, path: P) -> Self
  where
    P: Into<PathBuf> + AsRef<Path>,
  {
    // Test code using this constructor is assumed to only have tasks
    // that have no tags.
    tasks.iter().for_each(|x| assert!(x.tags().next().is_none()));

    State {
      path: path.into(),
      templates: Rc::new(Templates::new()),
      tasks: Rc::new(RefCell::new(tasks)),
    }
  }

  /// Convert this object into a serializable one.
  fn to_serde(&self) -> SerState {
    SerState {
      templates: self.templates.to_serde(),
      tasks: self.tasks.borrow().to_serde(),
    }
  }

  /// Persist the state into a file.
  pub fn save(&self) -> Result<()> {
    let tasks = self.to_serde();
    let serialized = to_json(&tasks)?;
    OpenOptions::new()
      .create(true)
      .truncate(true)
      .write(true)
      .open(&self.path)?
      .write_all(serialized.as_ref())?;
    Ok(())
  }

  /// Retrieve the tasks associated with this `State` object.
  pub fn tasks(&self) -> Query {
    QueryBuilder::new(self.tasks.clone()).build("all")
  }

  /// Add a new task to the list of tasks.
  pub fn add_task(&self, summary: impl Into<String>) -> TaskId {
    self.tasks.borrow_mut().add(summary)
  }

  /// Remove the task with the given `TaskId`.
  pub fn remove_task(&self, id: TaskId) {
    self.tasks.borrow_mut().remove(id)
  }

  /// Update a task.
  pub fn update_task(&self, task: Task) {
    self.tasks.borrow_mut().update(task)
  }
}


#[cfg(test)]
pub mod tests {
  use super::*;

  use ser::tags::Id as SerId;
  use ser::tags::Tag as SerTag;
  use ser::tags::Template as SerTemplate;
  use ser::tags::Templates as SerTemplates;
  use ser::tasks::Task as SerTask;
  use ser::tasks::Tasks as SerTasks;
  use tasks::tests::NamedTempFile;
  use tasks::tests::TaskVec;


  #[test]
  fn save_and_load_state() {
    let file = NamedTempFile::new();
    let task_vec = TaskVec(vec![
      Task::new("this is the first TODO"),
      Task::new("here goes the second one"),
      Task::new("and now for the final task"),
    ]);
    let tasks = Tasks::from(task_vec.clone());
    let state = State::with_tasks_and_path(tasks, file.path());
    state.save().unwrap();

    let new_state = State::new(file.path()).unwrap();
    let new_task_vec = new_state.tasks.borrow().iter().cloned().collect::<Vec<_>>();
    assert_eq!(TaskVec(new_task_vec), task_vec);
  }

  #[test]
  fn load_state_file_not_found() {
    let path = {
      let file = NamedTempFile::new();
      let tasks = vec![Task::new("make this not empty")];
      let state = State::with_tasks_and_path(Tasks::from(tasks), file.path());

      state.save().unwrap();
      file.path().clone()
    };

    // The file is removed by now, so we can test that `State` handles
    // such a missing file gracefully.
    let new_state = State::new(path).unwrap();
    let new_task_vec = new_state.tasks.borrow().iter().cloned().collect::<Vec<_>>();
    assert_eq!(TaskVec(new_task_vec), TaskVec(Vec::new()));
  }

  #[test]
  fn load_state_with_invalid_tag() {
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
    let state = SerState {
      templates: templates,
      tasks: tasks,
    };
    let path = Default::default();

    let err = State::with_serde(state, path).unwrap_err();
    assert_eq!(err.to_string(), "Encountered invalid tag Id 42")
  }

  #[test]
  fn load_state() {
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
    let state = SerState {
      templates: templates,
      tasks: tasks,
    };

    let state = State::with_serde(state, Default::default()).unwrap();
    let tasks = state.tasks.borrow();
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
