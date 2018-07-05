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
use std::iter::FromIterator;
use std::path::Path;
use std::slice;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use serde_json::from_reader;
use serde_json::to_string_pretty as to_json;


#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct Id {
  id: usize,
}

fn unique_id() -> Id {
  static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

  Id {
    id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
  }
}


/// A struct representing a task item.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Task {
  // The task's ID never ends up being serialized or deserialized by
  // virtue of tagging it with "skip". We need `Id`s to be unique and
  // when persistence comes into play that is way harder to enforce.
  #[serde(default = "unique_id", skip)]
  pub id: Id,
  pub summary: String,
}

impl Task {
  /// Create a new task.
  pub fn new(summary: impl Into<String>) -> Self {
    Task {
      id: unique_id(),
      summary: summary.into(),
    }
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
      .position(|x| x.id == id)
      .map(|x| self.tasks.remove(x))
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

impl FromIterator<Task> for Tasks {
  /// Create a `Tasks` object from an iterator of `Task` objects.
  fn from_iter<I>(iter: I) -> Self
  where
    I: IntoIterator<Item = Task>,
  {
    Tasks {
      tasks: Vec::<Task>::from_iter(iter),
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
  use std::ops::Deref;
  use std::path::PathBuf;

  use serde_json::from_str as from_json;


  pub fn make_tasks(count: usize) -> Tasks {
    Tasks::from_iter((0..count).map(|i| Task::new(format!("{}", i + 1))))
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
  }

  impl Deref for NamedTempFile {
    type Target = PathBuf;

    fn deref(&self) -> &PathBuf {
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
  fn unique_id_increases() {
    let id1 = unique_id();
    let id2 = unique_id();

    assert!(id2.id > id1.id);
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
    let id = tasks.iter().nth(1).unwrap().id;
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

    tasks.save(&*file).unwrap();

    let new_tasks = Tasks::new(&*file).unwrap();
    assert_eq!(new_tasks, tasks);
  }

  #[test]
  fn load_tasks_file_not_found() {
    let path = {
      let file = NamedTempFile::new();
      let tasks = Tasks {
        tasks: vec![Task::new("make this not empty")],
      };

      tasks.save(&*file).unwrap();
      file.clone()
    };

    // The file is removed by now, so we can test that Tasks handles
    // such a missing file gracefully.
    let new_tasks = Tasks::new(&path).unwrap();
    assert_eq!(new_tasks, Default::default());
  }
}
