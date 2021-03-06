// Copyright (C) 2018-2019,2021 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use serde::Deserialize;
use serde::Serialize;

use crate::colors::Colors;
use crate::ser::query::Query;
use crate::ser::tags::Templates;
use crate::ser::tasks::Tasks;


/// A struct comprising the task state of the program.
#[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TaskState {
  #[serde(default)]
  pub templates: Templates,
  pub tasks: Tasks,
}


/// A struct comprising the program state itself.
#[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct UiState {
  // We keep the colors at the start of the struct because that means
  // they will be at the start of the file and they are the most likely
  // to be modified by a user.
  #[serde(default)]
  pub colors: Colors,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub queries: Vec<(Query, Option<usize>)>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub selected: Option<usize>,
}
