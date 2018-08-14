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
use std::collections::BTreeMap;
use std::io::Error;
use std::io::ErrorKind;
use std::io::Result;
use std::rc::Rc;
use std::slice;

use id::Id as IdT;
use ser::tasks::Task as SerTask;
use ser::tasks::Tasks as SerTasks;
use tags::Id as TagId;
use tags::Tag;
use tags::TagMap;
use tags::Templates;

#[derive(Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct T(());

pub type Id = IdT<T>;


/// A struct representing a task item.
#[derive(Clone, Debug)]
pub struct Task {
  id: Id,
  pub summary: String,
  tags: BTreeMap<TagId, Tag>,
  templates: Rc<Templates>,
}

impl Task {
  /// Create a new task.
  #[cfg(test)]
  pub fn new(summary: impl Into<String>) -> Self {
    Task {
      id: Id::new(),
      summary: summary.into(),
      tags: Default::default(),
      templates: Rc::new(Templates::new()),
    }
  }

  /// Create a task using the given summary.
  fn with_summary(summary: impl Into<String>, templates: Rc<Templates>) -> Self {
    Task {
      id: Id::new(),
      summary: summary.into(),
      tags: Default::default(),
      templates: templates,
    }
  }

  /// Create a new task from a serializable one.
  fn with_serde(mut task: SerTask, templates: Rc<Templates>, map: &TagMap) -> Result<Task> {
    let mut tags = BTreeMap::new();
    for tag in task.tags.drain(..) {
      let id = map.get(&tag.id).ok_or_else(|| {
        let error = format!("Encountered invalid tag Id {}", tag.id);
        Error::new(ErrorKind::InvalidInput, error)
      })?;
      tags.insert(*id, templates.instantiate(*id));
    }

    Ok(Task {
      id: Id::new(),
      summary: task.summary,
      tags: tags,
      templates: templates,
    })
  }

  /// Convert this task into a serializable one.
  pub fn to_serde(&self) -> SerTask {
    SerTask {
      summary: self.summary.clone(),
      tags: self.tags.iter().map(|(_, x)| x.to_serde()).collect(),
    }
  }

  /// Retrieve this task's `Id`.
  pub fn id(&self) -> Id {
    self.id
  }

  /// Retrieve an iterator over this task's tags.
  pub fn tags(&self) -> impl Iterator<Item=&Tag> + Clone {
    self.tags.values()
  }

  /// Check whether the task is tagged as complete or not.
  pub fn is_complete(&self) -> bool {
    let id = self.templates.complete_tag().id();
    self.tags.contains_key(&id)
  }

  /// Toggle the completion state of the task.
  pub fn toggle_complete(&mut self) {
    let id = self.templates.complete_tag().id();

    // Try removing the complete tag, if that succeeds we are done (as
    // the tag was present and got removed), otherwise insert it (as it
    // was not present).
    if self.tags.remove(&id).is_none() {
      let tag = self.templates.instantiate(id);
      self.tags.insert(id, tag);
    }
  }
}

impl PartialEq for Task {
  fn eq(&self, other: &Task) -> bool {
    let result = self.id == other.id;
    assert!(!result || self.summary == other.summary);
    assert!(!result || self.tags == other.tags);
    result
  }
}


pub type TaskIter<'a> = slice::Iter<'a, Task>;


/// A management struct for tasks and their associated data.
#[derive(Debug, PartialEq)]
pub struct Tasks {
  templates: Rc<Templates>,
  tasks: Vec<Task>,
}

impl Tasks {
  /// Create a new `Tasks` object from a serializable one.
  pub fn with_serde(mut tasks: SerTasks, templates: Rc<Templates>, map: &TagMap) -> Result<Self> {
    let mut new_tasks = Vec::with_capacity(tasks.0.len());
    for task in tasks.0.drain(..) {
      let task = Task::with_serde(task, templates.clone(), &map)?;
      new_tasks.push(task);
    }

    Ok(Tasks {
      templates: templates,
      tasks: new_tasks,
    })
  }

  /// Convert this object into a serializable one.
  pub fn to_serde(&self) -> SerTasks {
    SerTasks(self.tasks.iter().map(|x| x.to_serde()).collect())
  }

  /// Retrieve an iterator over the tasks.
  pub fn iter(&self) -> TaskIter {
    self.tasks.iter()
  }

  /// Add a new task.
  pub fn add(&mut self, summary: impl Into<String>) -> Id {
    let task = Task::with_summary(summary, self.templates.clone());
    let id = task.id;
    self.tasks.push(task);
    id
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

#[cfg(test)]
impl From<Vec<Task>> for Tasks {
  /// Create a 'Tasks' object from a vector of tasks.
  fn from(tasks: Vec<Task>) -> Self {
    // Test code using this constructor is assumed to only have tasks
    // that have no tags.
    tasks.iter().for_each(|x| assert!(x.tags.is_empty()));

    Tasks {
      templates: Rc::new(Templates::new()),
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
  use std::iter::FromIterator;
  use std::ops::Deref;
  use std::ops::DerefMut;
  use std::path::PathBuf;
  use std::rc::Rc;

  use serde_json::from_str as from_json;
  use serde_json::to_string_pretty as to_json;

  use ser::tags::Id as SerId;
  use ser::tags::Tag as SerTag;
  use ser::tags::Template as SerTemplate;
  use ser::tags::Templates as SerTemplates;
  use tags::COMPLETE_TAG;


  #[derive(Debug)]
  pub struct TaskVec(pub Vec<Task>);

  impl Deref for TaskVec {
    type Target = Vec<Task>;

    fn deref(&self) -> &Self::Target {
      &self.0
    }
  }

  impl DerefMut for TaskVec {
    fn deref_mut(&mut self) -> &mut Self::Target {
      &mut self.0
    }
  }

  impl From<Tasks> for TaskVec {
    fn from(tasks: Tasks) -> Self {
      TaskVec::from_iter(tasks.iter().cloned())
    }
  }

  impl FromIterator<Task> for TaskVec {
    fn from_iter<I>(iter: I) -> Self
    where
      I: IntoIterator<Item = Task>,
    {
      TaskVec(Vec::<Task>::from_iter(iter))
    }
  }

  impl PartialEq for TaskVec {
    fn eq(&self, other: &TaskVec) -> bool {
      if self.len() != other.len() {
        false
      } else {
        for (x, y) in self.iter().zip(other.iter()) {
          if x.summary != y.summary {
            return false
          }
        }
        true
      }
    }
  }

  impl From<TaskVec> for Tasks {
    fn from(tasks: TaskVec) -> Self {
      Self::from(tasks.0)
    }
  }


  pub fn make_tasks_vec(count: usize) -> TaskVec {
    TaskVec(
      (0..count)
        .map(|i| Task::new(format!("{}", i + 1)))
        .collect(),
    )
  }

  /// Create a set of tasks that have associated tags.
  ///
  /// Tags are assigned in the following fashion:
  /// task1  -> []
  /// task2  -> [complete]
  /// task3  -> []
  /// task4  -> [complete]
  ///
  /// task5  -> [tag1]
  /// task6  -> [tag1 + complete]
  /// task7  -> [tag1]
  /// task8  -> [tag1 + complete]
  ///
  /// task9  -> [tag2]
  /// task10 -> [tag2 + complete]
  /// task11 -> [tag2 + tag1]
  /// task12 -> [tag2 + tag1 + complete]
  ///
  /// task13 -> [tag3]
  /// task14 -> [tag3 + complete]
  /// task15 -> [tag3 + tag2 + tag1]
  /// task16 -> [tag3 + tag2 + tag1 + complete]
  ///
  /// task17 -> [tag4]
  /// task18 -> [tag4 + complete]
  /// task19 -> [tag4 + tag3 + tag2 + tag1]
  /// task20 -> [tag4 + tag3 + tag2 + tag1 + complete]
  ///
  /// ...
  pub fn make_tasks_with_tags(count: usize) -> (Vec<SerId>, Vec<SerTemplate>, Vec<SerTask>) {
    let tag_ids = (0..count / 4 + 1)
      .map(|x| SerId::new(x))
      .collect::<Vec<_>>();
    let templates = (0..count / 4 + 1)
      .map(|x| if x == 0 {
        SerTemplate {
          id: tag_ids[x],
          name: COMPLETE_TAG.to_string(),
        }
      } else {
        SerTemplate {
          id: tag_ids[x],
          name: format!("tag{}", x),
        }
      })
      .collect::<Vec<_>>();
    let tasks = (0..count)
      .map(|x| {
        let mut tags = Vec::new();
        // Add 'complete' tag for uneven tasks.
        if x % 2 == 1 {
          tags.push(tag_ids[0])
        }
        // Add the "newest" tag.
        if x >= 4 {
          tags.push(tag_ids[x / 4])
        }
        // Add all previous tags.
        if x >= 8 && x % 4 >= 2 {
          tags.extend_from_slice(&tag_ids[1..x / 4])
        }
        let tags = tags
          .drain(..)
          .map(|x| {
            SerTag {
              id: x,
            }
          })
          .collect();
        SerTask {
          summary: format!("{}", x + 1),
          tags: tags,
        }
      })
      .collect();

    (tag_ids, templates, tasks)
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
    let mut tasks = Tasks::from(make_tasks_vec(3));
    tasks.add("4");

    assert_eq!(TaskVec::from(tasks), make_tasks_vec(4));
  }

  #[test]
  fn remove_task() {
    let mut tasks = Tasks::from(make_tasks_vec(3));
    let id = tasks.iter().nth(1).unwrap().id();
    tasks.remove(id);

    let mut expected = make_tasks_vec(3);
    expected.remove(1);

    assert_eq!(TaskVec::from(tasks), expected);
  }

  #[test]
  fn update_task() {
    let mut tasks = Tasks::from(make_tasks_vec(3));
    let mut task = tasks.iter().nth(1).unwrap().clone();
    task.summary = "amended".to_string();
    tasks.update(task);

    let expected = TaskVec(vec![
      Task::new("1".to_string()),
      Task::new("amended".to_string()),
      Task::new("3".to_string()),
    ]);

    assert_eq!(TaskVec::from(tasks), expected);
  }

  #[test]
  fn task_completion() {
    let mut task = Task::new("test task");
    assert!(!task.is_complete());
    task.toggle_complete();
    assert!(task.is_complete());
  }

  #[test]
  fn serialize_deserialize_task() {
    let task = Task::new("this is a TODO");
    let serialized = to_json(&task.to_serde()).unwrap();
    let deserialized = from_json::<SerTask>(&serialized).unwrap();

    assert_eq!(deserialized.summary, task.summary);
  }

  #[test]
  fn serialize_deserialize_tasks() {
    let task_vec = TaskVec(vec![
      Task::new("this is the first TODO"),
      Task::new("here goes the second one"),
      Task::new("and now for the final task"),
    ]);
    let (templates, map) = Templates::with_serde(SerTemplates(Default::default()));
    let templates = Rc::new(templates);
    let tasks = Tasks::from(task_vec.clone());
    let serialized = to_json(&tasks.to_serde()).unwrap();
    let deserialized = from_json::<SerTasks>(&serialized).unwrap();
    let tasks = Tasks::with_serde(deserialized, templates, &map).unwrap();

    assert_eq!(TaskVec::from(tasks), task_vec);
  }
}
