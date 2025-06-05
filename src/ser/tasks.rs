// Copyright (C) 2018-2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

//! A module providing serialization and deserialization support for
//! task objects.

use uuid::Uuid;

use crate::ser::tags::Tag;
use crate::ser::tags::Templates;


/// A serializable and deserializable task ID.
pub type Id = Uuid;


/// A task that we deserialize into and serialize from.
#[derive(Clone, Debug, PartialEq)]
pub struct Task {
  /// The task's ID.
  pub id: Id,
  /// The task's summary.
  pub summary: String,
  /// Arbitrary (and optional) details (such as notes) for the task.
  pub details: String,
  /// The task's list of currently set tags.
  pub tags: Vec<Tag>,
  /// The task's position.
  pub position: Option<f64>,
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
      details: Default::default(),
      tags: Default::default(),
      position: None,
    }
  }

  /// A convenience helper for setting the task's details.
  pub fn with_details<D>(mut self, details: D) -> Self
  where
    D: ToString,
  {
    self.details = details.to_string();
    self
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


/// Meta data for tasks that we deserialize into and serialize from.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct TasksMeta {
  /// The templates used by the corresponding tasks.
  pub templates: Templates,
}


/// A struct comprising a list of tasks.
#[derive(Debug, Default, PartialEq)]
pub struct Tasks(pub Vec<Task>);

#[cfg(test)]
impl Tasks {
  /// Convert this object into a vector of task objects.
  pub fn into_task_vec(mut self) -> Vec<Task> {
    // TODO: For the sake of testing we do not want to deal with
    //       positions, as they screw up any comparisons we want to
    //       perform. So just clear them out. This is certainly far from
    //       great, but no other way has been found.
    self.0.iter_mut().for_each(|task| {
      let _prev = task.position.take();
    });
    self.0
  }
}

#[cfg(any(test, feature = "test"))]
impl From<Vec<Task>> for Tasks {
  fn from(tasks: Vec<Task>) -> Self {
    Self(tasks)
  }
}
