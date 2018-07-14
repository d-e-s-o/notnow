// task_list_box.rs

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

use std::cell::Cell;
use std::cmp::max;
use std::cmp::min;

use gui::Cap;
use gui::Event;
use gui::EventChain;
use gui::Handleable;
use gui::Id;
use gui::Key;
use gui::MetaEvent;
use gui::UiEvent;
use gui::WidgetRef;

use event::EventUpdated;
use in_out::InOut;
use query::Query;


/// Sanitize a selection index.
fn sanitize_selection(selection: isize, count: usize) -> usize {
  max(0, min(count as isize - 1, selection)) as usize
}


/// A widget representing a list of `Task` objects.
#[derive(Debug, GuiWidget)]
pub struct TaskListBox {
  parent_id: Id,
  id: Id,
  in_out: Id,
  children: Vec<Id>,
  query: Query,
  offset: Cell<usize>,
  selection: usize,
}

impl TaskListBox {
  /// Create a new `TaskListBox` widget.
  pub fn new(parent: &mut WidgetRef, id: Id, in_out: Id, query: Query) -> Self {
    TaskListBox {
      parent_id: parent.as_id(),
      id: id,
      in_out: in_out,
      children: Vec::new(),
      query: query,
      offset: Cell::new(0),
      selection: 0,
    }
  }

  /// Retrieve the query associated with this widget.
  pub fn query(&self) -> Query {
    self.query.clone()
  }

  /// Retrieve the current view offset.
  ///
  /// The offset indicates the task at which to start displaying. Note
  /// that for various reasons such as resizing events the returned
  /// index should be sanitized via `sanitize_offset` before usage.
  pub fn offset(&self) -> usize {
    self.offset.get()
  }

  /// Adjust the view offset to use.
  pub fn reoffset(&self, offset: usize) {
    self.offset.set(offset)
  }

  /// Retrieve the current selection index.
  ///
  /// The selection index indicates the currently selected task.
  pub fn selection(&self) -> usize {
    self.selection
  }

  /// Change the currently selected task.
  fn select(&mut self, change: isize) -> bool {
    let count = self.query().count();
    let old_selection = self.selection;
    let new_selection = self.selection as isize + change;
    self.selection = sanitize_selection(new_selection, count);

    self.selection != old_selection
  }
}

impl Handleable for TaskListBox {
  /// Check for new input and react to it.
  fn handle(&mut self, event: Event, cap: &mut Cap) -> Option<MetaEvent> {
    match event {
      Event::KeyDown(key) |
      Event::KeyUp(key) => {
        match key {
          Key::Char('a') => {
            let event = UiEvent::Custom(self.in_out, Box::new(InOut::Input("".to_string())));
            cap.focus(&self.in_out);
            Some(event).update()
          },
          Key::Char('d') => {
            let clear = UiEvent::Custom(self.in_out, Box::new(InOut::Clear));
            if !self.query.is_empty() {
              let id = self.query().nth(self.selection).unwrap().id;
              let remove = UiEvent::Custom(self.id, Box::new(id));
              // The task will get removed so move the selection up by
              // one.
              self.select(-1);
              // We are about to remove a task. Always indicate that an
              // update is necessary here.
              Some(clear.chain(remove)).update()
            } else {
              Some(clear.into())
            }
          },
          Key::Char('j') => (None as Option<Event>).maybe_update(self.select(1)),
          Key::Char('k') => (None as Option<Event>).maybe_update(self.select(-1)),
          _ => Some(event.into()),
        }
      },
      Event::Custom(_) => Some(event.into()),
    }
  }
}
