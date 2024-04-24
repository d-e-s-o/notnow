// Copyright (C) 2022-2024 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::convert::TryFrom;
use std::str::FromStr as _;

use anyhow::Error;
use anyhow::Result;

use icalendar::Calendar;
use icalendar::Component as _;
use icalendar::Todo;

use crate::ser::tags::Tag;
use crate::ser::tasks::Id as TaskId;
use crate::ser::tasks::Task;
use crate::LINE_END;
use crate::LINE_END_STR;

use super::util::emit_list;
use super::util::parse_list;
use super::util::try_from_calendar_with_single_todo;
use super::SerICal;


/// The name of the property used for storing a task's tags.
const TAGS_PROPERTY: &str = "TAGS";
/// The name of the property used for storing a task's "position".
const POSITION_PROPERTY: &str = "POSITION";


impl From<&Task> for Todo {
  fn from(task: &Task) -> Self {
    let mut todo = Todo::new();
    todo.uid(&task.id.as_hyphenated().to_string());
    todo.summary(&task.summary);

    if !task.details.is_empty() {
      todo.description(&task.details.replace(LINE_END, "\n"));
    }

    if let Some(tags) = emit_list(&task.tags) {
      todo.add_property(TAGS_PROPERTY, &tags);
    }
    if let Some(position) = &task.position {
      todo.add_property(POSITION_PROPERTY, &position.to_string());
    }

    todo
  }
}


impl From<&Task> for Calendar {
  fn from(task: &Task) -> Self {
    let todo = Todo::from(task);
    let calendar = Calendar::from([todo]);
    calendar
  }
}


impl TryFrom<&Todo> for Task {
  type Error = Error;

  fn try_from(todo: &Todo) -> Result<Self, Self::Error> {
    // TODO: Ideally we would hook up the TODO's completion state. The
    //       problem is that currently doing so would require a lot of
    //       outside knowledge as to which tag actually maps to that.

    let id = todo
      .get_uid()
      .map(TaskId::from_str)
      .transpose()?
      .unwrap_or_else(TaskId::new_v4);
    let summary = todo.get_summary().unwrap_or("").to_string();
    let details = todo
      .get_description()
      .unwrap_or("")
      .to_string()
      // TODO: The first `replace` should not be necessary. It is needed right
      //       now due to https://github.com/hoodie/icalendar-rs/issues/87,
      //       but this should be fixed upstream.
      .replace("\\n", "\n")
      .replace('\n', LINE_END_STR);
    let tags = todo
      .property_value(TAGS_PROPERTY)
      .map(parse_list::<Tag>)
      .unwrap_or_else(|| Ok(Vec::new()))?;
    let position = todo
      .property_value(POSITION_PROPERTY)
      .map(f64::from_str)
      .transpose()?;

    Ok(Task {
      id,
      summary,
      details,
      tags,
      position,
    })
  }
}


impl TryFrom<&Calendar> for Task {
  type Error = Error;

  fn try_from(calendar: &Calendar) -> Result<Self, Self::Error> {
    try_from_calendar_with_single_todo::<Self>(calendar)
  }
}


impl SerICal for Task {
  #[inline]
  fn to_ical_string(&self) -> String {
    let calendar = Calendar::from(self);
    calendar.to_string()
  }

  #[inline]
  fn from_ical_string(data: &str) -> Result<Self, Error> {
    let calendar = Calendar::from_str(data).map_err(Error::msg)?;
    let task = Task::try_from(&calendar)?;
    Ok(task)
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  use crate::ser::tags::Id as TagId;
  use crate::LINE_END;

  use super::super::iCal;
  use super::super::Backend;


  /// Make sure that we can serialize and deserialize a `Task` that
  /// contains no tags.
  #[test]
  fn serialize_deserialize_task() {
    let task = Task::new("test task");
    let data = iCal::serialize(&task).unwrap();
    let new_task = <iCal as Backend<Task>>::deserialize(&data).unwrap();

    assert_eq!(new_task, task);
  }

  /// Make sure that we can serialize and deserialize a `Task` that
  /// contains a tag.
  #[test]
  fn serialize_deserialize_task_with_tag() {
    let tags = [Tag::from(TagId::try_from(1337).unwrap())];
    let task = Task::new("test task").with_tags(tags);

    let data = iCal::serialize(&task).unwrap();
    let new_task = <iCal as Backend<Task>>::deserialize(&data).unwrap();

    assert_eq!(new_task, task);
  }

  /// Make sure that we can serialize and deserialize a `Task` with
  /// details spanning multiple lines.
  #[test]
  fn serialize_deserialize_task_with_multiline_details() {
    let details = format!("multi-{LINE_END}line{LINE_END}string");
    let task = Task::new("test task").with_details(details);

    let data = iCal::serialize(&task).unwrap();
    let new_task = <iCal as Backend<Task>>::deserialize(&data).unwrap();

    assert_eq!(new_task, task);
  }
}
