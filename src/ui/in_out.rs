// in_out.rs

// *************************************************************************
// * Copyright (C) 2018-2020 Daniel Mueller (deso@posteo.net)              *
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

use std::any::Any;
#[cfg(feature = "readline")]
use std::ffi::CString;

use async_trait::async_trait;

use gui::Cap;
use gui::ChainEvent;
use gui::derive::Widget;
use gui::Handleable;
use gui::Id;
use gui::MutCap;
use gui::OptionChain;
use gui::UiEvent;
use gui::UiEvents;
use gui::Widget;

#[cfg(feature = "readline")]
use rline::Readline;

use super::event::Event;
use super::event::EventUpdate;
use super::event::Key;
use super::message::Message;


/// An object representing the in/out area within the TermUi.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InOut {
  Saved,
  Search(String),
  Error(String),
  Input(String, usize),
  Clear,
}

#[cfg(feature = "readline")]
impl InOut {
  /// Check whether the `InOut` state is `Input`.
  fn is_input(&self) -> bool {
    if let InOut::Input(..) = &self {
      true
    } else {
      false
    }
  }
}


#[derive(Debug)]
struct InOutState {
  /// The actual `InOut` state we have.
  in_out: InOut,
  /// The generation ID. The ID is incremented on every change being
  /// made to `in_out`.
  gen: usize,
}

impl InOutState {
  /// Retrieve the current `InOut` state.
  fn get(&self) -> &InOut {
    &self.in_out
  }

  /// Update the current `InOut` state.
  fn set(&mut self, in_out: InOut) {
    self.in_out = in_out;
    self.bump()
  }

  /// Bump the generation ID.
  fn bump(&mut self) {
    self.gen += 1;
  }
}

impl Default for InOutState {
  fn default() -> Self {
    Self {
      in_out: InOut::Clear,
      gen: 0,
    }
  }
}


/// Retrieve the index and length of a character at the given byte
/// index.
#[cfg(any(test, not(feature = "readline")))]
fn str_char(s: &str, pos: usize) -> (usize, usize) {
  for (idx, c) in s.char_indices() {
    if pos < idx + c.len_utf8() {
      return (idx, c.len_utf8())
    }
  }
  (pos, 1)
}


/// The data associated with an `InOutArea`.
pub struct InOutAreaData {
  /// The ID of the widget that was focused before the input/output area
  /// received the input focus.
  prev_focused: Option<Id>,
  /// The state of the area.
  in_out: InOutState,
  /// A readline object used for input.
  #[cfg(feature = "readline")]
  readline: Readline,
}

impl InOutAreaData {
  /// Create a new `InOutAreaData` object.
  pub fn new() -> Self {
    Self {
      prev_focused: None,
      in_out: Default::default(),
      #[cfg(feature = "readline")]
      readline: Readline::new(),
    }
  }

  /// Conditionally change the `InOut` state of the widget.
  fn change_state(&mut self, in_out: InOut) -> Option<UiEvents<Event>> {
    // We received a request to change the state. Unconditionally bump
    // the generation it has, irrespective of whether we actually change
    // it (which we don't, if the new state is equal to what we already
    // have).
    self.in_out.bump();

    if in_out != *self.in_out.get() {
      #[cfg(feature = "readline")]
      {
        if let InOut::Input(s, idx) = &in_out {
          // We clear the undo buffer if we transition from a non-Input
          // state to an Input state. Input-to-Input transitions are
          // believed to be those just updating the text the user is
          // working on already.
          let cstr = CString::new(s.clone()).unwrap();
          let clear_undo = !self.in_out.get().is_input();
          self.readline.reset(cstr, *idx, clear_undo);
        }
      }
      self.in_out.set(in_out);
      (None as Option<Event>).update()
    } else {
      None
    }
  }
}


/// A widget representing an input/output and status area.
#[derive(Debug, Widget)]
#[gui(Event = Event, Message = Message)]
pub struct InOutArea {
  id: Id,
}

impl InOutArea {
  /// Create a new input/output area object.
  pub fn new(id: Id, cap: &mut dyn MutCap<Event, Message>) -> Self {
    // Install a hook to be able to reset the input/output area into
    // "clear" state on every key press.
    let _ = cap.hook_events(id, Some(&InOutArea::handle_hooked_event));
    Self { id }
  }

  /// Handle a hooked event.
  fn handle_hooked_event(
    widget: &dyn Widget<Event, Message>,
    cap: &mut dyn MutCap<Event, Message>,
    event: &Event,
  ) -> Option<UiEvents<Event>> {
    let in_out = widget.downcast_ref::<InOutArea>();
    if let Some(in_out) = in_out {
      // We basically schedule a "callback" by virtue of sending an
      // event to ourselves. This event will be received only after we
      // handled any other key events, meaning we have full information
      // about what happened and can determine whether we ultimately
      // want to set our state to "Clear" or not.
      match event {
        Event::Key(..) => {
          let data = in_out.data::<InOutAreaData>(cap);
          let event = Box::new(Message::ClearInOut(data.in_out.gen));
          Some(UiEvent::Directed(in_out.id, event).into())
        },
      }
    } else {
      panic!("Widget {:?} is unexpected", in_out)
    }
  }

  /// Handle a custom event.
  fn handle_custom_event(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    event: Box<Message>,
  ) -> Option<UiEvents<Event>> {
    let data = self.data_mut::<InOutAreaData>(cap);
    match *event {
      Message::SetInOut(in_out) => {
        if let InOut::Input(ref s, idx) = in_out {
          // TODO: It is not nice that we allow clients to provide
          //       potentially unsanitized inputs.
          debug_assert!(idx <= s.len());

          let focused = cap.focused();
          cap.focus(self.id);

          let data = self.data_mut::<InOutAreaData>(cap);
          data.prev_focused = focused;
        };

        let data = self.data_mut::<InOutAreaData>(cap);
        data.change_state(in_out)
      },
      Message::ClearInOut(gen) => {
        // We only change our state to "Clear" if the generation number
        // is still the same, meaning that we did not change our state
        // between receiving the event hook and retrieving this event.
        if data.in_out.gen == gen {
          match data.in_out.get() {
            InOut::Saved |
            InOut::Search(_) |
            InOut::Error(_) => data.change_state(InOut::Clear),
            InOut::Input(..) |
            InOut::Clear => None,
          }
        } else {
          None
        }
      },
      _ => Some(UiEvent::Custom(event).into()),
    }
  }

  /// Finish text input by changing the internal state and emitting an event.
  fn finish_input(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    string: Option<String>,
  ) -> Option<UiEvents<Event>> {
    let data = self.data_mut::<InOutAreaData>(cap);
    let update = data.change_state(InOut::Clear);
    let widget = self.restore_focus(cap);
    let event = if let Some(s) = string {
      Box::new(Message::EnteredText(s))
    } else {
      Box::new(Message::InputCanceled)
    };
    debug_assert!(update.is_some());
    Some(ChainEvent::from(UiEvent::Directed(widget, event))).chain(update)
  }

  /// Handle a key press.
  #[allow(clippy::trivially_copy_pass_by_ref)]
  #[cfg(not(feature = "readline"))]
  fn handle_key(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    mut s: String,
    mut idx: usize,
    key: Key,
    _raw: &(),
  ) -> Option<UiEvents<Event>> {
    let data = self.data_mut::<InOutAreaData>(cap);
    match key {
      Key::Esc |
      Key::Char('\n') => {
        let string = if key == Key::Char('\n') { Some(s) } else { None };
        self.finish_input(cap, string)
      },
      // We cannot easily handle multi byte Unicode graphemes with
      // Rust's standard library, so just ignore everything that is
      // represented as more than one byte (the `unicode_segmentation`
      // would allow us to circumvent this restriction).
      Key::Char(c) => {
        s.insert(idx, c);
        data.change_state(InOut::Input(s, idx + c.len_utf8()))
      },
      Key::Backspace => {
        if idx > 0 {
          let (i, len) = str_char(&s, idx - 1);
          let _ = s.remove(i);
          idx = idx.saturating_sub(len);
        }
        data.change_state(InOut::Input(s, idx))
      },
      Key::Delete => {
        if idx < s.len() {
          let _ = s.remove(idx);
        }
        data.change_state(InOut::Input(s, idx))
      },
      Key::Left => {
        if idx > 0 {
          idx = str_char(&s, idx - 1).0;
          data.change_state(InOut::Input(s, idx))
        } else {
          None
        }
      },
      Key::Right => {
        if idx < s.len() {
          let (idx, len) = str_char(&s, idx);
          debug_assert!(idx + len <= s.len());
          data.change_state(InOut::Input(s, idx + len))
        } else {
          None
        }
      },
      Key::Home => {
        if idx != 0 {
          data.change_state(InOut::Input(s, 0))
        } else {
          None
        }
      },
      Key::End => {
        let length = s.len();
        if idx != length {
          data.change_state(InOut::Input(s, length))
        } else {
          None
        }
      },
      _ => None,
    }
  }

  /// Handle a key press.
  #[allow(clippy::needless_pass_by_value)]
  #[cfg(feature = "readline")]
  fn handle_key(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    _s: String,
    idx: usize,
    key: Key,
    raw: &[u8],
  ) -> Option<UiEvents<Event>> {
    let data = self.data_mut::<InOutAreaData>(cap);
    match data.readline.feed(raw) {
      Some(line) => self.finish_input(cap, Some(line.into_string().unwrap())),
      None => {
        let (s_, idx_) = data.readline.peek(|s, pos| (s.to_owned(), pos));
        // We treat Esc a little specially. In a vi-mode enabled
        // configuration of libreadline Esc cancels input mode when we
        // are in it, and does nothing otherwise. That is what we are
        // interested in here. So we peek at the index we get and see
        // if it changed (because leaving input mode moves the cursor
        // to the left by one). If nothing changed, then we actually
        // cancel the text input. That is not the nicest logic, but
        // the only way we have found that accomplishes what we want.
        if key == Key::Esc && idx_ == idx {
          // TODO: We have a problem here. What may end up happening
          //       is that we disrupt libreadline's workflow by
          //       effectively canceling what it was doing. If, for
          //       instance, we were in vi-movement-mode and we simply
          //       stop the input process libreadline does not know
          //       about that and will stay in this mode. So next time
          //       we start editing again, we will still be in this
          //       mode. Unfortunately, rline's reset does not deal
          //       with this case (perhaps rightly so). For now, just
          //       create a new `Readline` context and that will take
          //       care of resetting things to the default (which is
          //       input mode).
          data.readline = Readline::new();
          self.finish_input(cap, None)
        } else {
          data.change_state(InOut::Input(s_.into_string().unwrap(), idx_))
        }
      },
    }
  }

  /// Focus the previously focused widget or the parent.
  fn restore_focus(&self, cap: &mut dyn MutCap<Event, Message>) -> Id {
    let data = self.data::<InOutAreaData>(cap);
    match data.prev_focused {
      Some(to_focus) => {
        cap.focus(to_focus);
        to_focus
      },
      // Really should never happen. We are not the root widget so at
      // the very least there must be a parent to focus.
      None => panic!("No previous widget to focus"),
    }
  }

  /// Retrieve the input/output area's current state.
  pub fn state<'slf>(&'slf self, cap: &'slf dyn Cap) -> &'slf InOut {
    let data = self.data::<InOutAreaData>(cap);
    &data.in_out.get()
  }
}

#[async_trait(?Send)]
impl Handleable<Event, Message> for InOutArea {
  /// Handle an event.
  async fn handle(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    event: Event,
  ) -> Option<UiEvents<Event>> {
    match event {
      Event::Key(key, raw) => {
        let data = self.data::<InOutAreaData>(cap);
        let (s, idx) = if let InOut::Input(s, idx) = data.in_out.get() {
          (s.clone(), *idx)
        } else {
          panic!("In/out area not used for input.");
        };

        self.handle_key(cap, s, idx, key, &raw)
      },
    }
  }

  /// Handle a custom event.
  async fn handle_custom(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    event: Box<dyn Any>,
  ) -> Option<UiEvents<Event>> {
    match event.downcast::<Message>() {
      Ok(e) => self.handle_custom_event(cap, e),
      Err(e) => panic!("Received unexpected custom event: {:?}", e),
    }
  }

  /// React to a message.
  #[allow(unused)]
  async fn react(&self, message: Message, cap: &mut dyn MutCap<Event, Message>) -> Option<Message> {
    match message {
      #[cfg(all(test, not(feature = "readline")))]
      Message::GetInOut => {
        let data = self.data::<InOutAreaData>(cap);
        Some(Message::GotInOut(data.in_out.get().clone()))
      },
      m => panic!("Received unexpected message: {:?}", m),
    }
  }
}


#[cfg(all(test, feature = "readline"))]
mod tests {
  use super::*;

  #[test]
  fn string_characters() {
    let s = "abödeägh";
    assert_eq!(str_char(s, 0), (0, 1));
    assert_eq!(str_char(s, 1), (1, 1));
    assert_eq!(str_char(s, 2), (2, 2));
    assert_eq!(str_char(s, 3), (2, 2));
    assert_eq!(str_char(s, 4), (4, 1));
    assert_eq!(str_char(s, 5), (5, 1));
    assert_eq!(str_char(s, 6), (6, 2));
    assert_eq!(str_char(s, 7), (6, 2));
    assert_eq!(str_char(s, 8), (8, 1));
  }
}
