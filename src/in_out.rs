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

use std::any::Any;

use gui::Cap;
use gui::Event;
use gui::Handleable;
use gui::Id;
use gui::Key;
use gui::MetaEvent;
use gui::WidgetRef;

use event::EventUpdated;


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
  parent_id: Id,
  in_out: InOut,
}

impl InOutArea {
  /// Create a new input/output area object.
  pub fn new(parent: &mut WidgetRef, id: Id) -> Self {
    InOutArea {
      id: id,
      parent_id: parent.as_id(),
      in_out: InOut::Clear,
    }
  }

  fn handle_in_out_event(&mut self, data: Box<Any>) -> Option<MetaEvent> {
    match data.downcast::<InOut>() {
      Ok(in_out) => {
        if *in_out != self.in_out {
          self.in_out = *in_out;
          (None as Option<Event>).update()
        } else {
          None
        }
      },
      Err(data) => Some(Event::Custom(data).into()),
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
            let to_focus = cap.last_focused().unwrap_or(self.parent_id);
            cap.focus(&to_focus);
            // Send the content of the input/output area to the parent
            // widget. It can then do whatever it pleases with it.
            let event = Event::Custom(Box::new(InOut::Input(s)));
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
            let to_focus = cap.last_focused().unwrap_or(self.parent_id);
            cap.focus(&to_focus);
            (None as Option<Event>).update()
          },
          _ => None,
        }
      },
      Event::Custom(data) => self.handle_in_out_event(data),
    }
  }
}
