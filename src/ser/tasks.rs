// tasks.rs

// *************************************************************************
// * Copyright (C) 2018 Daniel Mueller (deso@posteo.net)                   *
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
        id: TagId::new(2),
      },
      Tag {
        id: TagId::new(4),
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
            id: TagId::new(10000),
          },
          Tag {
            id: TagId::new(5),
          },
        ],
      },
      Task {
        tags: vec![
          Tag {
            id: TagId::new(5),
          },
          Tag {
            id: TagId::new(6),
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
