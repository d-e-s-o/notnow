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

use std::any::Any;
use std::cmp::max;
use std::cmp::min;
use std::isize;

use gui::Cap;
use gui::ChainEvent;
use gui::Event;
use gui::Handleable;
use gui::Id;
use gui::Key;
use gui::UiEvent;
use gui::UiEvents;

use event::EventUpdated;
use selection::SelectionState as SelectionStateT;
use state::State;
use task_list_box::TaskListBox;
use termui::TermUiEvent;


/// The selection state as used by a `TabBar`.
pub type SelectionState = SelectionStateT<Id>;


/// Sanitize a selection index.
fn sanitize_selection(selection: isize, count: usize) -> usize {
  max(0, min(count as isize - 1, selection)) as usize
}


/// A widget representing a tabbed container for other widgets.
#[derive(Debug, GuiWidget)]
pub struct TabBar {
  id: Id,
  tabs: Vec<(String, Id)>,
  selection: usize,
  prev_selection: usize,
}

impl TabBar {
  /// Create a new `TabBar` widget.
  pub fn new(id: Id, cap: &mut dyn Cap, state: &State) -> Self {
    let selection = 0;
    // TODO: We really should not be cloning the queries to use here.
    let tabs = state
      .queries()
      .cloned()
      .enumerate()
      .map(|(i, query)| {
        let name = query.name().to_string();
        let mut query = Some(query);
        let task_list = cap.add_widget(id, &mut |id, _cap| {
          Box::new(TaskListBox::new(id, state.tasks(), query.take().unwrap()))
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
      selection: selection,
      prev_selection: selection,
    }
  }

  /// Handle a custom event.
  fn handle_custom_event(&mut self,
                         event: Box<TermUiEvent>,
                         cap: &mut dyn Cap) -> Option<UiEvents> {
    match *event {
      TermUiEvent::SelectTask(task_id, mut state) => {
        let iter = self.tabs.iter().map(|x| x.1);
        let new_idx = state.normalize(iter);

        if !state.has_cycled() {
          let tab = self.tabs.get(new_idx).unwrap().1;
          let event = Box::new(TermUiEvent::SelectTask(task_id, state));
          let event = UiEvent::Directed(tab, event);
          Some(ChainEvent::Event(event))
        } else {
          None
        }
      },
      TermUiEvent::SelectedTask(widget_id) => {
        let select = self.tabs.iter().position(|x| x.1 == widget_id).unwrap();
        let update = self.set_select(select as isize, cap);
        (None as Option<Event>).maybe_update(update)
      },
      _ => Some(UiEvent::Custom(event).into()),
    }
  }

  /// Retrieve an iterator over the names of all the tabs.
  pub fn iter(&self) -> impl Iterator<Item=&String> {
    self.tabs.iter().map(|(x, _)| x)
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
  fn set_select(&mut self, new_selection: isize, cap: &mut dyn Cap) -> bool {
    let count = self.iter().count();
    let old_selection = self.selection;
    let new_selection = sanitize_selection(new_selection, count);

    if new_selection != old_selection {
      cap.hide(self.selected_tab());
      self.prev_selection = old_selection;
      self.selection = new_selection;
      cap.focus(self.selected_tab());
      true
    } else {
      false
    }
  }

  /// Change the currently selected tab.
  fn select(&mut self, change: isize, cap: &mut dyn Cap) -> bool {
    let new_selection = self.selection as isize + change;
    self.set_select(new_selection, cap)
  }

  /// Select the previously selected tab.
  fn select_previous(&mut self, cap: &mut dyn Cap) -> bool {
    let selection = self.prev_selection as isize;
    self.set_select(selection, cap)
  }
}

impl Handleable for TabBar {
  /// Check for new input and react to it.
  fn handle(&mut self, event: Event, cap: &mut dyn Cap) -> Option<UiEvents> {
    match event {
      Event::KeyDown(key) |
      Event::KeyUp(key) => {
        match key {
          Key::Char('1') => (None as Option<Event>).maybe_update(self.set_select(0, cap)),
          Key::Char('2') => (None as Option<Event>).maybe_update(self.set_select(1, cap)),
          Key::Char('3') => (None as Option<Event>).maybe_update(self.set_select(2, cap)),
          Key::Char('4') => (None as Option<Event>).maybe_update(self.set_select(3, cap)),
          Key::Char('5') => (None as Option<Event>).maybe_update(self.set_select(4, cap)),
          Key::Char('6') => (None as Option<Event>).maybe_update(self.set_select(5, cap)),
          Key::Char('7') => (None as Option<Event>).maybe_update(self.set_select(6, cap)),
          Key::Char('8') => (None as Option<Event>).maybe_update(self.set_select(7, cap)),
          Key::Char('9') => (None as Option<Event>).maybe_update(self.set_select(8, cap)),
          Key::Char('0') => (None as Option<Event>).maybe_update(self.set_select(isize::MAX, cap)),
          Key::Char('`') => (None as Option<Event>).maybe_update(self.select_previous(cap)),
          Key::Char('h') => (None as Option<Event>).maybe_update(self.select(-1, cap)),
          Key::Char('l') => (None as Option<Event>).maybe_update(self.select(1, cap)),
          _ => Some(event.into()),
        }
      },
    }
  }

  /// Handle a custom event.
  fn handle_custom(&mut self, event: Box<dyn Any>, cap: &mut dyn Cap) -> Option<UiEvents> {
    match event.downcast::<TermUiEvent>() {
      Ok(e) => self.handle_custom_event(e, cap),
      Err(e) => panic!("Received unexpected custom event: {:?}", e),
    }
  }
}
