// Copyright (C) 2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::convert::TryFrom;
use std::str::FromStr as _;

use anyhow::Error;

use icalendar::Calendar;
use icalendar::Component as _;
use icalendar::Todo;

use uuid::uuid;

use crate::ser::tags::Template;
use crate::ser::tags::Templates;
use crate::ser::tasks::Id as TaskId;
use crate::ser::tasks::TasksMeta;

use super::util::emit_list;
use super::util::parse_list;
use super::util::try_from_calendar_with_single_todo;
use super::SerICal;


/// The name of the property used for storing templates.
const TEMPLATES_PROPERTY: &str = "TEMPLATES";
/// The name of the property used for storing the ordered list of task
/// IDs.
const IDS_PROPERTY: &str = "IDS";


impl From<&TasksMeta> for Todo {
  fn from(tasks_meta: &TasksMeta) -> Self {
    let mut todo = Todo::new();
    // TODO: We should share usage of `TASKS_META_ID` here, somehow,
    //       instead of hard coding the ID.
    todo.uid(
      &uuid!("00000000-0000-0000-0000-000000000000")
        .as_hyphenated()
        .to_string(),
    );

    if let Some(templates) = emit_list(&tasks_meta.templates.0) {
      todo.add_property(TEMPLATES_PROPERTY, &templates);
    }

    if let Some(ids) = emit_list(&tasks_meta.ids) {
      todo.add_property(IDS_PROPERTY, &ids);
    }

    todo
  }
}


impl From<&TasksMeta> for Calendar {
  fn from(tasks_meta: &TasksMeta) -> Self {
    let todo = Todo::from(tasks_meta);
    let calendar = Calendar::from([todo]);
    calendar
  }
}


impl TryFrom<&Todo> for TasksMeta {
  type Error = Error;

  fn try_from(todo: &Todo) -> Result<Self, Self::Error> {
    let templates = todo
      .property_value(TEMPLATES_PROPERTY)
      .map(|s| parse_list::<Template>(s).map(Templates))
      .unwrap_or_else(|| Ok(Templates::default()))?;
    let ids = todo
      .property_value(IDS_PROPERTY)
      .map(parse_list::<TaskId>)
      .unwrap_or_else(|| Ok(Vec::new()))?;

    let tasks_meta = TasksMeta { templates, ids };
    Ok(tasks_meta)
  }
}


impl TryFrom<&Calendar> for TasksMeta {
  type Error = Error;

  fn try_from(calendar: &Calendar) -> Result<Self, Self::Error> {
    try_from_calendar_with_single_todo::<Self>(calendar)
  }
}

impl SerICal for TasksMeta {
  #[inline]
  fn to_ical_string(&self) -> String {
    let calendar = Calendar::from(self);
    calendar.to_string()
  }

  #[inline]
  fn from_ical_string(data: &str) -> Result<Self, Error> {
    let calendar = Calendar::from_str(data).map_err(Error::msg)?;
    let task = TasksMeta::try_from(&calendar)?;
    Ok(task)
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  use crate::ser::tags::Id as TagId;
  use crate::ser::tasks::Id as TaskId;

  use super::super::iCal;
  use super::super::Backend;


  /// Make sure that we can serialize and deserialize a `TasksMeta`
  /// object.
  #[test]
  fn serialize_deserialize_tasks_meta() {
    let templates = Templates(vec![
      Template {
        id: TagId::try_from(1).unwrap(),
        name: "tag1".to_string(),
      },
      Template {
        id: TagId::try_from(2).unwrap(),
        name: "tag2".to_string(),
      },
    ]);

    let ids = vec![TaskId::new_v4(), TaskId::new_v4(), TaskId::new_v4()];

    let tasks_meta = TasksMeta { templates, ids };

    let serialized = iCal::serialize(&tasks_meta).unwrap();
    let deserialized = <iCal as Backend<TasksMeta>>::deserialize(&serialized).unwrap();

    assert_eq!(deserialized, tasks_meta);
  }
}
