// Copyright (C) 2018-2024 Daniel Mueller (deso@posteo.net)
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

use crate::text::EditableText;

use super::event::Event;
use super::event::Key;
use super::message::Message;
use super::message::MessageExt;
use super::modal::Modal;


/// An object representing the in/out area within the `TermUi`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InOut {
  Saved,
  Search(String),
  Error(String),
  Input(EditableText),
  Clear,
}

#[cfg(feature = "readline")]
impl InOut {
  /// Check whether the `InOut` state is `Input`.
  fn is_input(&self) -> bool {
    matches!(self, InOut::Input(..))
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
        if let InOut::Input(text) = &in_out {
          // We clear the undo buffer if we transition from a non-Input
          // state to an Input state. Input-to-Input transitions are
          // believed to be those just updating the text the user is
          // working on already.
          let cstr = CString::new(text.as_str()).unwrap();
          let clear_undo = !self.in_out.get().is_input();
          self
            .readline
            .reset(cstr, text.selection_byte_index(), clear_undo);
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
        // pre-hook such that we can decide whether to set our state to
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
  #[cfg(not(feature = "readline"))]
  async fn handle_key(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    mut text: EditableText,
    key: Key,
    _raw: &(),
  ) -> Option<Message> {
    let data = self.data_mut::<InOutAreaData>(cap);
    match key {
      Key::Esc | Key::Char('\n') => {
        let string = if key == Key::Char('\n') {
          Some(text.into_string())
        } else {
          None
        };
        self.finish_input(cap, string).await
      },
      Key::Char(c) => {
        let () = text.insert_char(c);
        data.change_state(InOut::Input(text))
      },
      Key::Backspace => {
        if text.selection() > 0 {
          let () = text.select_prev();
          let () = text.remove_char();
          data.change_state(InOut::Input(text))
        } else {
          None
        }
      },
      Key::Delete => {
        if text.selection() < text.len() {
          let () = text.remove_char();
          data.change_state(InOut::Input(text))
        } else {
          None
        }
      },
      Key::Left => {
        if text.selection() > 0 {
          let () = text.select_prev();
          data.change_state(InOut::Input(text))
        } else {
          None
        }
      },
      Key::Right => {
        if text.selection() < text.len() {
          let () = text.select_next();
          data.change_state(InOut::Input(text))
        } else {
          None
        }
      },
      Key::Home => {
        if text.selection() != 0 {
          let () = text.select_start();
          data.change_state(InOut::Input(text))
        } else {
          None
        }
      },
      Key::End => {
        if text.selection() != text.len() {
          let () = text.select_end();
          data.change_state(InOut::Input(text))
        } else {
          None
        }
      },
      _ => None,
    }
  }

  /// Handle a key press.
  #[cfg(feature = "readline")]
  async fn handle_key(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    text: EditableText,
    key: Key,
    raw: &[u8],
  ) -> Option<Message> {
    let data = self.data_mut::<InOutAreaData>(cap);
    match data.readline.feed(raw) {
      Some(text) => {
        self
          .finish_input(cap, Some(text.into_string().unwrap()))
          .await
      },
      None => {
        let (s, idx) = data.readline.peek(|s, pos| (s.to_owned(), pos));
        // We treat Esc a little specially. In a vi-mode enabled
        // configuration of libreadline Esc cancels input mode when we
        // are in it, and does nothing otherwise. That is what we are
        // interested in here. So we peek at the index we get and see
        // if it changed (because leaving input mode moves the cursor
        // to the left by one). If nothing changed, then we actually
        // cancel the text input. That is not the nicest logic, but
        // the only way we have found that accomplishes what we want.
        if key == Key::Esc && idx == text.selection_byte_index() {
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
          let mut text = EditableText::from_string(s.to_string_lossy());
          let () = text.select_byte_index(idx);

          data.change_state(InOut::Input(text))
        }
      },
    }
  }

  /// Retrieve the input/output area's current state.
  pub fn state<'slf>(&'slf self, cap: &'slf dyn Cap) -> &'slf InOut {
    let data = self.data::<InOutAreaData>(cap);
    data.in_out.get()
  }
}

impl Modal for InOutArea {
  fn prev_focused(&self, cap: &dyn Cap) -> Option<Id> {
    self.data::<InOutAreaData>(cap).prev_focused
  }

  fn set_prev_focused(&self, cap: &mut dyn MutCap<Event, Message>, focused: Option<Id>) {
    let data = self.data_mut::<InOutAreaData>(cap);
    data.prev_focused = focused;
  }
}

#[async_trait(?Send)]
impl Handleable<Event, Message> for InOutArea {
  /// Handle an event.
  async fn handle(&self, cap: &mut dyn MutCap<Event, Message>, event: Event) -> Option<Event> {
    match event {
      Event::Key(key, raw) => {
        let data = self.data::<InOutAreaData>(cap);
        let text = if let InOut::Input(text) = data.in_out.get() {
          text.clone()
        } else {
          panic!("In/out area not used for input.");
        };

        self.handle_key(cap, text, key, &raw).await.into_event()
      },
      _ => Some(event),
    }
  }

  /// React to a message.
  async fn react(&self, message: Message, cap: &mut dyn MutCap<Event, Message>) -> Option<Message> {
    match message {
      Message::SetInOut(in_out) => {
        if matches!(in_out, InOut::Input(..)) {
          self.make_focused(cap);
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
