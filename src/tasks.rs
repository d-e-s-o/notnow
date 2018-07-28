// tasks.rs

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

use std::cmp::PartialEq;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::io::Result;
use std::io::Write;
use std::path::Path;
use std::slice;

use serde_json::from_reader;
use serde_json::to_string_pretty as to_json;

use id::Id as IdT;

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct T(());

pub type Id = IdT<T>;


/// An enumeration describing the states a `Task` can be in.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum State {
  NotStarted,
  Completed,
}

impl State {
  pub fn cycle(&mut self) -> Self {
    match self {
      State::NotStarted => State::Completed,
      State::Completed => State::NotStarted,
    }
  }
}

impl Default for State {
  fn default() -> Self {
    State::NotStarted
  }
}


/// A struct representing a task item.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Task {
  // The task's ID never ends up being serialized or deserialized by
  // virtue of tagging it with "skip". We need `Id`s to be unique and
  // when persistence comes into play that is way harder to enforce.
  #[serde(default = "Id::new", skip)]
  id: Id,
  pub summary: String,
  #[serde(default)]
  pub state: State,
}

impl Task {
  /// Create a new task.
  pub fn new(summary: impl Into<String>) -> Self {
    Task {
      id: Id::new(),
      summary: summary.into(),
      state: State::NotStarted,
    }
  }

  /// Retrieve this task's `Id`.
  pub fn id(&self) -> Id {
    self.id
  }
}

impl PartialEq for Task {
  fn eq(&self, other: &Task) -> bool {
    // For our intents and purposes two `Task` objects are equal as long
    // as all fields with exception of `id` (which we don't care about)
    // are equal.
    self.summary == other.summary
  }
}


pub type TaskIter<'a> = slice::Iter<'a, Task>;


/// A management struct for tasks and their associated data.
#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub struct Tasks {
  tasks: Vec<Task>,
}

impl Tasks {
  /// Create a new `Tasks` object by loading tasks from a file.
  pub fn new<P>(path: &P) -> Result<Self>
  where
    P: AsRef<Path>,
  {
    match File::open(path) {
      Ok(file) => Ok(from_reader(&file)?),
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

  /// Persist the tasks into a file.
  pub fn save<P>(&self, path: &P) -> Result<()>
  where
    P: AsRef<Path>,
  {
    let serialized = to_json(&self)?;
    OpenOptions::new()
      .create(true)
      .truncate(true)
      .write(true)
      .open(path)?
      .write_all(serialized.as_ref())?;
    Ok(())
  }

  /// Retrieve an iterator over the tasks.
  pub fn iter(&self) -> TaskIter {
    self.tasks.iter()
  }

  /// Add a new task.
  pub fn add(&mut self, task: Task) {
    self.tasks.push(task);
  }

  /// Remove a task.
  pub fn remove(&mut self, id: Id) {
    self
      .tasks
      .iter()
      .position(|x| x.id() == id)
      .map(|x| self.tasks.remove(x))
      .unwrap();
  }

  /// Update a task.
  pub fn update(&mut self, task: Task) {
    self
      .tasks
      .iter_mut()
      .position(|x| x.id() == task.id())
      .map(|x| self.tasks[x] = task)
      .unwrap();
  }
}

impl Default for Tasks {
  fn default() -> Self {
    Tasks {
      tasks: Vec::new(),
    }
  }
}

impl From<Vec<Task>> for Tasks {
  /// Create a 'Tasks' object from a vector of tasks.
  fn from(tasks: Vec<Task>) -> Self {
    Tasks {
      tasks: tasks,
    }
  }
}


// Our tests can live with having unsafe code in them.
#[allow(unsafe_code)]
#[cfg(test)]
pub mod tests {
  extern crate libc;

  use super::*;
  use std::ffi::CString;
  use std::fs::remove_file;
  use std::path::PathBuf;

  use serde_json::from_str as from_json;


  pub fn make_tasks_vec(count: usize) -> Vec<Task> {
    (0..count).map(|i| Task::new(format!("{}", i + 1))).collect()
  }

  pub fn make_tasks(count: usize) -> Tasks {
    Tasks::from(make_tasks_vec(count))
  }

  #[link(name = "c")]
  extern "C" {
    fn mkstemp(template: *mut libc::c_char) -> libc::c_int;
    fn close(file: libc::c_int) -> libc::c_int;
  }

  /// A temporary file with a visible file system path.
  ///
  /// This class is only meant for our internal testing!
  pub struct NamedTempFile {
    file: u64,
    path: PathBuf,
  }

  impl NamedTempFile {
    pub fn new() -> Self {
      let template = CString::new("/tmp/tempXXXXXX").unwrap();
      let raw = template.into_raw();
      let result = unsafe { mkstemp(raw) };
      assert!(result > 0);

      NamedTempFile {
        file: result as u64,
        path: unsafe { PathBuf::from(CString::from_raw(raw).into_string().unwrap()) },
      }
    }

    pub fn path(&self) -> &PathBuf {
      &self.path
    }
  }

  impl Drop for NamedTempFile {
    fn drop(&mut self) {
      remove_file(&self.path).unwrap();

      let result = unsafe { close(self.file as libc::c_int) };
      assert!(result == 0)
    }
  }


  #[test]
  fn add_task() {
    let mut tasks = make_tasks(3);
    tasks.add(Task::new("4"));

    assert_eq!(tasks, make_tasks(4));
  }

  #[test]
  fn remove_task() {
    let mut tasks = make_tasks(3);
    let id = tasks.iter().nth(1).unwrap().id();
    tasks.remove(id);

    let expected = Tasks {
      tasks: vec![
        Task::new("1".to_string()),
        Task::new("3".to_string()),
      ]
    };

    assert_eq!(tasks, expected);
  }

  #[test]
  fn update_task() {
    let mut tasks = make_tasks(3);
    let mut task = tasks.iter().nth(1).unwrap().clone();
    task.summary = "amended".to_string();
    tasks.update(task);

    let expected = Tasks {
      tasks: vec![
        Task::new("1".to_string()),
        Task::new("amended".to_string()),
        Task::new("3".to_string()),
      ]
    };

    assert_eq!(tasks, expected);
  }

  #[test]
  fn serialize_deserialize_task() {
    let task = Task::new("this is a TODO");
    let serialized = to_json(&task).unwrap();
    let deserialized = from_json::<Task>(&serialized).unwrap();

    assert_eq!(deserialized, task);
  }

  #[test]
  fn serialize_deserialize_tasks() {
    let tasks = Tasks {
      tasks: vec![
        Task::new("this is the first TODO"),
        Task::new("here goes the second one"),
        Task::new("and now for the final task"),
      ],
    };
    let serialized = to_json(&tasks).unwrap();
    let deserialized = from_json::<Tasks>(&serialized).unwrap();

    assert_eq!(deserialized, tasks);
  }

  #[test]
  fn save_and_load_tasks() {
    let file = NamedTempFile::new();
    let tasks = Tasks {
      tasks: vec![
        Task::new("this is the first TODO"),
        Task::new("here goes the second one"),
        Task::new("and now for the final task"),
      ],
    };

    tasks.save(file.path()).unwrap();

    let new_tasks = Tasks::new(file.path()).unwrap();
    assert_eq!(new_tasks, tasks);
  }

  #[test]
  fn load_tasks_file_not_found() {
    let path = {
      let file = NamedTempFile::new();
      let tasks = Tasks {
        tasks: vec![Task::new("make this not empty")],
      };

      tasks.save(file.path()).unwrap();
      file.path().clone()
    };

    // The file is removed by now, so we can test that Tasks handles
    // such a missing file gracefully.
    let new_tasks = Tasks::new(&path).unwrap();
    assert_eq!(new_tasks, Default::default());
  }
}
