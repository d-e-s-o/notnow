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

use std::any::Any;
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
use gui::UiEvent;
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
use super::tab_bar::IterationState;
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
}

impl TaskListBox {
  /// Create a new `TaskListBox` widget.
  pub fn new(id: Id, cap: &mut dyn MutCap<Event, Message>, selected: Option<usize>) -> Self {
    let task_list_box = Self { id };
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
  /// added or modified. After a such an operation task may or may not
  /// be covered by our query anymore, meaning we either need to select
  /// it or find somebody else who displays it and ask him to select it.
  ///
  /// Note that the emitted event chain will only contain an `Updated`
  /// event if the selection changed. However, there may be other
  /// conditions under which an update must happen, e.g., when the
  /// summary or the tags of the task changed. Handling of updates in
  /// those cases is left to clients.
  fn handle_select_task(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    task_id: TaskId,
    mut state: IterationState,
  ) -> Option<UiEvents<Event>> {
    let data = self.data_mut::<TaskListBoxData>(cap);
    let idx = data.query.iter().position(|x| x.id() == task_id);

    if let Some(idx) = idx {
      let update = data.set_select(idx as isize);
      let event = Message::SelectedTask(self.id);
      // Indicate to the parent that we selected the task in
      // question successfully. The widget should make sure to focus
      // us subsequently.
      Some(UiEvent::Custom(Box::new(event))).maybe_update(update)
    } else {
      // We don't have a task that should get selected. Bounce the event
      // back to the parent to let it check with the next widget.
      state.advance();

      let data = Box::new(Message::SelectTask(task_id, state));
      let event = UiEvent::Custom(data);
      Some(event.into())
    }
  }

  /// Start the selection of a task.
  fn handle_select_task_start(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    task_id: TaskId,
  ) -> Option<UiEvents<Event>> {
    let state = IterationState::new(self.id);
    self.handle_select_task(cap, task_id, state)
  }

  /// Search for a task containing the given string.
  fn search_task_index(
    &self,
    cap: &dyn Cap,
    string: &str,
    search_state: &mut SearchState,
    iter_state: &mut IterationState,
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
      SearchState::Current => {
        if iter_state.is_reversed() {
          (count - 1) - data.selection(0)
        } else {
          data.selection(0)
        }
      },
      SearchState::First => 0,
      SearchState::Task(idx) => {
        if iter_state.is_reversed() {
          (count - 1) - *idx + 1
        } else {
          *idx + 1
        }
      },
    };

    // Note that a simpler version of this find magic would just use
    // the `enumerate` functionality. However, for some reason that
    // would require us to work with an `ExactSizeIterator`, which is
    // not something that we can provide.
    if iter_state.is_reversed() {
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
  fn handle_search_task(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    string: &str,
    search_state: &mut SearchState,
    iter_state: &mut IterationState,
  ) -> Option<UiEvents<Event>> {
    debug_assert_eq!(string, &string.to_ascii_lowercase());

    let idx = self.search_task_index(cap, string, search_state, iter_state);
    if let Some(idx) = idx {
      *search_state = SearchState::Task(idx);

      let data = self.data_mut::<TaskListBoxData>(cap);
      let update = data.set_select(idx as isize);
      let event = Message::SelectedTask(self.id);
      Some(UiEvent::Custom(Box::new(event))).maybe_update(update)
    } else {
      iter_state.advance();
      *search_state = SearchState::First;
      None
    }
  }

  /// Handle a custom event.
  fn handle_custom_event(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    event: Box<Message>,
  ) -> Option<UiEvents<Event>> {
    let data = self.data_mut::<TaskListBoxData>(cap);
    match *event {
      Message::SelectTask(task_id, state) => {
        self.handle_select_task(cap, task_id, state)
      },
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
                self.handle_select_task_start(cap, id)
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
                self.handle_select_task_start(cap, id).update()
              } else {
                data.tasks.borrow_mut().remove(id);
                (None as Option<Event>).update()
              }
            },
          }
        } else {
          Some(UiEvent::Custom(event).into())
        }
      },
      #[cfg(not(feature = "readline"))]
      Message::InputCanceled => {
        if data.state.take().is_some() {
          (None as Option<Event>).update()
        } else {
          Some(UiEvent::Custom(Box::new(Message::InputCanceled)).into())
        }
      },
      _ => Some(UiEvent::Custom(event).into()),
    }
  }

  /// Handle a "returnable" custom event.
  fn handle_custom_event_ref(
    &self,
    event: &mut Message,
    cap: &mut dyn MutCap<Event, Message>,
  ) -> Option<UiEvents<Event>> {
    match event {
      Message::SearchTask(string, search_state, iter_state) => {
        self.handle_search_task(cap, string, search_state, iter_state)
      },
      _ => None,
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
              self.handle_select_task_start(cap, id).update()
            } else {
              None
            }
          },
          Key::Char('a') => {
            let event = Message::SetInOut(InOut::Input("".to_string(), 0));
            let event = UiEvent::Custom(Box::new(event));
            data.state = Some(State::Add);
            Some(event.into())
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
              let event = Message::SetInOut(InOut::Input(string, idx));
              let event = UiEvent::Custom(Box::new(event));
              data.state = Some(State::Edit(task));
              Some(event.into())
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

  /// Handle a custom event.
  async fn handle_custom(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    event: Box<dyn Any>,
  ) -> Option<UiEvents<Event>> {
    match event.downcast::<Message>() {
      Ok(e) => self.handle_custom_event(cap, e),
      Err(e) => panic!("Received unexpected custom event: {:?}", e),
    }
  }

  /// Handle a custom event.
  async fn handle_custom_ref(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    event: &mut dyn Any,
  ) -> Option<UiEvents<Event>> {
    match event.downcast_mut::<Message>() {
      Some(e) => self.handle_custom_event_ref(e, cap),
      None => panic!("Received unexpected custom event"),
    }
  }

  /// Respond to a message.
  async fn respond(
    &self,
    message: &mut Message,
    cap: &mut dyn MutCap<Event, Message>,
  ) -> Option<Message> {
    match message {
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
