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
use std::mem::replace;

use gui::Cap;
use gui::ChainEvent;
use gui::Event;
use gui::EventChain;
use gui::Handleable;
use gui::Id;
use gui::Key;
use gui::UiEvent;
use gui::UiEvents;
use gui_derive::GuiWidget;

use crate::event::EventUpdate;
use crate::in_out::InOut;
use crate::selection::SelectionState as SelectionStateT;
use crate::state::TaskState;
use crate::state::UiState;
use crate::task_list_box::TaskListBox;
use crate::termui::TermUiEvent;


/// The selection state as used by a `TabBar`.
pub type SelectionState = SelectionStateT<Id>;


/// An enum representing the states a search can be in.
#[derive(Debug, PartialEq)]
enum SearchT<T>
where
  T: PartialEq,
{
  /// No search is currently in progress.
  Unset,
  /// The input/output widget has been asked for the string to search
  /// for, but at this point we don't have it. We only know whether to
  /// search clock-wise or counter clock-wise (i.e., reverse).
  Preparing(bool),
  /// A search was started but the state is currently being transferred
  /// to another widget.
  // Note that in principle we could use the Unset variant to fudge this
  // case, but we would like to assert different invariants when the
  // state is taken versus when it was never actually set.
  Taken,
  /// The full state of a search. The first value is the (lowercased)
  /// text to search for, the second one represents the task that was
  /// selected last, and the third one represents the selection state.
  State(String, SearchState, T),
}

impl<T> SearchT<T>
where
  T: PartialEq,
{
  /// Take the value, leave the `Taken` variant in its place.
  fn take(&mut self) -> SearchT<T> {
    replace(self, SearchT::Taken)
  }

  /// Check whether the search is meant to be a reverse one or not.
  ///
  /// # Panics
  ///
  /// Panics if the search is in anything but the `SearchT::Preparing`
  /// state.
  fn is_reverse(&self) -> bool {
    match *self {
      SearchT::Preparing(reverse) => reverse,
      SearchT::Unset |
      SearchT::Taken |
      SearchT::State(..) => panic!("invalid search state"),
    }
  }
}

/// The selection state as used by a `TabBar`.
type Search = SearchT<SelectionState>;


/// An enum capturing the search behavior on an individual tab.
#[derive(Debug, PartialEq)]
pub enum SearchState {
  /// Start the search at the currently selected task.
  Current,
  /// Start the search at the first task in the query being displayed.
  First,
  /// Start the search at the task with the given index.
  Task(usize),
}

impl SearchState {
  /// Check if the state has the `First` variant active.
  fn is_first(&self) -> bool {
    match self {
      SearchState::First => true,
      _ => false,
    }
  }
}


/// Sanitize a selection index.
fn sanitize_selection(selection: isize, count: usize) -> usize {
  if count == 0 {
    0
  } else {
    max(0, min(count as isize - 1, selection)) as usize
  }
}


/// A widget representing a tabbed container for other widgets.
#[derive(Debug, GuiWidget)]
pub struct TabBar {
  id: Id,
  tabs: Vec<(String, Id)>,
  selection: isize,
  prev_selection: isize,
  search: Search,
}

impl TabBar {
  /// Create a new `TabBar` widget.
  pub fn new(id: Id, cap: &mut dyn Cap, task_state: &TaskState, ui_state: &UiState) -> Self {
    let selection = 0;
    // TODO: We really should not be cloning the queries to use here.
    let tabs = ui_state
      .queries()
      .cloned()
      .enumerate()
      .map(|(i, query)| {
        let name = query.name().to_string();
        let mut query = Some(query);
        let task_list = cap.add_widget(id, &mut |id, _cap| {
          Box::new(TaskListBox::new(id, task_state.tasks(), query.take().unwrap()))
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
      selection: selection as isize,
      prev_selection: selection as isize,
      search: SearchT::Unset,
    }
  }

  /// Handle a `TermUiEvent::SearchTask` event.
  fn handle_search_task(&mut self,
                        string: String,
                        search_state: SearchState,
                        mut selection_state: SelectionState) -> Option<UiEvents> {
    match self.search {
      SearchT::Taken => {
        // We have to distinguish three cases here, in order:
        // 1) No matching task was to be found on all tabs, i.e., we
        //    wrapped around already. In this case we should display an
        //    error.
        // 2) No task matching the search string was found on the tab we
        //    just issued the search to, in which case we need to try
        //    the next tab.
        // 3) The next task in line was found and selected, in which
        //    case we just store the search state and wait for
        //    additional user input to select the next one or similar.
        if selection_state.has_cycled(self.tabs.iter().len()) {
          self.search = SearchT::State(string.clone(), search_state, selection_state);

          let error = format!("Text '{}' not found", string).to_string();
          let event = TermUiEvent::SetInOut(InOut::Error(error));
          Some(UiEvent::Custom(Box::new(event)).into())
        } else if selection_state.has_advanced() {
          debug_assert!(search_state.is_first());

          let iter = self.tabs.iter().map(|x| x.1);
          let new_idx = selection_state.normalize(iter);
          let tab = self.tabs.get(new_idx).unwrap().1;

          let event = TermUiEvent::SearchTask(string, SearchState::First, selection_state);
          let event = UiEvent::Returnable(self.id, tab, Box::new(event));
          Some(ChainEvent::Event(event))
        } else {
          selection_state.reset_cycled();
          self.search = SearchT::State(string, search_state, selection_state);
          None
        }
      },
      SearchT::Unset |
      SearchT::Preparing(..) |
      SearchT::State(..) => panic!("invalid search state"),
    }
  }

  /// Handle a custom event.
  fn handle_custom_event(&mut self,
                         event: Box<TermUiEvent>,
                         cap: &mut dyn Cap) -> Option<UiEvents> {
    // See Rust issue #16223: We cannot properly move out of an enum
    // where one part of a tuple inside does not implement Copy. This
    // screws up the `SearchTask` case. Use identity function trick here
    // to work around this problem.
    match { *event } {
      TermUiEvent::SelectTask(task_id, mut state) => {
        let iter = self.tabs.iter().map(|x| x.1);
        let new_idx = state.normalize(iter.clone());

        if !state.has_cycled(iter.len()) {
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
      TermUiEvent::EnteredText(mut string) => {
        if !string.is_empty() && !self.tabs.is_empty() {
          string.make_ascii_lowercase();

          let reverse = self.search.take().is_reverse();
          let tab = self.selected_tab();
          let mut state = SelectionState::new(tab);
          state.reverse(reverse);

          let event1 = TermUiEvent::SetInOut(InOut::Search(string.clone()));
          let event1 = UiEvent::Custom(Box::new(event1));

          let event2 = TermUiEvent::SearchTask(string, SearchState::Current, state);
          let event2 = UiEvent::Returnable(self.id, tab, Box::new(event2));

          Some(UiEvents::from(event1).chain(event2)).update()
        } else {
          None
        }
      },
      TermUiEvent::InputCanceled => None,
      TermUiEvent::SearchTask(string, search_state, selection_state) => {
        self.handle_search_task(string, search_state, selection_state)
      },
      // Unfortunately, due to Rust issue #16223 and the work around we
      // have in place we need an additional allocation here.
      event => Some(UiEvent::Custom(Box::new(event)).into()),
    }
  }

  /// Retrieve an iterator over the names of all the tabs.
  pub fn iter(&self) -> impl ExactSizeIterator<Item=&String> {
    self.tabs.iter().map(|(x, _)| x)
  }

  /// Retrieve the index of the currently selected tab.
  pub fn selection(&self) -> usize {
    let count = self.tabs.iter().len();
    sanitize_selection(self.selection, count)
  }

  /// Retrieve the `Id` of the selected tab.
  fn selected_tab(&self) -> Id {
    self.tabs[self.selection()].1
  }

  /// Change the currently selected tab.
  fn set_select(&mut self, selection: isize, cap: &mut dyn Cap) -> bool {
    let count = self.tabs.iter().len();
    let old_selection = sanitize_selection(self.selection, count);
    let new_selection = sanitize_selection(selection, count);

    if new_selection != old_selection {
      cap.hide(self.selected_tab());
      self.prev_selection = self.selection;
      self.selection = selection;
      cap.focus(self.selected_tab());
      true
    } else {
      false
    }
  }

  /// Change the currently selected tab.
  fn select(&mut self, change: isize, cap: &mut dyn Cap) -> bool {
    let count = self.tabs.iter().len();
    let selection = sanitize_selection(self.selection, count);
    let new_selection = selection as isize + change;
    self.set_select(new_selection, cap)
  }

  /// Select the previously selected tab.
  fn select_previous(&mut self, cap: &mut dyn Cap) -> bool {
    let selection = self.prev_selection;
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
          Key::Char('n') |
          Key::Char('N') => {
            let event = match self.search.take() {
              SearchT::Unset => {
                self.search = SearchT::Unset;

                let error = InOut::Error("Nothing to search for".to_string());
                let event = TermUiEvent::SetInOut(error);
                UiEvent::Custom(Box::new(event)).into()
              },
              SearchT::Taken |
              SearchT::Preparing(..) => panic!("invalid search state"),
              SearchT::State(string, search_state, mut selection_state) => {
                let iter = self.tabs.iter().map(|x| x.1);
                let new_idx = selection_state.normalize(iter);
                let tab = self.tabs.get(new_idx).unwrap().1;
                let reverse = key == Key::Char('N');
                selection_state.reverse(reverse);

                let event1 = TermUiEvent::SetInOut(InOut::Search(string.clone()));
                let event1 = UiEvent::Custom(Box::new(event1));

                let event2 = TermUiEvent::SearchTask(string, search_state, selection_state);
                let event2 = UiEvent::Returnable(self.id, tab, Box::new(event2));

                UiEvents::from(event1).chain(event2)
              },
            };
            Some(event)
          },
          Key::Char('/') |
          Key::Char('?') => {
            let reverse = key == Key::Char('?');
            self.search = SearchT::Preparing(reverse);

            let event = TermUiEvent::SetInOut(InOut::Input("".to_string(), 0));
            let event = UiEvent::Custom(Box::new(event));
            Some(event.into())
          },
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


#[cfg(test)]
mod tests {
  use super::*;

  type TestSelectionState = SelectionStateT<u16>;


  #[test]
  fn search_take() {
    let mut search = SearchT::Unset::<u16>;
    assert_eq!(search.take(), SearchT::Unset);
    assert_eq!(search, SearchT::Taken);

    let mut search = SearchT::Taken::<u16>;
    assert_eq!(search.take(), SearchT::Taken);
    assert_eq!(search, SearchT::Taken);

    let selection_state = TestSelectionState::new(42);
    let mut search = SearchT::State("test".to_string(), SearchState::First, selection_state);

    match search.take() {
      SearchT::State(string, search_state, selection_state) => {
        assert_eq!(string, "test");
        assert_eq!(search_state, SearchState::First);
        assert_eq!(selection_state, TestSelectionState::new(42));
      },
      _ => assert!(false),
    }
    assert_eq!(search, SearchT::Taken);
  }
}
