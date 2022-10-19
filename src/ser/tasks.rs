// Copyright (C) 2018-2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use serde::Deserialize;
use serde::Serialize;

use crate::ser::tags::Tag;


/// A task that can be serialized and deserialized.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Task {
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
      summary: summary.into(),
      tags: Default::default(),
    }
  }
}


/// A struct comprising a list of tasks.
#[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Tasks(pub Vec<Task>);


#[cfg(test)]
mod tests {
  use super::*;

  use serde_json::from_str as from_json;
  use serde_json::to_string as to_json;

  use crate::ser::tags::Id as TagId;


  #[test]
  fn serialize_deserialize_task_without_tags() {
    let task = Task {
      summary: "task without tags".to_string(),
      tags: Vec::new(),
    };
    let serialized = to_json(&task).unwrap();
    let deserialized = from_json::<Task>(&serialized).unwrap();

    assert_eq!(deserialized, task);
  }

  #[test]
  fn serialize_deserialize_task() {
    let tags = vec![
      Tag {
        id: TagId::try_from(2).unwrap(),
      },
      Tag {
        id: TagId::try_from(4).unwrap(),
      },
    ];
    let task = Task {
      summary: "this is a task".to_string(),
      tags: tags,
    };
    let serialized = to_json(&task).unwrap();
    let deserialized = from_json::<Task>(&serialized).unwrap();

    assert_eq!(deserialized, task);
  }

  #[test]
  fn serialize_deserialize_tasks() {
    let task_vec = vec![
      Task {
        summary: "task 1".to_string(),
        tags: vec![
          Tag {
            id: TagId::try_from(10000).unwrap(),
          },
          Tag {
            id: TagId::try_from(5).unwrap(),
          },
        ],
      },
      Task {
        tags: vec![
          Tag {
            id: TagId::try_from(5).unwrap(),
          },
          Tag {
            id: TagId::try_from(6).unwrap(),
          },
        ],
        summary: "task 2".to_string(),
      },
    ];
    let tasks = Tasks(task_vec);
    let serialized = to_json(&tasks).unwrap();
    let deserialized = from_json::<Tasks>(&serialized).unwrap();

    assert_eq!(deserialized, tasks);
  }
}
