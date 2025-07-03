// Copyright (C) 2018-2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::max;
use std::cmp::min;
use std::mem::replace;
use std::rc::Rc;

use async_trait::async_trait;

use gui::derive::Widget;
use gui::Cap;
use gui::Handleable;
use gui::Id;
use gui::MutCap;
use gui::Widget;

use crate::tags::Tag;
use crate::tasks::Task;
use crate::tasks::Tasks;
use crate::view::View;

use super::event::Event;
use super::event::Key;
use super::in_out::InOut;
use super::in_out::Input;
use super::input::InputText;
use super::message::Message;
use super::message::MessageExt;
use super::task_list_box::TaskListBox;
use super::task_list_box::TaskListBoxData;


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
  /// The text to search for. The `bool` determines whether the search
  /// is for an exact match or not.
  State(String, bool),
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
#[derive(Debug, PartialEq, Eq)]
pub enum SearchState {
  /// Start the search at the currently selected task.
  Current,
  /// Start right after the currently selected task.
  AfterCurrent,
  /// Start the search at the first task in the view being displayed.
  First,
  /// The search is done.
  Done,
}


/// The state used for collecting the views from the various
/// `TaskListBox` objects.
#[derive(Debug)]
pub struct TabState {
  /// The accumulation of views gathered from the individual
  /// `TaskListBox` objects.
  pub views: Vec<(View, Option<usize>)>,
  /// The currently selected view.
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
  /// A copied task. Used for copy & paste operations.
  copied_task: Option<Task>,
}

impl TabBarData {
  /// Create a new `TabBarData` object.
  pub fn new() -> Self {
    Self {
      tabs: Default::default(),
      selection: 0,
      prev_selection: 0,
      search: Search::Unset,
      copied_task: None,
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
  pub fn search_snapshot(&self, reverse: bool) -> Vec<(usize, Id)> {
    let tabs = self.tabs.iter().map(|(_, id)| id).copied().enumerate();
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
  #[expect(clippy::too_many_arguments)]
  pub fn new(
    id: Id,
    cap: &mut dyn MutCap<Event, Message>,
    detail_dialog: Id,
    tag_dialog: Id,
    in_out: Id,
    tasks: Rc<Tasks>,
    views: Vec<(View, Option<usize>)>,
    toggle_tag: Option<Tag>,
    selected: Option<usize>,
  ) -> Self {
    let count = views.len();
    let selected = selected.map(|x| min(x, isize::MAX as usize)).unwrap_or(0) as isize;
    let selected = sanitize_selection(selected, count);
    let tab_bar = id;

    let tabs = views
      .into_iter()
      .enumerate()
      .map(|(i, (view, task))| {
        let name = view.name().to_string();
        let tasks = Rc::clone(&tasks);
        let toggle_tag = toggle_tag.clone();
        let task_list = cap.add_widget(
          id,
          Box::new(|| Box::new(TaskListBoxData::new(tasks, view, toggle_tag))),
          Box::new(move |id, cap| {
            Box::new(TaskListBox::new(
              id,
              cap,
              tab_bar,
              detail_dialog,
              tag_dialog,
              in_out,
              task,
            ))
          }),
        );

        if i == selected {
          cap.focus(task_list);
        } else {
          cap.hide(task_list);
        }
        (name, task_list)
      })
      .collect();

    let tab_bar = Self { id, in_out };
    let data = tab_bar.data_mut::<TabBarData>(cap);
    data.tabs = tabs;
    data.selection = selected as isize;
    data.prev_selection = selected as isize;

    tab_bar
  }

  /// Initiate the search of a task based on a string.
  async fn start_task_search(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    string: String,
    reverse: bool,
    exact: bool,
  ) -> Option<Message> {
    let data = self.data::<TabBarData>(cap);
    if !string.is_empty() && !data.tabs.is_empty() {
      let message = Message::SetInOut(InOut::Search(string.clone()));
      let result1 = cap.send(self.in_out, message).await;

      let search_state = SearchState::Current;
      let result2 = self
        .search_task(cap, string, search_state, reverse, exact)
        .await;

      result1.maybe_update(result2)
    } else {
      None
    }
  }

  /// Continue the search of a task based on a string.
  async fn continue_task_search(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    string: String,
    reverse: bool,
    exact: bool,
  ) -> Option<Message> {
    let search_state = SearchState::AfterCurrent;
    self
      .search_task(cap, string, search_state, reverse, exact)
      .await
  }

  /// Handle a `Message::SearchTask` event.
  async fn search_task(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    string: String,
    search_state: SearchState,
    reverse: bool,
    exact: bool,
  ) -> Option<Message> {
    let mut result = None;
    let data = self.data_mut::<TabBarData>(cap);
    let tabs = data.search_snapshot(reverse);
    let mut message = Message::SearchTask(string.clone(), search_state, reverse, exact);
    let mut found = false;

    for (idx, tab) in tabs {
      let result1 = cap.call(tab, &mut message).await;
      result = result.maybe_update(result1);

      if let Message::SearchTask(_, search_state, _, _) = &mut message {
        if let SearchState::Done = search_state {
          let update = self.set_select(cap, idx as isize);
          result = result.maybe_update(update.then(|| Message::updated(self.id)));
          found = true;
          break
        }

        // After the first call (which started on the currently selected
        // task on the currently selected tab) we always continue with the
        // first task on any subsequent tab.
        *search_state = SearchState::First;
      } else {
        panic!("Received unexpected message: {message:?}")
      }
    }

    if !found {
      let error = format!("Text '{string}' not found");
      let message = Message::SetInOut(InOut::Error(error));
      let result1 = cap.send(self.in_out, message).await;
      result = result.maybe_update(result1);
    }

    let data = self.data_mut::<TabBarData>(cap);
    data.search = Search::State(string, exact);
    result
  }

  /// Retrieve an iterator over the names of all the tabs.
  pub fn iter<'slf>(&'slf self, cap: &'slf dyn Cap) -> impl ExactSizeIterator<Item = &'slf String> {
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
  async fn handle(&self, cap: &mut dyn MutCap<Event, Message>, event: Event) -> Option<Event> {
    let data = self.data_mut::<TabBarData>(cap);
    match event {
      Event::Key((key, _)) => match key {
        Key::Char('1') => self.set_select(cap, 0).then(|| Event::updated(self.id)),
        Key::Char('2') => self.set_select(cap, 1).then(|| Event::updated(self.id)),
        Key::Char('3') => self.set_select(cap, 2).then(|| Event::updated(self.id)),
        Key::Char('4') => self.set_select(cap, 3).then(|| Event::updated(self.id)),
        Key::Char('5') => self.set_select(cap, 4).then(|| Event::updated(self.id)),
        Key::Char('6') => self.set_select(cap, 5).then(|| Event::updated(self.id)),
        Key::Char('7') => self.set_select(cap, 6).then(|| Event::updated(self.id)),
        Key::Char('8') => self.set_select(cap, 7).then(|| Event::updated(self.id)),
        Key::Char('9') => self.set_select(cap, 8).then(|| Event::updated(self.id)),
        Key::Char('0') => self
          .set_select(cap, isize::MAX)
          .then(|| Event::updated(self.id)),
        Key::Char('`') => self.select_previous(cap).then(|| Event::updated(self.id)),
        Key::Char('h') => self.select(cap, -1).then(|| Event::updated(self.id)),
        Key::Char('l') => self.select(cap, 1).then(|| Event::updated(self.id)),
        Key::Char('H') => self.swap(cap, true).then(|| Event::updated(self.id)),
        Key::Char('L') => self.swap(cap, false).then(|| Event::updated(self.id)),
        Key::Char('n') | Key::Char('N') => {
          let event = match data.search.take() {
            Search::Preparing(..) | Search::Unset => {
              data.search = Search::Unset;

              let error = InOut::Error("Nothing to search for".to_string());
              let message = Message::SetInOut(error);
              cap.send(self.in_out, message).await.into_event()
            },
            Search::Taken => panic!("invalid search state"),
            Search::State(string, exact) => {
              let reverse = key == Key::Char('N');
              let message = Message::SetInOut(InOut::Search(string.clone()));
              let result1 = cap.send(self.in_out, message).await;
              let result2 = self.continue_task_search(cap, string, reverse, exact).await;

              result1.maybe_update(result2).into_event()
            },
          };
          event
        },
        Key::Char('/') | Key::Char('?') => {
          let reverse = key == Key::Char('?');
          data.search = Search::Preparing(reverse);

          let input = Input {
            text: InputText::default(),
            response_id: self.id,
          };
          let message = Message::SetInOut(InOut::Input(input));
          cap.send(self.in_out, message).await.into_event()
        },
        _ => Some(event),
      },
      _ => Some(event),
    }
  }

  /// React to a message.
  async fn react(&self, message: Message, cap: &mut dyn MutCap<Event, Message>) -> Option<Message> {
    match message {
      Message::CollectState => {
        let tab_state = TabState {
          views: Vec::new(),
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
          cap.call(tab, &mut message).await;
        }

        let tab_state = if let Message::GetTabState(tab_state) = message {
          tab_state
        } else {
          unreachable!()
        };

        let message = Message::CollectedState(tab_state);
        Some(message)
      },
      Message::SelectTask(task_id, ..) => {
        let data = self.data::<TabBarData>(cap);
        let mut message = Message::SelectTask(task_id, false);
        let mut result = None;

        let tabs = data
          .tabs
          .iter()
          .map(|(_, id)| id)
          .copied()
          .enumerate()
          .collect::<Vec<_>>();

        for (idx, tab) in tabs {
          cap.call(tab, &mut message).await;
          if let Message::SelectTask(_, done) = message {
            if done {
              let update = self.set_select(cap, idx as isize);
              result = result.maybe_update(update.then(|| Message::updated(self.id)));
              break
            }
          } else {
            panic!("Received unexpected message: {message:?}")
          }
        }
        result
      },
      Message::StartTaskSearch(ref string) | Message::EnteredText(ref string) => {
        let entered = matches!(message, Message::EnteredText(_));
        let data = self.data_mut::<TabBarData>(cap);
        let (string, reverse, exact) = if entered {
          (
            string.to_lowercase(),
            data.search.take().is_reverse(),
            false,
          )
        } else {
          (string.clone(), false, true)
        };

        let result1 = self
          .start_task_search(cap, string.clone(), reverse, exact)
          .await;
        let result2 = if !entered {
          self.continue_task_search(cap, string, reverse, exact).await
        } else {
          None
        };

        result1.maybe_update(result2)
      },
      Message::InputCanceled => {
        let data = self.data_mut::<TabBarData>(cap);
        data.search = Search::Unset;
        None
      },
      Message::CopyTask(copied) => {
        let data = self.data_mut::<TabBarData>(cap);
        data.copied_task = Some(copied);
        None
      },
      message => panic!("Received unexpected message: {message:?}"),
    }
  }

  /// Respond to a message.
  async fn respond(
    &self,
    message: &mut Message,
    cap: &mut dyn MutCap<Event, Message>,
  ) -> Option<Message> {
    match message {
      Message::GetCopiedTask(ref mut task) => {
        let data = self.data::<TabBarData>(cap);
        task.clone_from(&data.copied_task);
        None
      },
      message => panic!("Received unexpected message: {message:?}"),
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

    let mut search = Search::State("test".to_string(), false);
    match search.take() {
      Search::State(string, exact) => {
        assert_eq!(string, "test");
        assert!(!exact);
      },
      _ => panic!(),
    }
    assert_eq!(search, Search::Taken);
  }

  #[test]
  fn search_tabs() {
    let tabs = [1, 2, 3];
    let selected = 0;
    let reverse = false;
    let snapshot = search_snapshot(tabs.iter().copied(), selected, reverse);
    assert_eq!(snapshot, vec![1, 2, 3, 1]);

    let tabs = [1, 2, 3, 4];
    let selected = 1;
    let reverse = false;
    let snapshot = search_snapshot(tabs.iter().copied(), selected, reverse);
    assert_eq!(snapshot, vec![2, 3, 4, 1, 2]);

    let tabs = [1, 2, 3];
    let selected = 2;
    let reverse = false;
    let snapshot = search_snapshot(tabs.iter().copied(), selected, reverse);
    assert_eq!(snapshot, vec![3, 1, 2, 3]);

    let tabs = [1, 2, 3];
    let selected = 0;
    let reverse = true;
    let snapshot = search_snapshot(tabs.iter().copied(), selected, reverse);
    assert_eq!(snapshot, vec![1, 3, 2, 1]);

    let tabs = [1, 2, 3, 4, 5];
    let selected = 1;
    let reverse = true;
    let snapshot = search_snapshot(tabs.iter().copied(), selected, reverse);
    assert_eq!(snapshot, vec![2, 1, 5, 4, 3, 2]);

    let tabs = [1, 2, 3];
    let selected = 2;
    let reverse = true;
    let snapshot = search_snapshot(tabs.iter().copied(), selected, reverse);
    assert_eq!(snapshot, vec![3, 2, 1, 3]);
  }
}
