// Copyright (C) 2021 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use gui::Cap;
use gui::Id;
use gui::MutCap;
use gui::Widget;

use super::event::Event;
use super::message::Message;


/// A trait representing modal widgets, i.e., those that take the input
/// focus for a while and then restore it back to the previously
/// selected widget.
pub trait Modal: Widget<Event, Message> {
  /// Retrieve the previously focused widget.
  fn prev_focused(&self, cap: &dyn Cap) -> Option<Id>;

  /// Remember the previously focused widget.
  fn set_prev_focused(&self, cap: &mut dyn MutCap<Event, Message>, focused: Option<Id>);

  /// Make `self` the focused widget.
  fn make_focused(&self, cap: &mut dyn MutCap<Event, Message>) {
    let focused = cap.focused();
    cap.focus(self.id());

    self.set_prev_focused(cap, focused)
  }

  /// Focus the previously focused widget.
  fn restore_focus(&self, cap: &mut dyn MutCap<Event, Message>) -> Id {
    let prev_focused = self.prev_focused(cap);
    match prev_focused {
      Some(to_focus) => {
        cap.focus(to_focus);
        self.set_prev_focused(cap, None);
        to_focus
      },
      // We consider it a bug attempting to restore the focus without a
      // previously focused widget present.
      None => panic!("No previous widget to focus"),
    }
  }
}
