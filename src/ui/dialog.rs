// Copyright (C) 2021 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use async_trait::async_trait;

use gui::derive::Widget;
use gui::Handleable;
use gui::Id;
use gui::MutCap;

use super::event::Event;
use super::message::Message;


/// The data associated with a `Dialog` widget.
#[derive(Debug)]
pub struct DialogData {}

impl DialogData {
  pub fn new() -> Self {
    Self {}
  }
}


/// A modal dialog used for editing a task's tags.
#[derive(Debug, Widget)]
#[gui(Event = Event, Message = Message)]
pub struct Dialog {
  id: Id,
}

impl Dialog {
  /// Create a new `Dialog`.
  pub fn new(id: Id) -> Self {
    Self { id }
  }
}

#[async_trait(?Send)]
impl Handleable<Event, Message> for Dialog {
  /// Handle an event.
  async fn handle(&self, _cap: &mut dyn MutCap<Event, Message>, event: Event) -> Option<Event> {
    Some(event)
  }
}
