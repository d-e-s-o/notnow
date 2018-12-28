// state.rs

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
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub queries: Vec<Query>,
}
