// Copyright (C) 2018-2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

//! A module providing serialization and deserialization support for the
//! program's state objects.

use serde::Deserialize;
use serde::Serialize;

use crate::colors::Colors;
use crate::ser::tags::Tag;
use crate::ser::tasks::Tasks;
use crate::ser::tasks::TasksMeta;
use crate::ser::view::View;


/// A struct comprising the program state itself.
#[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct UiState {
  // We keep the colors at the start of the struct because that means
  // they will be at the start of the file and they are the most likely
  // to be modified by a user.
  #[serde(default)]
  pub colors: Colors,
  /// The tag to toggle on user initiated action.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub toggle_tag: Option<Tag>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub views: Vec<(View, Option<usize>)>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub selected: Option<usize>,
}


/// A struct comprising the task state of the program.
///
/// Note that this type is not actually serialized or deserialized in
/// this form directly. It merely acts as a way of grouping
/// functionality that is frequently used alongside each other.
///
#[derive(Debug, Default, PartialEq)]
pub struct TaskState {
  /// Meta data about tasks.
  pub tasks_meta: TasksMeta,
  /// A list of tasks.
  pub tasks: Tasks,
}
