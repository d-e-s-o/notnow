// in_out.rs

// *************************************************************************
// * Copyright (C) 2018 Daniel Mueller (deso@posteo.net)                   *
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

use gui::Cap;
use gui::Event;
use gui::Handleable;
use gui::Id;
use gui::Key;
use gui::MetaEvent;
use gui::UiEvent;
use gui::Widget;

use event::EventUpdated;
use termui::TermUiEvent;


/// An object representing the in/out area within the TermUi.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InOut {
  Saved,
  Error(String),
  Input(String, usize),
  Clear,
}


/// A widget representing an input/output and status area.
#[derive(Debug, GuiWidget)]
pub struct InOutArea {
  id: Id,
  prev_focused: Option<Id>,
  in_out: InOut,
}

impl InOutArea {
  /// Create a new input/output area object.
  pub fn new(id: Id, cap: &mut Cap) -> Self {
    // Install a hook to be able to reset the input/output area into
    // "clear" state on every key press.
    cap.hook_events(id, Some(&InOutArea::handle_hooked_event));

    InOutArea {
      id: id,
      prev_focused: None,
      in_out: InOut::Clear,
    }
  }

  /// Conditionally change the `InOut` state of the widget.
  fn change_state(&mut self, in_out: InOut) -> Option<MetaEvent> {
    if in_out != self.in_out {
      self.in_out = in_out;
      (None as Option<Event>).update()
    } else {
      None
    }
  }

  /// Handle a hooked event.
  fn handle_hooked_event(widget: &mut Widget, event: &Event, cap: &Cap) -> Option<MetaEvent> {
    let in_out = widget.downcast_mut::<InOutArea>();
    if let Some(in_out) = in_out {
      // If we are focused then text is being entered and we should not
      // clear the state.
      if !cap.is_focused(in_out.id) {
        match event {
          Event::KeyDown(_) |
          Event::KeyUp(_) => in_out.change_state(InOut::Clear),
          _ => None,
        }
      } else {
        None
      }
    } else {
      panic!("Widget {:?} is unexpected", in_out)
    }
  }

  /// Handle a custom event.
  fn handle_custom_event(&mut self, event: Box<TermUiEvent>, cap: &mut Cap) -> Option<MetaEvent> {
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
      #[cfg(test)]
      TermUiEvent::GetInOut => {
        let resp = TermUiEvent::GetInOutResp(self.in_out.clone());
        Some(Event::Custom(Box::new(resp)).into())
      },
      _ => Some(Event::Custom(event).into()),
    }
  }

  /// Focus the previously focused widget or the parent.
  fn restore_focus(&mut self, cap: &mut Cap) -> Id {
    let to_focus = self.prev_focused.or_else(|| cap.parent_id(self.id));
    match to_focus {
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
    &self.in_out
  }
}

impl Handleable for InOutArea {
  /// Handle an event.
  fn handle(&mut self, event: Event, cap: &mut Cap) -> Option<MetaEvent> {
    match event {
      Event::KeyDown(key) |
      Event::KeyUp(key) => {
        let (mut s, mut idx) = if let InOut::Input(s, idx) = &self.in_out {
          (s.clone(), *idx)
        } else {
          panic!("In/out area not used for input.");
        };

        match key {
          Key::Return => {
            self.in_out = InOut::Clear;

            let event = if let Some(id) = self.prev_focused {
              Some(UiEvent::Custom(id, Box::new(TermUiEvent::EnteredText(s))))
            } else {
              None
            };
            self.restore_focus(cap);
            event.update()
          },
          Key::Char(c) => {
            s.insert(idx, c);
            self.in_out = InOut::Input(s, idx + 1);
            (None as Option<Event>).update()
          },
          Key::Backspace => {
            if idx > 0 {
              s.remove(idx - 1);
              idx -= 1;
            }
            self.in_out = InOut::Input(s, idx);
            (None as Option<Event>).update()
          },
          Key::Delete => {
            if idx < s.len() {
              s.remove(idx);
              if idx > s.len() {
                idx -= 1;
              }
            }
            self.in_out = InOut::Input(s, idx);
            (None as Option<Event>).update()
          },
          Key::Left => {
            if idx > 0 {
              self.in_out = InOut::Input(s, idx - 1);
              (None as Option<Event>).update()
            } else {
              None
            }
          },
          Key::Right => {
            if idx < s.len() {
              self.in_out = InOut::Input(s, idx + 1);
              (None as Option<Event>).update()
            } else {
              None
            }
          },
          Key::Home => {
            if idx != 0 {
              self.in_out = InOut::Input(s, 0);
              (None as Option<Event>).update()
            } else {
              None
            }
          },
          Key::End => {
            let length = s.len();
            if idx != length {
              self.in_out = InOut::Input(s, length);
              (None as Option<Event>).update()
            } else {
              None
            }
          },
          Key::Esc => {
            self.in_out = InOut::Clear;
            let widget = self.restore_focus(cap);
            let event = Box::new(TermUiEvent::InputCanceled);
            Some(UiEvent::Custom(widget, event)).update()
          },
          _ => None,
        }
      },
      Event::Custom(data) => {
        match data.downcast::<TermUiEvent>() {
          Ok(e) => self.handle_custom_event(e, cap),
          Err(e) => panic!("Received unexpected custom event: {:?}", e),
        }
      },
    }
  }
}
