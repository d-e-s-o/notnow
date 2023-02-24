// Copyright (C) 2018-2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

//! A module providing serialization and deserialization support for
//! task objects.

use serde::Deserialize;
use serde::Serialize;

use uuid::Uuid;

use crate::ser::tags::Tag;
use crate::ser::tags::Templates;


/// A serializable and deserializable task ID.
pub type Id = Uuid;


/// A task that can be serialized and deserialized.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Task {
  #[serde(skip)]
  pub id: Id,
  pub summary: String,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub tags: Vec<Tag>,
}

#[cfg(any(test, feature = "test"))]
impl Task {
  /// Create a new task with the given summary and no tags.
  pub fn new<S>(summary: S) -> Self
  where
    S: Into<String>,
  {
    Self {
      id: Id::new_v4(),
      summary: summary.into(),
      tags: Default::default(),
    }
  }

  /// A convenience helper for setting the task's tags.
  pub fn with_tags<I>(mut self, tags: I) -> Self
  where
    I: IntoIterator<Item = Tag>,
  {
    self.tags = tags.into_iter().collect();
    self
  }
}

// TODO: We currently exclude the ID from any equality checking.
//       Effectively, a `Task` object has no identity, only state.
//       Long term we may want to adjust tests to not assume such
//       behavior, because this behavior can be problematic. For
//       example, we use this equality check not only in tests but also
//       in the core serialization logic of the program to determine
//       whether the in-program state has changed from the persisted
//       one and decide whether to save state in the first place.
//       Because we exclude the ID from equality checks, a change of it
//       would not trigger a save. That's okay, because we don't change
//       the ID, but it's unclean nevertheless.
impl PartialEq for Task {
  fn eq(&self, other: &Self) -> bool {
    self.summary == other.summary && self.tags == other.tags
  }
}


/// Meta data for tasks.
#[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TasksMeta {
  #[serde(default)]
  pub templates: Templates,
  /// IDs of tasks in the intended order.
  pub ids: Vec<Id>,
}


/// A struct comprising a list of tasks.
#[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Tasks(pub Vec<Task>);

#[cfg(test)]
impl Tasks {
  /// Convert this object into a vector of task objects.
  pub fn into_task_vec(self) -> Vec<Task> {
    self.0
  }
}

#[cfg(any(test, feature = "test"))]
impl From<Vec<Task>> for Tasks {
  fn from(tasks: Vec<Task>) -> Self {
    Self(tasks)
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  use serde_json::from_str as from_json;
  use serde_json::to_string as to_json;

  use crate::ser::tags::Id as TagId;


  #[test]
  fn serialize_deserialize_task_without_tags() {
    let task = Task::new("task without tags");
    let serialized = to_json(&task).unwrap();
    let deserialized = from_json::<Task>(&serialized).unwrap();

    assert_eq!(deserialized, task);
  }

  #[test]
  fn serialize_deserialize_task() {
    let tags = [
      Tag {
        id: TagId::try_from(2).unwrap(),
      },
      Tag {
        id: TagId::try_from(4).unwrap(),
      },
    ];
    let task = Task::new("this is a task").with_tags(tags);
    let serialized = to_json(&task).unwrap();
    let deserialized = from_json::<Task>(&serialized).unwrap();

    assert_eq!(deserialized, task);
  }

  #[test]
  fn serialize_deserialize_tasks() {
    let task_vec = vec![
      Task::new("task 1").with_tags([
        Tag {
          id: TagId::try_from(10000).unwrap(),
        },
        Tag {
          id: TagId::try_from(5).unwrap(),
        },
      ]),
      Task::new("task 2").with_tags([
        Tag {
          id: TagId::try_from(5).unwrap(),
        },
        Tag {
          id: TagId::try_from(6).unwrap(),
        },
      ]),
    ];
    let tasks = Tasks::from(task_vec);
    let serialized = to_json(&tasks).unwrap();
    let deserialized = from_json::<Tasks>(&serialized).unwrap();

    assert_eq!(deserialized, tasks);
  }
}
