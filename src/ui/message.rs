// message.rs

// *************************************************************************
// * Copyright (C) 2020 Daniel Mueller (deso@posteo.net)                   *
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

use gui::Id;

use crate::tasks::Id as TaskId;
#[cfg(all(test, not(feature = "readline")))]
use crate::tasks::Task;

use super::in_out::InOut;
use super::tab_bar::IterationState;
use super::tab_bar::SearchState;
use super::tab_bar::TabState;


/// An enumeration comprising all custom events we support.
#[derive(Debug)]
pub enum Message {
  /// A message to ask a widget to select the task with the given
  /// `TaskId`.
  SelectTask(TaskId, IterationState),
  /// Search for a task containing the given string in its summary and
  /// select it.
  SearchTask(String, SearchState, IterationState),
  /// The tab with the given `Id` has selected the task as indicated by
  /// `SelectTask` or `SearchTask`.
  SelectedTask(Id),
  /// Set the state of the input/output area.
  SetInOut(InOut),
  /// Change the state of the input/output area to Clear, unless the
  /// generation ID supplied does not match the current generation ID.
  /// This message is internal to the InOutArea, there is no need for
  /// other clients to use it.
  ClearInOut(usize),
  /// Text has been entered.
  EnteredText(String),
  /// Text input has been canceled.
  InputCanceled,
  /// A message used to collect the state from the `TabBar`. The flag
  /// indicates whether the `TermUi` issued the message as part of a
  /// "save" operation. If it is false, the UI will just ignore the
  /// response to this message and let it bubble up.
  CollectState(bool),
  /// The response to the `CollectState` message.
  CollectedState(TabState),
  /// A message used to collect the state of all tabs.
  GetTabState(TabState, IterationState),
  /// A indication that some component changed and that we should
  /// re-render everything.
  Updated,
  /// Retrieve the current set of tasks.
  #[cfg(all(test, not(feature = "readline")))]
  GetTasks,
  /// The response to the `GetTasks` message.
  #[cfg(all(test, not(feature = "readline")))]
  GotTasks(Vec<Task>),
  /// Retrieve the current state of the input/output area.
  #[cfg(all(test, not(feature = "readline")))]
  GetInOut,
  /// The response to the `GetInOut` message.
  #[cfg(all(test, not(feature = "readline")))]
  GotInOut(InOut),
}

impl Message {
  /// Check whether the message is the `Updated` variant.
  pub fn is_updated(&self) -> bool {
    if let Message::Updated = self {
      true
    } else {
      false
    }
  }
}
