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

use event::EventUpdated;
use termui::TermUiEvent;


/// An object representing the in/out area within the TermUi.
#[derive(Debug, Eq, PartialEq)]
pub enum InOut {
  Saved,
  Error(String),
  Input(String),
  Clear,
}


/// A widget representing an input/output and status area.
#[derive(Debug, GuiWidget)]
pub struct InOutArea {
  id: Id,
  in_out: InOut,
}

impl InOutArea {
  /// Create a new input/output area object.
  pub fn new(id: Id) -> Self {
    InOutArea {
      id: id,
      in_out: InOut::Clear,
    }
  }

  /// Handle a custom event.
  fn handle_custom_event(&mut self, event: Box<TermUiEvent>) -> Option<MetaEvent> {
    match *event {
      TermUiEvent::SetInOut(in_out) => {
        if in_out != self.in_out {
          self.in_out = in_out;
          (None as Option<Event>).update()
        } else {
          None
        }
      },
      _ => Some(Event::Custom(event).into()),
    }
  }

  /// Focus the previously focused widget or the parent.
  fn restore_focus(&mut self, cap: &mut Cap) {
    let to_focus = cap.last_focused().or_else(|| cap.parent_id(self.id));
    match to_focus {
      Some(to_focus) => cap.focus(to_focus),
      // Really should never happen. We are not the root widget so at
      // the very least there must be a parent to focus.
      None => assert!(false, "No previous widget to focus"),
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
        match key {
          Key::Return => {
            let s = if let InOut::Input(ref s) = self.in_out {
              s.clone()
            } else {
              panic!("In/out area not used for input.");
            };

            self.in_out = InOut::Clear;
            self.restore_focus(cap);
            let event = Event::Custom(Box::new(TermUiEvent::AddTask(s)));
            Some(event).update()
          },
          Key::Char(c) => {
            self.in_out = InOut::Input(match self.in_out {
              InOut::Input(ref mut s) => {
                s.push(c);
                s.clone()
              },
              InOut::Clear => c.to_string(),
              _ => panic!("In/out area not used for input."),
            });
            (None as Option<Event>).update()
          },
          Key::Backspace => {
            self.in_out = InOut::Input(match self.in_out {
              InOut::Input(ref mut s) => {
                s.pop();
                s.clone()
              },
              InOut::Clear => "".to_string(),
              _ => panic!("In/out area not used for input."),
            });
            (None as Option<Event>).update()
          },
          Key::Esc => {
            self.in_out = InOut::Clear;
            self.restore_focus(cap);
            (None as Option<Event>).update()
          },
          _ => None,
        }
      },
      Event::Custom(data) => {
        match data.downcast::<TermUiEvent>() {
          Ok(e) => self.handle_custom_event(e),
          Err(e) => panic!("Received unexpected custom event: {:?}", e),
        }
      },
    }
  }
}
