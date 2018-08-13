// tab_bar.rs

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
use gui::Handleable;
use gui::Id;
use gui::Key;
use gui::MetaEvent;
use gui::UiEvent;

use event::EventUpdated;
use state::State;
use task_list_box::TaskListBox;
use termui::TermUiEvent;


/// Sanitize a selection index.
fn sanitize_selection(selection: isize, count: usize) -> usize {
  max(0, min(count as isize - 1, selection)) as usize
}


/// A widget representing a tabbed container for other widgets.
#[derive(Debug, GuiWidget)]
pub struct TabBar {
  id: Id,
  tabs: Vec<(String, Id)>,
  offset: Cell<usize>,
  selection: usize,
}

impl TabBar {
  /// Create a new `TabBar` widget.
  pub fn new(id: Id, cap: &mut Cap, state: &State) -> Self {
    let selection = 0;
    // TODO: We need a dynamic mechanism to retrieve all Query objects
    //       we are interested in.
    let mut queries = vec![
      ("all".to_string(), state.tasks()),
    ];
    let tabs = queries
      .drain(..)
      .enumerate()
      .map(|(i, (name, query))| {
        let mut query = Some(query);
        let task_list = cap.add_widget(id, &mut |id, _cap| {
          Box::new(TaskListBox::new(id, query.take().unwrap()))
        });

        if i == selection {
          cap.focus(task_list);
        } else {
          cap.hide(task_list);
        }
        (name, task_list)
      }).collect();

    TabBar {
      id: id,
      tabs: tabs,
      offset: Cell::new(0),
      selection: selection,
    }
  }

  /// Handle a custom event.
  fn handle_custom_event(&mut self, event: Box<TermUiEvent>) -> Option<MetaEvent> {
    match *event {
      TermUiEvent::AddTaskResp(_) => {
        // Forward the response to the currently active `TaskListBox`.
        let tab = self.selected_tab();
        let event = UiEvent::Custom(tab, event);
        Some(MetaEvent::UiEvent(event))
      },
      _ => Some(Event::Custom(event).into()),
    }
  }

  /// Retrieve an iterator over the names of all the tabs.
  pub fn iter(&self) -> impl Iterator<Item=&String> {
    self.tabs.iter().map(|(x, _)| x)
  }

  /// Retrieve the current tab offset.
  ///
  /// The offset indicates the tab at which to start displaying. Note
  /// that for various reasons such as resizing events the returned
  /// index should be sanitized via `sanitize_offset` before usage.
  pub fn offset(&self) -> usize {
    self.offset.get()
  }

  /// Adjust the tab offset to use.
  pub fn reoffset(&self, offset: usize) {
    self.offset.set(offset)
  }

  /// Retrieve the index of the currently selected tab.
  pub fn selection(&self) -> usize {
    self.selection
  }

  /// Retrieve the `Id` of the selected tab.
  fn selected_tab(&self) -> Id {
    self.tabs[self.selection].1
  }

  /// Change the currently selected tab.
  fn select(&mut self, change: isize) -> bool {
    let count = self.iter().count();
    let old_selection = self.selection;
    let new_selection = self.selection as isize + change;
    self.selection = sanitize_selection(new_selection, count);

    self.selection != old_selection
  }
}

impl Handleable for TabBar {
  /// Check for new input and react to it.
  fn handle(&mut self, event: Event, _cap: &mut Cap) -> Option<MetaEvent> {
    match event {
      Event::KeyDown(key) |
      Event::KeyUp(key) => {
        match key {
          Key::Char('h') => (None as Option<Event>).maybe_update(self.select(-1)),
          Key::Char('l') => (None as Option<Event>).maybe_update(self.select(1)),
          _ => Some(event.into()),
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
