// Copyright (C) 2022 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt::Display;
use std::str::FromStr;

use anyhow::bail;
use anyhow::Context as _;
use anyhow::Error;
use anyhow::Result;

use icalendar::Calendar;
use icalendar::CalendarComponent;
use icalendar::Component as _;
use icalendar::Todo;


/// The separator to use between list items.
const LIST_ITEM_SEPARATOR: char = '|';


/// Parse a list of items that can be built from strings.
pub(super) fn parse_list<T>(string: &str) -> Result<Vec<T>>
where
  T: FromStr,
  Error: From<<T as FromStr>::Err>,
{
  // We don't currently support parsing an empty string, but it should
  // really be represented as `Option<&str>` anyway.
  debug_assert!(!string.is_empty());

  let items = string
    .split(LIST_ITEM_SEPARATOR)
    .map(|s| {
      T::from_str(s)
        .map_err(Error::from)
        .with_context(|| format!("failed to parse object from string '{s}'"))
    })
    .collect::<Result<_>>()?;
  Ok(items)
}


/// Emit a list of items as a string.
///
/// # Notes
/// This function does no escaping on the individual item's contents and
/// there is risk of a clash when parsing again if they contain
/// `LIST_ITEM_SEPARATOR` characters.
pub(super) fn emit_list<I, T>(iter: I) -> Option<String>
where
  I: IntoIterator<Item = T>,
  T: Display,
{
  // We hand roll a "join" here because the std one requires a `Vec` to
  // be allocated first and it does not spit out an `Option` directly,
  // as we want it.

  let mut iter = iter.into_iter();
  iter.next().map(|first| {
    iter.fold(first.to_string(), |list, item| {
      format!("{list}{LIST_ITEM_SEPARATOR}{item}")
    })
  })
}


/// Attempt to extract a custom object from a [`Calendar`] with a single
/// [`Todo`] component.
pub(super) fn try_from_calendar_with_single_todo<T>(calendar: &Calendar) -> Result<T>
where
  T: for<'todo> TryFrom<&'todo Todo, Error = Error>,
{
  fn validate_and_convert<T>(calendar: &Calendar) -> Result<T>
  where
    T: for<'todo> TryFrom<&'todo Todo, Error = Error>,
  {
    match calendar.components.as_slice() {
      [component] => {
        if let CalendarComponent::Todo(todo) = component {
          T::try_from(todo).with_context(|| {
            format!(
              "failed to convert TODO {} into object",
              todo.get_uid().unwrap_or("<undefined>")
            )
          })
        } else {
          bail!("calendar contains unsupported component type")
        }
      },
      [] => bail!("calendar contains no components"),
      [..] => bail!("calendar contains multiple components"),
    }
  }

  let name = calendar.get_name().unwrap_or("<unnamed>").to_string();
  validate_and_convert(calendar)
    .with_context(|| format!("failed to convert iCal calendar {name} to object"))
}


#[cfg(test)]
mod tests {
  use super::*;

  use crate::ser::tags::Id as TagId;
  use crate::ser::tags::Tag;
  use crate::ser::tasks::Task;


  /// Check that we can correctly emit and parse a list of `Tag`
  /// objects.
  #[test]
  fn emit_parse_tag_list() {
    assert_eq!(emit_list::<_, Tag>([]), None);

    let tags = [Tag::from(TagId::try_from(1).unwrap())];
    assert_eq!(parse_list::<Tag>(&emit_list(tags).unwrap()).unwrap(), tags);

    let tags = [
      Tag::from(TagId::try_from(42).unwrap()),
      Tag::from(TagId::try_from(37).unwrap()),
    ];
    assert_eq!(parse_list::<Tag>(&emit_list(tags).unwrap()).unwrap(), tags);
  }

  /// Check that we fail conversion from a `Calendar` object if it does
  /// not meet certain requirements.
  #[test]
  fn try_from_calendar_failure() {
    let calendar = Calendar::new();
    let err = try_from_calendar_with_single_todo::<Task>(&calendar).unwrap_err();

    assert_eq!(
      err.root_cause().to_string(),
      "calendar contains no components"
    );

    let todo1 = Todo::new();
    let todo2 = Todo::new();
    let calendar = Calendar::from([todo1, todo2]);

    let err = try_from_calendar_with_single_todo::<Task>(&calendar).unwrap_err();

    assert_eq!(
      err.root_cause().to_string(),
      "calendar contains multiple components"
    );
  }
}
