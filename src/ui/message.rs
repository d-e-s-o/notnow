// Copyright (C) 2020-2024 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::rc::Rc;

use gui::Id;

use crate::tasks::Task;

use super::event::Event;
use super::event::Ids;
use super::in_out::InOut;
use super::tab_bar::SearchState;
use super::tab_bar::TabState;


/// An enumeration comprising all custom events we support.
#[derive(Debug)]
pub enum Message {
  /// A message to ask a widget to select the task with the given
  /// `TaskId`. The last argument is used to indicate that a task with
  /// the given ID has been selected.
  SelectTask(Rc<Task>, bool),
  /// Initiate the search of a task based on a string.
  StartTaskSearch(String),
  /// Search for a task containing the given string in its summary and
  /// select it. The second to last argument indicates whether we search
  /// in reverse order (true) or not (false). The last argument
  /// determines whether we accept only an exact match (true) or merely
  /// require a substring match (false).
  SearchTask(String, SearchState, bool, bool),
  /// Copy a task for later paste.
  CopyTask(Task),
  /// Retrieve the copied task, if any.
  GetCopiedTask(Option<Task>),
  /// Edit the details associated with a task.
  EditDetails(Rc<Task>, Task),
  /// Edit the tags associated with a task.
  EditTags(Rc<Task>, Task),
  /// Update a task.
  UpdateTask(Rc<Task>, Task),
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
  /// An indication that one or more widgets changed and that we should
  /// re-render them.
  Updated(Ids),
  /// Retrieve the current set of tasks.
  #[cfg(all(test, not(feature = "readline")))]
  GetTasks,
  /// The response to the `GetTasks` message.
  #[cfg(all(test, not(feature = "readline")))]
  GotTasks(Vec<Rc<Task>>),
  /// Retrieve the current state of the input/output area.
  #[cfg(all(test, not(feature = "readline")))]
  GetInOut,
  /// The response to the `GetInOut` message.
  #[cfg(all(test, not(feature = "readline")))]
  GotInOut(InOut),
}

impl Message {
  /// Create the `Message::Updated` variant with a single `Id`.
  #[inline]
  pub fn updated(id: Id) -> Self {
    Self::Updated(Ids::One(id))
  }
}


/// A trait for converting something into an `Option<Event>`.
pub trait MessageExt {
  /// Merge the `Message::Updated` variants of this message and the
  /// provided one.
  fn maybe_update(self, updated: Option<Message>) -> Option<Message>;

  /// Convert an optional message into an optional event.
  fn into_event(self) -> Option<Event>;
}

impl MessageExt for Option<Message> {
  fn maybe_update(self, message: Option<Message>) -> Option<Message> {
    match (self, message) {
      (Some(Message::Updated(ids1)), Some(Message::Updated(ids2))) => {
        Some(Message::Updated(ids1.merge_with(ids2)))
      },
      (None, Some(Message::Updated(ids))) => Some(Message::Updated(ids)),
      (Some(Message::Updated(ids)), None) => Some(Message::Updated(ids)),
      (None, None) => None,
      (m1, m2) => {
        debug_assert!(
          false,
          "Cannot update non-update messages: `{m1:?}` & `{m2:?}`"
        );
        None
      },
    }
  }

  fn into_event(self) -> Option<Event> {
    match self {
      Some(Message::Updated(ids)) => Some(Event::Updated(ids)),
      None => None,
      message => panic!("Message cannot be converted to event: {message:?}"),
    }
  }
}
