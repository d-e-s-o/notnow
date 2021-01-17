// task_list_box.rs

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

use std::cmp::max;
use std::cmp::min;
use std::isize;
use std::rc::Rc;

use async_trait::async_trait;

use cell::RefCell;

use gui::Cap;
use gui::Handleable;
use gui::Id;
use gui::MutCap;
use gui::UiEvents;
use gui::Widget;
use gui::derive::Widget;

use crate::query::Query;
use crate::tasks::Id as TaskId;
use crate::tasks::Task;
use crate::tasks::Tasks;

use super::event::Event;
use super::event::EventUpdate;
use super::event::Key;
use super::in_out::InOut;
use super::message::Message;
use super::message::MessageExt;
use super::tab_bar::SearchState;
use super::tab_bar::TabState;


/// Sanitize a selection index.
fn sanitize_selection(selection: isize, count: usize) -> usize {
  if count == 0 {
    0
  } else {
    max(0, min(count as isize - 1, selection)) as usize
  }
}


/// An enum representing the state a `TaskListBox` can be in.
#[derive(Debug)]
enum State {
  Add,
  Edit(Task),
}


/// The data associated with a `TaskListBox`.
pub struct TaskListBoxData {
  /// The tasks database.
  tasks: Rc<RefCell<Tasks>>,
  /// The query represented by the `TaskListBox`.
  query: Query,
  /// The currently selected task.
  selection: isize,
  /// The state the `TaskListBox` is in.
  state: Option<State>,
}

impl TaskListBoxData {
  /// Create a new `TaskListBoxData` object.
  pub fn new(tasks: Rc<RefCell<Tasks>>, query: Query) -> Self {
    Self {
      tasks,
      query,
      selection: 0,
      state: None,
    }
  }

  /// Retrieve the selection index with some relative change.
  fn selection(&self, add: isize) -> usize {
    let count = self.query.iter().clone().count();
    let selection = sanitize_selection(self.selection, count);
    debug_assert!(add >= 0 || selection as isize >= add);
    (selection as isize + add) as usize
  }

  /// Change the currently selected task.
  fn set_select(&mut self, selection: isize) -> bool {
    let count = self.query.iter().clone().count();
    let old_selection = sanitize_selection(self.selection, count);
    let new_selection = sanitize_selection(selection, count);

    self.selection = selection;
    new_selection != old_selection
  }

  /// Change the currently selected task in a relative fashion.
  fn select(&mut self, change: isize) -> bool {
    // We always make sure to base the given `change` value off of a
    // sanitized selection. Otherwise the result is not as expected.
    let count = self.query.iter().clone().count();
    let selection = sanitize_selection(self.selection, count);
    let new_selection = selection as isize + change;
    self.set_select(new_selection)
  }

  /// Retrieve a copy of the selected task.
  ///
  /// This method must only be called if tasks are available.
  fn selected_task(&self) -> Task {
    debug_assert!(!self.query.is_empty());

    let selection = self.selection(0);
    // We maintain the invariant that the selection is always valid,
    // which means that we should always expect a task to be found.
    let task = self.query.iter().clone().cloned().nth(selection).unwrap();
    task
  }
}


/// A widget representing a list of `Task` objects.
#[derive(Debug, Widget)]
#[gui(Event = Event, Message = Message)]
pub struct TaskListBox {
  id: Id,
  in_out: Id,
  tab_bar: Id,
}

impl TaskListBox {
  /// Create a new `TaskListBox` widget.
  pub fn new(
    id: Id,
    cap: &mut dyn MutCap<Event, Message>,
    tab_bar: Id,
    in_out: Id,
    selected: Option<usize>,
  ) -> Self {
    let task_list_box = Self {
      id,
      tab_bar,
      in_out,
    };
    let data = task_list_box.data_mut::<TaskListBoxData>(cap);

    let count = data.query.iter().clone().count();
    let selected = selected
      .map(|x| min(x, isize::MAX as usize))
      .unwrap_or(0) as isize;
    let selected = sanitize_selection(selected, count) as isize;
    data.selection = selected;

    task_list_box
  }

  /// Select a task and emit an event indicating success/failure.
  ///
  /// This method takes care of correctly selecting a task after it was
  /// added or modified. After such an operation a task may or may not
  /// be covered by our query anymore, meaning we either need to select
  /// it or find somebody else who displays it and ask him to select it.
  async fn handle_select_task(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    task_id: TaskId,
    done: Option<&mut bool>,
  ) -> Option<Message> {
    let data = self.data_mut::<TaskListBoxData>(cap);
    let idx = data.query.iter().position(|x| x.id() == task_id);

    if let Some(idx) = idx {
      let update = data.set_select(idx as isize);
      if let Some(done) = done {
        *done = true
      }
      MessageExt::maybe_update(None, update)
    } else {
      // If there is no `done` we were called directly from within the
      // widget and not in response to a message from the TabBar. In
      // that case, given that we have not found the task in our own
      // query, reach out to the TabBar.
      if done.is_none() {
        let message = Message::SelectTask(task_id, false);
        cap.send(self.tab_bar, message).await
      } else {
        None
      }
    }
  }

  /// Start the selection of a task.
  async fn select_task(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    task_id: TaskId,
  ) -> Option<Message> {
    self.handle_select_task(cap, task_id, None).await
  }

  /// Search for a task containing the given string.
  fn search_task_index(
    &self,
    cap: &dyn Cap,
    string: &str,
    search_state: &SearchState,
    reverse: bool,
  ) -> Option<usize> {
    let data = self.data::<TaskListBoxData>(cap);
    // Note that because we use the count for index calculation
    // purposes, we subtract one below on every use.
    let count = data.query.iter().clone().count();
    // First figure out from where we start the search. If we have
    // visited this `TaskListBox` beforehand we may have already visited
    // the first couple of tasks matching the given string and we should
    // skip those.
    let start_idx = match search_state {
      SearchState::Current | SearchState::AfterCurrent => {
        let offset = if let SearchState::AfterCurrent = search_state {
          1
        } else {
          0
        };

        if reverse {
          count.saturating_sub(1).saturating_sub(data.selection(-offset))
        } else {
          data.selection(offset)
        }
      },
      SearchState::First => 0,
      SearchState::Done => unreachable!(),
    };

    // Note that a simpler version of this find magic would just use
    // the `enumerate` functionality. However, for some reason that
    // would require us to work with an `ExactSizeIterator`, which is
    // not something that we can provide.
    if reverse {
      data
        .query
        .iter()
        .clone()
        .rev()
        .skip(start_idx)
        .position(|x| x.summary.to_ascii_lowercase().contains(string))
        .map(|idx| (count - 1) - (start_idx + idx))
    } else {
      data
        .query
        .iter()
        .clone()
        .skip(start_idx)
        .position(|x| x.summary.to_ascii_lowercase().contains(string))
        .map(|idx| start_idx + idx)
    }
  }

  /// Handle a `Message::SearchTask` event.
  async fn handle_search_task(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    string: &str,
    search_state: &mut SearchState,
    reverse: bool,
  ) -> Option<Message> {
    debug_assert_eq!(string, &string.to_ascii_lowercase());

    let idx = self.search_task_index(cap, string, search_state, reverse);
    if let Some(idx) = idx {
      *search_state = SearchState::Done;

      let data = self.data_mut::<TaskListBoxData>(cap);
      let update = data.set_select(idx as isize);
      MessageExt::maybe_update(None, update)
    } else {
      None
    }
  }

  /// Retrieve the query associated with this widget.
  pub fn query(&self, cap: &dyn Cap) -> Query {
    let data = self.data::<TaskListBoxData>(cap);
    data.query.clone()
  }

  /// Retrieve the current selection index.
  ///
  /// The selection index indicates the currently selected task.
  pub fn selection(&self, cap: &dyn Cap) -> usize {
    let data = self.data::<TaskListBoxData>(cap);
    data.selection(0)
  }
}

#[async_trait(?Send)]
impl Handleable<Event, Message> for TaskListBox {
  /// Check for new input and react to it.
  async fn handle(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    event: Event,
  ) -> Option<UiEvents<Event>> {
    let data = self.data_mut::<TaskListBoxData>(cap);
    match event {
      Event::Key(key, _) => {
        match key {
          Key::Char(' ') => {
            if !data.query.is_empty() {
              let mut task = data.selected_task();
              let id = task.id();
              task.toggle_complete();
              data.tasks.borrow_mut().update(task);
              self
                .select_task(cap, id)
                .await
                .into_event()
                .map(UiEvents::from)
                .update()
            } else {
              None
            }
          },
          Key::Char('a') => {
            data.state = Some(State::Add);
            let message = Message::SetInOut(InOut::Input("".to_string(), 0));
            cap.send(self.in_out, message).await.into_event().map(UiEvents::from)
          },
          Key::Char('d') => {
            if !data.query.is_empty() {
              let id = data.selected_task().id();
              data.tasks.borrow_mut().remove(id);
              (None as Option<Event>).update()
            } else {
              None
            }
          },
          Key::Char('e') => {
            if !data.query.is_empty() {
              let task = data.selected_task();
              let string = task.summary.clone();
              let idx = string.len();
              data.state = Some(State::Edit(task));

              let message = Message::SetInOut(InOut::Input(string, idx));
              cap.send(self.in_out, message).await.into_event().map(UiEvents::from)
            } else {
              None
            }
          },
          Key::Char('J') => {
            if !data.query.is_empty() {
              let to_move = data.selected_task();
              let other = data.query.iter().nth(data.selection(1));
              if let Some(other) = other {
                data.tasks.borrow_mut().move_after(to_move.id(), other.id());
                (None as Option<Event>).maybe_update(data.select(1))
              } else {
                None
              }
            } else {
              None
            }
          },
          Key::Char('K') => {
            if !data.query.is_empty() && data.selection(0) > 0 {
              let to_move = data.selected_task();
              let other = data.query.iter().nth(data.selection(-1));
              if let Some(other) = other {
                data.tasks.borrow_mut().move_before(to_move.id(), other.id());
                (None as Option<Event>).maybe_update(data.select(-1))
              } else {
                None
              }
            } else {
              None
            }
          },
          Key::Char('g') => (None as Option<Event>).maybe_update(data.set_select(0)),
          Key::Char('G') => (None as Option<Event>).maybe_update(data.set_select(isize::MAX)),
          Key::Char('j') => (None as Option<Event>).maybe_update(data.select(1)),
          Key::Char('k') => (None as Option<Event>).maybe_update(data.select(-1)),
          _ => Some(event.into()),
        }
      },
    }
  }

  /// React to a message.
  async fn react(&self, message: Message, cap: &mut dyn MutCap<Event, Message>) -> Option<Message> {
    let data = self.data_mut::<TaskListBoxData>(cap);
    match message {
      Message::EnteredText(ref text) => {
        if let Some(state) = data.state.take() {
          match state {
            State::Add => {
              if !text.is_empty() {
                let tags = if !data.query.is_empty() {
                  let mut task = data.selected_task();
                  if task.is_complete() {
                    task.toggle_complete()
                  }
                  let tags = task.tags().cloned().collect();
                  tags
                } else {
                  Default::default()
                };

                let id = data.tasks.borrow_mut().add(text.clone(), tags);
                self.select_task(cap, id).await
              } else {
                None
              }
            },
            State::Edit(mut task) => {
              let id = task.id();

              // Editing a task to empty just removes the task
              // altogether.
              if !text.is_empty() {
                task.summary = text.clone();
                data.tasks.borrow_mut().update(task);
                self.select_task(cap, id).await.maybe_update(true)
              } else {
                data.tasks.borrow_mut().remove(id);
                Some(Message::Updated)
              }
            },
          }
        } else {
          cap.send(self.tab_bar, message).await
        }
      },
      #[cfg(not(feature = "readline"))]
      Message::InputCanceled => {
        if data.state.take().is_some() {
          Some(Message::Updated)
        } else {
          None
        }
      },
      #[cfg(feature = "readline")]
      Message::InputCanceled => None,
      m => panic!("Received unexpected message: {:?}", m),
    }
  }

  /// Respond to a message.
  async fn respond(
    &self,
    message: &mut Message,
    cap: &mut dyn MutCap<Event, Message>,
  ) -> Option<Message> {
    match message {
      Message::SelectTask(task_id, done) => {
        self.handle_select_task(cap, *task_id, Some(done)).await
      },
      Message::SearchTask(string, search_state, reverse) => {
        self
          .handle_search_task(cap, string, search_state, *reverse)
          .await
      },
      Message::GetTabState(ref mut tab_state) => {
        let TabState {
          ref mut queries, ..
        } = tab_state;
        let data = self.data::<TaskListBoxData>(cap);
        let selected = Some(data.selection(0));

        queries.push((self.query(cap), selected));
        None
      },
      m => panic!("Received unexpected message: {:?}", m),
    }
  }
}
