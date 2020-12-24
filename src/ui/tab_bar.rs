// tab_bar.rs

// *************************************************************************
// * Copyright (C) 2018-2020 Daniel Mueller (deso@posteo.net)              *
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
use std::rc::Rc;

use cell::RefCell;

use gui::Cap;
use gui::ChainEvent;
use gui::derive::Widget;
use gui::EventChain;
use gui::Handleable;
use gui::Id;
use gui::MutCap;
use gui::UiEvent;
use gui::UiEvents;
use gui::Widget;

use crate::query::Query;

use super::event::Event;
use super::event::EventUpdate;
use super::event::Key;
use super::in_out::InOut;
use super::iteration::IterationState as IterationStateT;
use super::message::Message;
use crate::tasks::Tasks;
use super::task_list_box::TaskListBox;
use super::task_list_box::TaskListBoxData;


/// The iteration state as used by a `TabBar`.
pub type IterationState = IterationStateT<Id>;


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
  /// selected last, and the third one represents the iteration state.
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

/// A search as used by a `TabBar`.
type Search = SearchT<IterationState>;


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


/// The state used for collecting the queries from the various
/// `TaskListBox` objects.
#[derive(Debug)]
pub struct TabState {
  /// The accumulation of queries gathered from the individual
  /// `TaskListBox` objects.
  pub queries: Vec<(Query, Option<usize>)>,
  /// The currently selected query.
  pub selected: Option<usize>,
  /// Flag indicating whether we retrieve the state for a save operation
  /// or not.
  // TODO: That feels more like a hack than anything else. The problem
  //       we are trying to solve is that the TermUi but also tests use
  //       an event containing this struct. The TermUi uses it as part
  //       of the save-to-file functionality and will handle the
  //       corresponding event. However, for tests we need a dedicated
  //       way to retrieve the same information without the UI
  //       intercepting and handling the event on its own (as it did not
  //       issue it).
  pub for_save: bool,
}


/// Sanitize a selection index.
fn sanitize_selection(selection: isize, count: usize) -> usize {
  if count == 0 {
    0
  } else {
    max(0, min(count as isize - 1, selection)) as usize
  }
}


/// The data associated with a `TabBar`.
pub struct TabBarData {
  /// The list of tabs, with title and widget ID.
  tabs: Vec<(String, Id)>,
  /// The index of the currently selected tab.
  selection: isize,
  /// The index of the previously selected tab.
  prev_selection: isize,
  /// An object representing a search.
  search: Search,
}

impl TabBarData {
  /// Create a new `TabBarData` object.
  pub fn new() -> Self {
    Self {
      tabs: Default::default(),
      selection: 0,
      prev_selection: 0,
      search: SearchT::Unset,
    }
  }

  /// Retrieve the index of the currently selected tab.
  pub fn selection(&self) -> usize {
    let count = self.tabs.iter().len();
    sanitize_selection(self.selection, count)
  }

  /// Retrieve the `Id` of the selected tab.
  pub fn selected_tab(&self) -> Id {
    self.tabs[self.selection()].1
  }
}

/// A widget representing a tabbed container for other widgets.
#[derive(Debug, Widget)]
#[gui(Event = Event)]
pub struct TabBar {
  id: Id,
}

impl TabBar {
  /// Create a new `TabBar` widget.
  pub fn new(
    id: Id,
    cap: &mut dyn MutCap<Event>,
    tasks: Rc<RefCell<Tasks>>,
    queries: Vec<(Query, Option<usize>)>,
    selected: Option<usize>,
  ) -> Self {
    let count = queries.len();
    let selected = selected
      .map(|x| min(x, isize::MAX as usize))
      .unwrap_or(0) as isize;
    let selected = sanitize_selection(selected, count);

    let tabs = queries
      .into_iter()
      .enumerate()
      .map(|(i, (query, task))| {
        let name = query.name().to_string();
        let tasks = tasks.clone();
        let task_list = cap.add_widget(
          id,
          Box::new(|| Box::new(TaskListBoxData::new(tasks, query))),
          Box::new(move |id, cap| Box::new(TaskListBox::new(id, cap, task))),
        );

        if i == selected {
          cap.focus(task_list);
        } else {
          cap.hide(task_list);
        }
        (name, task_list)
      }).collect();

    let tab_bar = Self { id };
    let data = tab_bar.data_mut::<TabBarData>(cap);
    data.tabs = tabs;
    data.selection = selected as isize;
    data.prev_selection = selected as isize;

    tab_bar
  }

  /// Handle a `Message::SearchTask` event.
  fn handle_search_task(
    &self,
    cap: &mut dyn MutCap<Event>,
    string: String,
    search_state: SearchState,
    mut iter_state: IterationState,
  ) -> Option<UiEvents<Event>> {
    let data = self.data_mut::<TabBarData>(cap);
    match data.search {
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
        if iter_state.has_cycled(data.tabs.iter().len()) {
          data.search = SearchT::State(string.clone(), search_state, iter_state);

          let error = format!("Text '{}' not found", string);
          let event = Message::SetInOut(InOut::Error(error));
          Some(UiEvent::Custom(Box::new(event)).into())
        } else if iter_state.has_advanced() {
          debug_assert!(search_state.is_first());

          let iter = data.tabs.iter().map(|x| x.1);
          let new_idx = iter_state.normalize(iter);
          let tab = data.tabs[new_idx].1;

          let event = Message::SearchTask(string, SearchState::First, iter_state);
          let event = UiEvent::Returnable(self.id, tab, Box::new(event));
          Some(ChainEvent::Event(event))
        } else {
          iter_state.reset_cycled();
          data.search = SearchT::State(string, search_state, iter_state);
          None
        }
      },
      SearchT::Unset |
      SearchT::Preparing(..) |
      SearchT::State(..) => panic!("invalid search state"),
    }
  }

  /// Handle a custom event.
  fn handle_custom_event(
    &self,
    mut event: Box<Message>,
    cap: &mut dyn MutCap<Event>,
  ) -> Option<UiEvents<Event>> {
    match *event {
      Message::SelectTask(_, ref mut state) => {
        let data = self.data::<TabBarData>(cap);
        let iter = data.tabs.iter().map(|x| x.1);
        let new_idx = state.normalize(iter.clone());

        if !state.has_cycled(iter.len()) {
          let tab = data.tabs[new_idx].1;
          let event = UiEvent::Directed(tab, event);
          Some(ChainEvent::Event(event))
        } else {
          None
        }
      },
      Message::SelectedTask(widget_id) => {
        let data = self.data::<TabBarData>(cap);
        let select = data.tabs.iter().position(|x| x.1 == widget_id).unwrap();
        let update = self.set_select(cap, select as isize);
        (None as Option<Event>).maybe_update(update)
      },
      Message::EnteredText(mut string) => {
        let data = self.data_mut::<TabBarData>(cap);
        if !string.is_empty() && !data.tabs.is_empty() {
          string.make_ascii_lowercase();

          let reverse = data.search.take().is_reverse();
          let tab = data.selected_tab();
          let mut state = IterationState::new(tab);
          state.reverse(reverse);

          let event1 = Message::SetInOut(InOut::Search(string.clone()));
          let event1 = UiEvent::Custom(Box::new(event1));

          let event2 = Message::SearchTask(string, SearchState::Current, state);
          let event2 = UiEvent::Returnable(self.id, tab, Box::new(event2));

          Some(UiEvents::from(event1).chain(event2)).update()
        } else {
          None
        }
      },
      Message::InputCanceled => None,
      Message::CollectState(for_save) => {
        let data = self.data::<TabBarData>(cap);
        if let Some((_, tab)) = data.tabs.first() {
          let tab_state = TabState{
            queries: Vec::new(),
            selected: Some(self.selection(cap)),
            for_save,
          };
          let iter_state = IterationState::new(*tab);
          let event = Message::GetTabState(tab_state, iter_state);
          Some(UiEvent::Returnable(self.id, *tab, Box::new(event)).into())
        } else {
          // If there are no tabs there are no queries -- respond
          // directly.
          let tab_state = TabState{
            queries: Vec::new(),
            selected: None,
            for_save,
          };
          let event = Message::CollectedState(tab_state);
          Some(UiEvent::Custom(Box::new(event)).into())
        }
      },
      Message::GetTabState(tab_state, mut iter_state) => {
        debug_assert!(iter_state.has_advanced());

        let data = self.data::<TabBarData>(cap);
        // If we have covered all tabs then send back the queries.
        if iter_state.is_last(data.tabs.iter().len()) {
          let event = Message::CollectedState(tab_state);
          Some(UiEvent::Custom(Box::new(event)).into())
        } else {
          let iter = data.tabs.iter().map(|x| x.1);
          let new_idx = iter_state.normalize(iter);
          let tab = data.tabs[new_idx].1;

          let event = Message::GetTabState(tab_state, iter_state);
          Some(UiEvent::Returnable(self.id, tab, Box::new(event)).into())
        }
      },
      Message::SearchTask(string, search_state, iter_state) => {
        self.handle_search_task(cap, string, search_state, iter_state)
      },
      _ => Some(UiEvent::Custom(event).into()),
    }
  }

  /// Retrieve an iterator over the names of all the tabs.
  pub fn iter<'slf>(&'slf self, cap: &'slf dyn Cap) -> impl ExactSizeIterator<Item=&'slf String> {
    let data = self.data::<TabBarData>(cap);
    data.tabs.iter().map(|(x, _)| x)
  }

  /// Retrieve the index of the currently selected tab.
  pub fn selection(&self, cap: &dyn Cap) -> usize {
    let data = self.data::<TabBarData>(cap);
    data.selection()
  }

  /// Change the currently selected tab.
  fn set_select(&self, cap: &mut dyn MutCap<Event>, selection: isize) -> bool {
    let data = self.data::<TabBarData>(cap);
    let count = data.tabs.iter().len();
    let old_selection = sanitize_selection(data.selection, count);
    let new_selection = sanitize_selection(selection, count);

    if new_selection != old_selection {
      let selected = data.selected_tab();
      cap.hide(selected);

      let data = self.data_mut::<TabBarData>(cap);
      data.prev_selection = data.selection;
      data.selection = selection;

      let selected = data.selected_tab();
      cap.focus(selected);
      true
    } else {
      false
    }
  }

  /// Change the currently selected tab.
  fn select(&self, cap: &mut dyn MutCap<Event>, change: isize) -> bool {
    let data = self.data::<TabBarData>(cap);
    let count = data.tabs.iter().len();
    let selection = sanitize_selection(data.selection, count);
    let new_selection = selection as isize + change;
    self.set_select(cap, new_selection)
  }

  /// Select the previously selected tab.
  fn select_previous(&self, cap: &mut dyn MutCap<Event>) -> bool {
    let data = self.data::<TabBarData>(cap);
    let selection = data.prev_selection;
    self.set_select(cap, selection)
  }

  /// Swap the currently selected tab with the one to its left or right.
  fn swap(&self, cap: &mut dyn MutCap<Event>, left: bool) -> bool {
    let data = self.data_mut::<TabBarData>(cap);
    let count = data.tabs.iter().len();
    let old_selection = sanitize_selection(data.selection, count);
    let selection = data.selection + if left { -1 } else { 1 };
    let new_selection = sanitize_selection(selection, count);

    if new_selection != old_selection {
      data.tabs.swap(old_selection, new_selection);
      data.selection = selection;
      true
    } else {
      false
    }
  }
}

impl Handleable<Event> for TabBar {
  /// Check for new input and react to it.
  fn handle(&self, cap: &mut dyn MutCap<Event>, event: Event) -> Option<UiEvents<Event>> {
    let data = self.data_mut::<TabBarData>(cap);
    match event {
      Event::Key(key, _) => {
        match key {
          Key::Char('1') => (None as Option<Event>).maybe_update(self.set_select(cap, 0)),
          Key::Char('2') => (None as Option<Event>).maybe_update(self.set_select(cap, 1)),
          Key::Char('3') => (None as Option<Event>).maybe_update(self.set_select(cap, 2)),
          Key::Char('4') => (None as Option<Event>).maybe_update(self.set_select(cap, 3)),
          Key::Char('5') => (None as Option<Event>).maybe_update(self.set_select(cap, 4)),
          Key::Char('6') => (None as Option<Event>).maybe_update(self.set_select(cap, 5)),
          Key::Char('7') => (None as Option<Event>).maybe_update(self.set_select(cap, 6)),
          Key::Char('8') => (None as Option<Event>).maybe_update(self.set_select(cap, 7)),
          Key::Char('9') => (None as Option<Event>).maybe_update(self.set_select(cap, 8)),
          Key::Char('0') => (None as Option<Event>).maybe_update(self.set_select(cap, isize::MAX)),
          Key::Char('`') => (None as Option<Event>).maybe_update(self.select_previous(cap)),
          Key::Char('h') => (None as Option<Event>).maybe_update(self.select(cap, -1)),
          Key::Char('l') => (None as Option<Event>).maybe_update(self.select(cap, 1)),
          Key::Char('H') => (None as Option<Event>).maybe_update(self.swap(cap, true)),
          Key::Char('L') => (None as Option<Event>).maybe_update(self.swap(cap, false)),
          Key::Char('n') |
          Key::Char('N') => {
            let event = match data.search.take() {
              SearchT::Preparing(..) |
              SearchT::Unset => {
                data.search = SearchT::Unset;

                let error = InOut::Error("Nothing to search for".to_string());
                let event = Message::SetInOut(error);
                UiEvent::Custom(Box::new(event)).into()
              },
              SearchT::Taken => panic!("invalid search state"),
              SearchT::State(string, search_state, mut iter_state) => {
                let iter = data.tabs.iter().map(|x| x.1);
                let new_idx = iter_state.normalize(iter);
                let tab = data.tabs[new_idx].1;
                let reverse = key == Key::Char('N');
                iter_state.reverse(reverse);

                let event1 = Message::SetInOut(InOut::Search(string.clone()));
                let event1 = UiEvent::Custom(Box::new(event1));

                let event2 = Message::SearchTask(string, search_state, iter_state);
                let event2 = UiEvent::Returnable(self.id, tab, Box::new(event2));

                UiEvents::from(event1).chain(event2)
              },
            };
            Some(event)
          },
          Key::Char('/') |
          Key::Char('?') => {
            let reverse = key == Key::Char('?');
            data.search = SearchT::Preparing(reverse);

            let event = Message::SetInOut(InOut::Input("".to_string(), 0));
            let event = UiEvent::Custom(Box::new(event));
            Some(event.into())
          },
          _ => Some(event.into()),
        }
      },
    }
  }

  /// Handle a custom event.
  fn handle_custom(
    &self,
    cap: &mut dyn MutCap<Event>,
    event: Box<dyn Any>,
  ) -> Option<UiEvents<Event>> {
    match event.downcast::<Message>() {
      Ok(e) => self.handle_custom_event(e, cap),
      Err(e) => panic!("Received unexpected custom event: {:?}", e),
    }
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  type TestIterationState = IterationStateT<u16>;


  #[test]
  fn search_take() {
    let mut search = SearchT::Unset::<u16>;
    assert_eq!(search.take(), SearchT::Unset);
    assert_eq!(search, SearchT::Taken);

    let mut search = SearchT::Taken::<u16>;
    assert_eq!(search.take(), SearchT::Taken);
    assert_eq!(search, SearchT::Taken);

    let iter_state = TestIterationState::new(42);
    let mut search = SearchT::State("test".to_string(), SearchState::First, iter_state);

    match search.take() {
      SearchT::State(string, search_state, iter_state) => {
        assert_eq!(string, "test");
        assert_eq!(search_state, SearchState::First);
        assert_eq!(iter_state, TestIterationState::new(42));
      },
      _ => panic!(),
    }
    assert_eq!(search, SearchT::Taken);
  }
}
