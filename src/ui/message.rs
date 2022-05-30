// Copyright (C) 2020-2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::tasks::Id as TaskId;
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
  /// Initiate the search of a task based on a string.
  StartTaskSearch(String),
  /// Search for a task containing the given string in its summary and
  /// select it. The second to last argument indicates whether we search
  /// in reverse order (true) or not (false). The last argument
  /// determines whether we accept only an exact match (true) or merely
  /// require a substring match (false).
  SearchTask(String, SearchState, bool, bool),
  /// Edit the tags associated with a task.
  EditTags(Task),
  /// Update a task.
  UpdateTask(Task),
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
    matches!(self, Message::Updated)
  }
}

/// A trait for converting something into an `Option<Event>`.
pub trait MessageExt {
  /// Potentially convert an optional `Message` into the
  /// `Message::Updated` variant.
  fn maybe_update(self, update: bool) -> Option<Message>;

  /// Convert an optional message into an optional event.
  fn into_event(self) -> Option<Event>;
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

  fn into_event(self) -> Option<Event> {
    match self {
      Some(Message::Updated) => Some(Event::Updated),
      None => None,
      m => panic!("Message cannot be converted to event: {:?}", m),
    }
  }
}
