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

use gui::UiEvent;

use crate::tasks::Id as TaskId;
#[cfg(all(test, not(feature = "readline")))]
use crate::tasks::Task;

use super::event::Event;
use super::in_out::InOut;
use super::tab_bar::SearchState;
use super::tab_bar::TabState;


/// An enumeration comprising all custom events we support.
#[derive(Debug)]
pub enum Message {
  /// A message to ask a widget to select the task with the given
  /// `TaskId`. The last argument is used to indicate that a task with
  /// the given ID has been selected.
  SelectTask(TaskId, bool),
  /// Search for a task containing the given string in its summary and
  /// select it. The last argument indicates whether we search in
  /// reverse order (true) or not (false).
  SearchTask(String, SearchState, bool),
  /// Set the state of the input/output area.
  SetInOut(InOut),
  /// Text has been entered.
  EnteredText(String),
  /// Text input has been canceled.
  InputCanceled,
  /// A message used to collect the state from the `TabBar`.
  CollectState,
  /// The response to the `CollectState` message.
  CollectedState(TabState),
  /// A message used to collect the state of all tabs.
  GetTabState(TabState),
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

/// A trait for converting something into an `Option<UiEvent<Event>>`.
pub trait MessageExt {
  /// Potentially convert an optional `Message` into the
  /// `Message::Updated` variant.
  fn maybe_update(self, update: bool) -> Option<Message>;

  /// Convert an optional message into an optional event.
  fn into_event(self) -> Option<UiEvent<Event>>;
}

impl MessageExt for Option<Message> {
  fn maybe_update(self, update: bool) -> Option<Message> {
    match self {
      Some(Message::Updated) => self,
      None => {
        if update {
          Some(Message::Updated)
        } else {
          None
        }
      },
      Some(..) => panic!("Unexpected message: {:?}", self),
    }
  }

  fn into_event(self) -> Option<UiEvent<Event>> {
    match self {
      Some(m @ Message::Updated) => Some(UiEvent::Custom(Box::new(m))),
      None => None,
      m => panic!("Message cannot be converted to event: {:?}", m),
    }
  }
}
