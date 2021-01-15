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

use async_trait::async_trait;

use cell::RefCell;

use gui::Cap;
use gui::derive::Widget;
use gui::Handleable;
use gui::Id;
use gui::MutCap;
use gui::OptionChain;
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
use super::message::MessageExt;
use crate::tasks::Tasks;
use super::task_list_box::TaskListBox;
use super::task_list_box::TaskListBoxData;


/// The iteration state as used by a `TabBar`.
pub type IterationState = IterationStateT<Id>;


/// An enum representing the states a search can be in.
#[derive(Debug, PartialEq)]
enum Search {
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
  /// The (lowercased) text to search for.
  State(String),
}

impl Search {
  /// Take the value, leave the `Taken` variant in its place.
  fn take(&mut self) -> Self {
    replace(self, Search::Taken)
  }

  /// Check whether the search is meant to be a reverse one or not.
  ///
  /// # Panics
  ///
  /// Panics if the search is in anything but the `Search::Preparing`
  /// state.
  fn is_reverse(&self) -> bool {
    match *self {
      Search::Preparing(reverse) => reverse,
      Search::Unset | Search::Taken | Search::State(..) => panic!("invalid search state"),
    }
  }
}


/// An enum capturing the search behavior on an individual tab.
#[derive(Debug, PartialEq)]
pub enum SearchState {
  /// Start the search at the currently selected task.
  Current,
  /// Start right after the currently selected task.
  AfterCurrent,
  /// Start the search at the first task in the query being displayed.
  First,
  /// The search is done.
  Done,
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
}


/// Sanitize a selection index.
fn sanitize_selection(selection: isize, count: usize) -> usize {
  if count == 0 {
    0
  } else {
    max(0, min(count as isize - 1, selection)) as usize
  }
}

/// Provide a snapshot ready for a search, assuming the element at index
/// `selected` is selected and should be visited first.
fn search_snapshot<I, T>(iter: I, selected: usize, reverse: bool) -> Vec<T>
where
  I: Iterator<Item = T>,
  T: Copy,
{
  let mut snapshot = iter.collect::<Vec<_>>();

  let selected = if reverse {
    snapshot.reverse();
    // We need to adjust our selection index now that the elements got
    // reordered.
    snapshot.len() - 1 - selected
  } else {
    selected
  };
  snapshot.rotate_left(selected);

  // A search begins on the selected tab, but it may do so on a task
  // somewhere in the middle. Hence, once we visited all other tabs we
  // should return back to the initially selected one to cover the
  // remaining tasks.
  if let Some(first) = snapshot.first().copied() {
    snapshot.push(first);
  }
  snapshot
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
      search: Search::Unset,
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

  /// Provide a snapshot of the tabs, ready for a search starting with
  /// the selected one.
  pub fn search_snapshot(&self, reverse: bool) -> Vec<Id> {
    let tabs = self.tabs.iter().map(|(_, id)| id).copied();
    let selected = self.selection();
    search_snapshot(tabs, selected, reverse)
  }
}

/// A widget representing a tabbed container for other widgets.
#[derive(Debug, Widget)]
#[gui(Event = Event, Message = Message)]
pub struct TabBar {
  id: Id,
  in_out: Id,
}

impl TabBar {
  /// Create a new `TabBar` widget.
  pub fn new(
    id: Id,
    cap: &mut dyn MutCap<Event, Message>,
    in_out: Id,
    tasks: Rc<RefCell<Tasks>>,
    queries: Vec<(Query, Option<usize>)>,
    selected: Option<usize>,
  ) -> Self {
    let count = queries.len();
    let selected = selected
      .map(|x| min(x, isize::MAX as usize))
      .unwrap_or(0) as isize;
    let selected = sanitize_selection(selected, count);
    let tab_bar = id;

    let tabs = queries
      .into_iter()
      .enumerate()
      .map(|(i, (query, task))| {
        let name = query.name().to_string();
        let tasks = tasks.clone();
        let task_list = cap.add_widget(
          id,
          Box::new(|| Box::new(TaskListBoxData::new(tasks, query))),
          Box::new(move |id, cap| Box::new(TaskListBox::new(id, cap, tab_bar, in_out, task))),
        );

        if i == selected {
          cap.focus(task_list);
        } else {
          cap.hide(task_list);
        }
        (name, task_list)
      }).collect();

    let tab_bar = Self { id, in_out };
    let data = tab_bar.data_mut::<TabBarData>(cap);
    data.tabs = tabs;
    data.selection = selected as isize;
    data.prev_selection = selected as isize;

    tab_bar
  }

  /// Handle a `Message::SearchTask` event.
  async fn search_task(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    string: String,
    search_state: SearchState,
    reverse: bool,
  ) -> Option<Message> {
    let data = self.data_mut::<TabBarData>(cap);
    let tabs = data.search_snapshot(reverse);
    let mut message = Message::SearchTask(string.clone(), search_state, reverse);
    let mut found = false;

    for tab in tabs {
      // TODO: We may want to use the result?
      let _ = cap.call(tab, &mut message).await;

      if let Message::SearchTask(_, search_state, _) = &mut message {
        if let SearchState::Done = search_state {
          found = true;
          break
        }

        // After the first call (which started on the currently selected
        // task on the currently selected tab) we always continue with the
        // first task on any subsequent tab.
        *search_state = SearchState::First;
      } else {
        panic!("Received unexpected message: {:?}", message)
      }
    }

    if !found {
      let error = format!("Text '{}' not found", string);
      let message = Message::SetInOut(InOut::Error(error));
      let _ = cap.send(self.in_out, message).await;
    }

    let data = self.data_mut::<TabBarData>(cap);
    data.search = Search::State(string);
    // TODO: May need to return Updated?
    Some(Message::Updated)
  }

  /// Handle a custom event.
  async fn handle_custom_event(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    event: Box<Message>,
  ) -> Option<UiEvents<Event>> {
    match *event {
      Message::EnteredText(mut string) => {
        let data = self.data_mut::<TabBarData>(cap);
        if !string.is_empty() && !data.tabs.is_empty() {
          string.make_ascii_lowercase();

          let reverse = data.search.take().is_reverse();
          let message = Message::SetInOut(InOut::Search(string.clone()));
          let event1 = cap
            .send(self.in_out, message)
            .await
            .into_event()
            .map(UiEvents::from);

          let search_state = SearchState::Current;
          let event2 = self
            .search_task(cap, string, search_state, reverse)
            .await
            .into_event()
            .map(UiEvents::from);

          event1.update().chain(event2)
        } else {
          None
        }
      },
      Message::InputCanceled => None,
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
  fn set_select(&self, cap: &mut dyn MutCap<Event, Message>, selection: isize) -> bool {
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
  fn select(&self, cap: &mut dyn MutCap<Event, Message>, change: isize) -> bool {
    let data = self.data::<TabBarData>(cap);
    let count = data.tabs.iter().len();
    let selection = sanitize_selection(data.selection, count);
    let new_selection = selection as isize + change;
    self.set_select(cap, new_selection)
  }

  /// Select the previously selected tab.
  fn select_previous(&self, cap: &mut dyn MutCap<Event, Message>) -> bool {
    let data = self.data::<TabBarData>(cap);
    let selection = data.prev_selection;
    self.set_select(cap, selection)
  }

  /// Swap the currently selected tab with the one to its left or right.
  fn swap(&self, cap: &mut dyn MutCap<Event, Message>, left: bool) -> bool {
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

#[async_trait(?Send)]
impl Handleable<Event, Message> for TabBar {
  /// Check for new input and react to it.
  async fn handle(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    event: Event,
  ) -> Option<UiEvents<Event>> {
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
              Search::Preparing(..) |
              Search::Unset => {
                data.search = Search::Unset;

                let error = InOut::Error("Nothing to search for".to_string());
                let message = Message::SetInOut(error);
                cap
                  .send(self.in_out, message)
                  .await
                  .into_event()
                  .map(UiEvents::from)
              },
              Search::Taken => panic!("invalid search state"),
              Search::State(string) => {
                let reverse = key == Key::Char('N');
                let message = Message::SetInOut(InOut::Search(string.clone()));
                let event1 = cap
                  .send(self.in_out, message)
                  .await
                  .into_event()
                  .map(UiEvents::from);

                let search_state = SearchState::AfterCurrent;
                let event2 = self
                  .search_task(cap, string, search_state, reverse)
                  .await
                  .into_event()
                  .map(UiEvents::from);

                event1.chain(event2)
              },
            };
            event
          },
          Key::Char('/') |
          Key::Char('?') => {
            let reverse = key == Key::Char('?');
            data.search = Search::Preparing(reverse);

            let message = Message::SetInOut(InOut::Input("".to_string(), 0));
            cap
              .send(self.in_out, message)
              .await
              .into_event()
              .map(UiEvents::from)
          },
          _ => Some(event.into()),
        }
      },
    }
  }

  /// Handle a custom event.
  async fn handle_custom(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    event: Box<dyn Any>,
  ) -> Option<UiEvents<Event>> {
    match event.downcast::<Message>() {
      Ok(e) => self.handle_custom_event(cap, e).await,
      Err(e) => panic!("Received unexpected custom event: {:?}", e),
    }
  }

  /// React to a message.
  async fn react(
    &self,
    mut message: Message,
    cap: &mut dyn MutCap<Event, Message>,
  ) -> Option<Message> {
    match message {
      Message::CollectState => {
        let tab_state = TabState {
          queries: Vec::new(),
          selected: Some(self.selection(cap)),
        };
        let mut message = Message::GetTabState(tab_state);
        let data = self.data::<TabBarData>(cap);

        // We circumvent any issues with tabs being added/removed behind
        // our back (not that this should happen...) by taking a
        // snapshot of the current set -- that mostly works because
        // right now widgets cannot be removed from the UI.
        let tabs = data
          .tabs
          .iter()
          .map(|(_, id)| id)
          .copied()
          .collect::<Vec<_>>();
        for tab in tabs {
          let _ = cap.call(tab, &mut message).await;
        }

        let tab_state = if let Message::GetTabState(tab_state) = message {
          tab_state
        } else {
          unreachable!()
        };

        let message = Message::CollectedState(tab_state);
        Some(message)
      },
      Message::SelectTask(_, ref mut state) => {
        let data = self.data::<TabBarData>(cap);
        let iter = data.tabs.iter().map(|x| x.1);
        let new_idx = state.normalize(iter.clone());

        if !state.has_cycled(iter.len()) {
          let tab = data.tabs[new_idx].1;
          cap.send(tab, message).await
        } else {
          None
        }
      },
      Message::SelectedTask(widget_id) => {
        let data = self.data::<TabBarData>(cap);
        let select = data.tabs.iter().position(|x| x.1 == widget_id).unwrap();
        let update = self.set_select(cap, select as isize);
        MessageExt::maybe_update(None, update)
      },
      m => panic!("Received unexpected message: {:?}", m),
    }
  }
}


#[cfg(test)]
mod tests {
  use super::*;


  #[test]
  fn search_take() {
    let mut search = Search::Unset;
    assert_eq!(search.take(), Search::Unset);
    assert_eq!(search, Search::Taken);

    let mut search = Search::Taken;
    assert_eq!(search.take(), Search::Taken);
    assert_eq!(search, Search::Taken);

    let mut search = Search::State("test".to_string());
    match search.take() {
      Search::State(string) => assert_eq!(string, "test"),
      _ => panic!(),
    }
    assert_eq!(search, Search::Taken);
  }

  #[test]
  fn search_tabs() {
    let tabs = vec![1, 2, 3];
    let selected = 0;
    let reverse = false;
    let snapshot = search_snapshot(tabs.iter().copied(), selected, reverse);
    assert_eq!(snapshot, vec![1, 2, 3, 1]);

    let tabs = vec![1, 2, 3, 4];
    let selected = 1;
    let reverse = false;
    let snapshot = search_snapshot(tabs.iter().copied(), selected, reverse);
    assert_eq!(snapshot, vec![2, 3, 4, 1, 2]);

    let tabs = vec![1, 2, 3];
    let selected = 2;
    let reverse = false;
    let snapshot = search_snapshot(tabs.iter().copied(), selected, reverse);
    assert_eq!(snapshot, vec![3, 1, 2, 3]);

    let tabs = vec![1, 2, 3];
    let selected = 0;
    let reverse = true;
    let snapshot = search_snapshot(tabs.iter().copied(), selected, reverse);
    assert_eq!(snapshot, vec![1, 3, 2, 1]);

    let tabs = vec![1, 2, 3, 4, 5];
    let selected = 1;
    let reverse = true;
    let snapshot = search_snapshot(tabs.iter().copied(), selected, reverse);
    assert_eq!(snapshot, vec![2, 1, 5, 4, 3, 2]);

    let tabs = vec![1, 2, 3];
    let selected = 2;
    let reverse = true;
    let snapshot = search_snapshot(tabs.iter().copied(), selected, reverse);
    assert_eq!(snapshot, vec![3, 2, 1, 3]);
  }
}
