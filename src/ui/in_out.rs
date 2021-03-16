// Copyright (C) 2018-2021 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

#[cfg(feature = "readline")]
use std::ffi::CString;
use std::future::Future;
use std::pin::Pin;

use async_trait::async_trait;

use gui::derive::Widget;
use gui::Cap;
use gui::Handleable;
use gui::Id;
use gui::MutCap;
use gui::Widget;

#[cfg(feature = "readline")]
use rline::Readline;

use super::event::Event;
use super::event::Key;
use super::message::Message;
use super::message::MessageExt;


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
  /// The generation number at which to clear the state.
  clear_gen: Option<usize>,
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
      clear_gen: None,
      in_out: Default::default(),
      #[cfg(feature = "readline")]
      readline: Readline::new(),
    }
  }

  /// Conditionally change the `InOut` state of the widget.
  fn change_state(&mut self, in_out: InOut) -> Option<Message> {
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
      Some(Message::Updated)
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
    cap.hook_events(id, Some(&InOutArea::handle_hooked_event));
    Self { id }
  }

  /// Handle a hooked event.
  fn handle_hooked_event<'f>(
    widget: &'f dyn Widget<Event, Message>,
    cap: &'f mut dyn MutCap<Event, Message>,
    event: Option<&'f Event>,
  ) -> Pin<Box<dyn Future<Output = Option<Event>> + 'f>> {
    Box::pin(async move {
      let data = cap
        .data_mut(widget.id())
        .downcast_mut::<InOutAreaData>()
        .unwrap();
      if let Some(event) = event {
        // We remember the generation number we had when we entered the
        // pre-hook such that we can decided whether to set our state to
        // "Clear" or not on the post-hook path.
        match event {
          Event::Key(..) => {
            data.clear_gen = Some(data.in_out.gen);
            None
          },
          Event::Updated | Event::Quit => None,
        }
      } else {
        // We only change our state to "Clear" if the generation number
        // is still the same, meaning that we did not change our state
        // between pre- and post-hook.
        if data.clear_gen.take() == Some(data.in_out.gen) {
          match data.in_out.get() {
            InOut::Saved | InOut::Search(_) | InOut::Error(_) => {
              data.change_state(InOut::Clear).map(|_| Event::Updated)
            },
            InOut::Input(..) | InOut::Clear => None,
          }
        } else {
          None
        }
      }
    })
  }

  /// Finish text input by changing the internal state and emitting an event.
  async fn finish_input(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    string: Option<String>,
  ) -> Option<Message> {
    let data = self.data_mut::<InOutAreaData>(cap);
    let updated1 = data
      .change_state(InOut::Clear)
      .map(|m| m.is_updated())
      .unwrap_or(false);
    let widget = self.restore_focus(cap);
    let message = if let Some(s) = string {
      Message::EnteredText(s)
    } else {
      Message::InputCanceled
    };

    let updated2 = cap
      .send(widget, message)
      .await
      .map(|m| m.is_updated())
      .unwrap_or(false);
    MessageExt::maybe_update(None, updated1 || updated2)
  }

  /// Handle a key press.
  #[allow(clippy::trivially_copy_pass_by_ref)]
  #[cfg(not(feature = "readline"))]
  async fn handle_key(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    mut s: String,
    mut idx: usize,
    key: Key,
    _raw: &(),
  ) -> Option<Message> {
    let data = self.data_mut::<InOutAreaData>(cap);
    match key {
      Key::Esc | Key::Char('\n') => {
        let string = if key == Key::Char('\n') {
          Some(s)
        } else {
          None
        };
        self.finish_input(cap, string).await
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
          s.remove(i);
          idx = idx.saturating_sub(len);
        }
        data.change_state(InOut::Input(s, idx))
      },
      Key::Delete => {
        if idx < s.len() {
          s.remove(idx);
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
  async fn handle_key(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    _s: String,
    idx: usize,
    key: Key,
    raw: &[u8],
  ) -> Option<Message> {
    let data = self.data_mut::<InOutAreaData>(cap);
    match data.readline.feed(raw) {
      Some(line) => {
        self
          .finish_input(cap, Some(line.into_string().unwrap()))
          .await
      },
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
          self.finish_input(cap, None).await
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
  async fn handle(&self, cap: &mut dyn MutCap<Event, Message>, event: Event) -> Option<Event> {
    match event {
      Event::Key(key, raw) => {
        let data = self.data::<InOutAreaData>(cap);
        let (s, idx) = if let InOut::Input(s, idx) = data.in_out.get() {
          (s.clone(), *idx)
        } else {
          panic!("In/out area not used for input.");
        };

        self.handle_key(cap, s, idx, key, &raw).await.into_event()
      },
      _ => Some(event),
    }
  }

  /// React to a message.
  async fn react(&self, message: Message, cap: &mut dyn MutCap<Event, Message>) -> Option<Message> {
    match message {
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
