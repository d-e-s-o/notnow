// in_out.rs

// *************************************************************************
// * Copyright (C) 2018-2019 Daniel Mueller (deso@posteo.net)              *
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
use super::termui::TermUiEvent;


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
      return true
    } else {
      return false
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


/// A widget representing an input/output and status area.
#[derive(Debug, Widget)]
#[gui(Event = "Event")]
pub struct InOutArea {
  id: Id,
  prev_focused: Option<Id>,
  in_out: InOutState,
  #[cfg(feature = "readline")]
  readline: Readline,
}

impl InOutArea {
  /// Create a new input/output area object.
  pub fn new(id: Id, cap: &mut dyn MutCap<Event>) -> Self {
    // Install a hook to be able to reset the input/output area into
    // "clear" state on every key press.
    let _ = cap.hook_events(id, Some(&InOutArea::handle_hooked_event));

    InOutArea {
      id: id,
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

  /// Handle a hooked event.
  fn handle_hooked_event(widget: &mut dyn Widget<Event>,
                         event: &Event,
                         _cap: &dyn Cap) -> Option<UiEvents<Event>> {
    let in_out = widget.downcast_mut::<InOutArea>();
    if let Some(in_out) = in_out {
      // We basically schedule a "callback" by virtue of sending an
      // event to ourselves. This event will be received only after we
      // handled any other key events, meaning we have full information
      // about what happened and can determine whether we ultimately
      // want to set our state to "Clear" or not.
      match event {
        Event::Key(..) => {
          let event = Box::new(TermUiEvent::ClearInOut(in_out.in_out.gen));
          Some(UiEvent::Directed(in_out.id, event).into())
        },
      }
    } else {
      panic!("Widget {:?} is unexpected", in_out)
    }
  }

  /// Handle a custom event.
  fn handle_custom_event(&mut self,
                         event: Box<TermUiEvent>,
                         cap: &mut dyn MutCap<Event>) -> Option<UiEvents<Event>> {
    match *event {
      TermUiEvent::SetInOut(in_out) => {
        if let InOut::Input(ref s, idx) = in_out {
          // TODO: It is not nice that we allow clients to provide
          //       potentially unsanitized inputs.
          debug_assert!(idx <= s.len());

          self.prev_focused = cap.focused();
          cap.focus(self.id);
        };
        self.change_state(in_out)
      },
      TermUiEvent::ClearInOut(gen) => {
        // We only change our state to "Clear" if the generation number
        // is still the same, meaning that we did not change our state
        // between receiving the event hook and retrieving this event.
        if self.in_out.gen == gen {
          match self.in_out.get() {
            InOut::Saved |
            InOut::Search(_) |
            InOut::Error(_) => self.change_state(InOut::Clear),
            InOut::Input(..) |
            InOut::Clear => None,
          }
        } else {
          None
        }
      },
      #[cfg(all(test, not(feature = "readline")))]
      TermUiEvent::GetInOut => {
        let resp = TermUiEvent::GotInOut(self.in_out.get().clone());
        Some(UiEvent::Custom(Box::new(resp)).into())
      },
      _ => Some(UiEvent::Custom(event).into()),
    }
  }

  /// Finish text input by changing the internal state and emitting an event.
  fn finish_input(&mut self,
                  string: Option<String>,
                  cap: &mut dyn MutCap<Event>) -> Option<UiEvents<Event>> {
    let update = self.change_state(InOut::Clear);
    let widget = self.restore_focus(cap);
    let event = if let Some(s) = string {
      Box::new(TermUiEvent::EnteredText(s))
    } else {
      Box::new(TermUiEvent::InputCanceled)
    };
    debug_assert!(update.is_some());
    Some(ChainEvent::from(UiEvent::Directed(widget, event))).chain(update)
  }

  /// Handle a key press.
  #[cfg(not(feature = "readline"))]
  fn handle_key(&mut self,
                mut s: String,
                mut idx: usize,
                key: Key,
                _raw: &(),
                cap: &mut dyn MutCap<Event>) -> Option<UiEvents<Event>> {
    match key {
      Key::Esc |
      Key::Char('\n') => {
        let string = if key == Key::Char('\n') { Some(s) } else { None };
        self.finish_input(string, cap)
      },
      // We cannot easily handle multi byte Unicode graphemes with
      // Rust's standard library, so just ignore everything that is
      // represented as more than one byte (the `unicode_segmentation`
      // would allow us to circumvent this restriction).
      Key::Char(c) if c.len_utf8() == 1 => {
        s.insert(idx, c);
        self.change_state(InOut::Input(s, idx + 1))
      },
      Key::Backspace => {
        if idx > 0 {
          let _ = s.remove(idx - 1);
          idx -= 1;
        }
        self.change_state(InOut::Input(s, idx))
      },
      Key::Delete => {
        if idx < s.len() {
          let _ = s.remove(idx);
          if idx > s.len() {
            idx -= 1;
          }
        }
        self.change_state(InOut::Input(s, idx))
      },
      Key::Left => {
        if idx > 0 {
          self.change_state(InOut::Input(s, idx - 1))
        } else {
          None
        }
      },
      Key::Right => {
        if idx < s.len() {
          self.change_state(InOut::Input(s, idx + 1))
        } else {
          None
        }
      },
      Key::Home => {
        if idx != 0 {
          self.change_state(InOut::Input(s, 0))
        } else {
          None
        }
      },
      Key::End => {
        let length = s.len();
        if idx != length {
          self.change_state(InOut::Input(s, length))
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
  fn handle_key(&mut self,
                _s: String,
                idx: usize,
                key: Key,
                raw: &[u8],
                cap: &mut dyn MutCap<Event>) -> Option<UiEvents<Event>> {
    match self.readline.feed(raw) {
      Some(line) => self.finish_input(Some(line.into_string().unwrap()), cap),
      None => {
        let (s_, idx_) = self.readline.peek(|s, pos| (s.to_owned(), pos));
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
          self.readline = Readline::new();
          self.finish_input(None, cap)
        } else {
          self.change_state(InOut::Input(s_.into_string().unwrap(), idx_))
        }
      },
    }
  }

  /// Focus the previously focused widget or the parent.
  fn restore_focus(&mut self, cap: &mut dyn MutCap<Event>) -> Id {
    match self.prev_focused {
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
  pub fn state(&self) -> &InOut {
    &self.in_out.get()
  }
}

impl Handleable<Event> for InOutArea {
  /// Handle an event.
  fn handle(&mut self, event: Event, cap: &mut dyn MutCap<Event>) -> Option<UiEvents<Event>> {
    match event {
      Event::Key(key, raw) => {
        let (s, idx) = if let InOut::Input(s, idx) = self.in_out.get() {
          (s.clone(), *idx)
        } else {
          panic!("In/out area not used for input.");
        };

        self.handle_key(s, idx, key, &raw, cap)
      },
    }
  }

  /// Handle a custom event.
  fn handle_custom(&mut self,
                   event: Box<dyn Any>,
                   cap: &mut dyn MutCap<Event>) -> Option<UiEvents<Event>> {
    match event.downcast::<TermUiEvent>() {
      Ok(e) => self.handle_custom_event(e, cap),
      Err(e) => panic!("Received unexpected custom event: {:?}", e),
    }
  }
}
