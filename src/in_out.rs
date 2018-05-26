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
  update: bool,
}

impl InOutArea {
  /// Create a new input/output area object.
  pub fn new(parent: &mut WidgetRef, id: Id) -> Self {
    InOutArea {
      id: id,
      parent_id: parent.as_id(),
      in_out: InOut::Clear,
      update: true,
    }
  }

  fn handle_in_out_event(&mut self, data: Box<Any>) -> (Option<MetaEvent>, bool) {
    match data.downcast::<InOut>() {
      Ok(in_out) => {
        let update = if *in_out != self.in_out {
          self.in_out = *in_out;
          true
        } else {
          false
        };
        (None, update)
      },
      Err(data) => (Some(Event::Custom(data).into()), false),
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
    let (result, update) = match event {
      Event::KeyDown(key) |
      Event::KeyUp(key) => {
        match key {
          Key::Char('\n') => {
            let s = if let InOut::Input(ref s) = self.in_out {
              s.clone()
            } else {
              panic!("In/out area not used for input.");
            };

            self.in_out = InOut::Clear;
            cap.focus(&self.parent_id);
            // Send the content of the input/output area to the parent
            // widget. It can then do whatever it pleases with it.
            let event = Event::Custom(Box::new(InOut::Input(s))).into();
            (Some(event), true)
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
            (None, true)
          },
          Key::Esc => {
            self.in_out = InOut::Clear;
            cap.focus(&self.parent_id);
            (None, true)
          },
          _ => (None, false),
        }
      },
      Event::Custom(data) => self.handle_in_out_event(data),
    };

    self.update = update;
    result
  }
}
