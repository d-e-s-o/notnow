// task_list_box.rs

// *************************************************************************
// * Copyright (C) 2018-2019 Daniel Mueller (deso@posteo.net)              *
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

use cell::RefCell;

use gui::Handleable;
use gui::Id;
use gui::MutCap;
use gui::UiEvent;
use gui::UiEvents;
use gui::derive::Widget;

use crate::query::Query;
use crate::tasks::Id as TaskId;
use crate::tasks::Task;
use crate::tasks::Tasks;

use super::event::Event;
use super::event::EventUpdate;
use super::event::Key;
use super::in_out::InOut;
use super::tab_bar::IterationState;
use super::tab_bar::SearchState;
use super::tab_bar::TabState;
use super::termui::TermUiEvent;


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


/// A widget representing a list of `Task` objects.
#[derive(Debug, Widget)]
#[gui(Event = "Event")]
pub struct TaskListBox {
  id: Id,
  tasks: Rc<RefCell<Tasks>>,
  query: Query,
  selection: isize,
  state: Option<State>,
}

impl TaskListBox {
  /// Create a new `TaskListBox` widget.
  pub fn new(id: Id, tasks: Rc<RefCell<Tasks>>,
             query: Query, selected: Option<usize>) -> Self {
    let count = query.iter().clone().count();
    let selected = selected
      .map(|x| min(x, isize::MAX as usize))
      .unwrap_or(0) as isize;
    let selected = sanitize_selection(selected, count) as isize;

    TaskListBox {
      id: id,
      tasks: tasks,
      query: query,
      selection: selected,
      state: None,
    }
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
  fn handle_select_task(&mut self,
                        task_id: TaskId,
                        mut state: IterationState) -> Option<UiEvents<Event>> {
    let idx = self.query.iter().position(|x| x.id() == task_id);
    if let Some(idx) = idx {
      let update = self.set_select(idx as isize);
      let event = TermUiEvent::SelectedTask(self.id);
      // Indicate to the parent that we selected the task in
      // question successfully. The widget should make sure to focus
      // us subsequently.
      Some(UiEvent::Custom(Box::new(event))).maybe_update(update)
    } else {
      // We don't have a task that should get selected. Bounce the event
      // back to the parent to let it check with the next widget.
      state.advance();

      let data = Box::new(TermUiEvent::SelectTask(task_id, state));
      let event = UiEvent::Custom(data);
      Some(event.into())
    }
  }

  /// Start the selection of a task.
  fn handle_select_task_start(&mut self, task_id: TaskId) -> Option<UiEvents<Event>> {
    let state = IterationState::new(self.id);
    self.handle_select_task(task_id, state)
  }

  /// Search for a task containing the given string.
  fn search_task_index(&self,
                       string: &str,
                       search_state: &mut SearchState,
                       iter_state: &mut IterationState) -> Option<usize> {
    // Note that because we use the count for index calculation
    // purposes, we subtract one below on every use.
    let count = self.query.iter().clone().count();
    // First figure out from where we start the search. If we have
    // visited this `TaskListBox` beforehand we may have already visited
    // the first couple of tasks matching the given string and we should
    // skip those.
    let start_idx = match search_state {
      SearchState::Current => {
        if iter_state.is_reversed() {
          (count - 1) - self.selection()
        } else {
          self.selection()
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
      self
        .query
        .iter()
        .clone()
        .rev()
        .skip(start_idx)
        .position(|x| x.summary.to_ascii_lowercase().contains(string))
        .and_then(|idx| Some((count - 1) - (start_idx + idx)))
    } else {
      self
        .query
        .iter()
        .clone()
        .skip(start_idx)
        .position(|x| x.summary.to_ascii_lowercase().contains(string))
        .and_then(|idx| Some(start_idx + idx))
    }
  }

  /// Handle a `TermUiEvent::SearchTask` event.
  fn handle_search_task(&mut self,
                        string: &str,
                        search_state: &mut SearchState,
                        iter_state: &mut IterationState) -> Option<UiEvents<Event>> {
    debug_assert_eq!(string, &string.to_ascii_lowercase());

    let idx = self.search_task_index(string, search_state, iter_state);
    if let Some(idx) = idx {
      *search_state = SearchState::Task(idx);

      let update = self.set_select(idx as isize);
      let event = TermUiEvent::SelectedTask(self.id);
      Some(UiEvent::Custom(Box::new(event))).maybe_update(update)
    } else {
      iter_state.advance();
      *search_state = SearchState::First;
      None
    }
  }

  /// Handle a custom event.
  fn handle_custom_event(&mut self, event: Box<TermUiEvent>) -> Option<UiEvents<Event>> {
    match *event {
      TermUiEvent::SelectTask(task_id, state) => {
        self.handle_select_task(task_id, state)
      },
      TermUiEvent::EnteredText(ref text) => {
        if let Some(state) = self.state.take() {
          match state {
            State::Add => {
              if !text.is_empty() {
                let tags = if !self.query.is_empty() {
                  let mut task = self.selected_task();
                  if task.is_complete() {
                    task.toggle_complete()
                  }
                  let tags = task.tags().cloned().collect();
                  tags
                } else {
                  Default::default()
                };

                let id = self.tasks.borrow_mut().add(text.clone(), tags);
                self.handle_select_task_start(id)
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
                self.tasks.borrow_mut().update(task);
                self.handle_select_task_start(id).update()
              } else {
                self.tasks.borrow_mut().remove(id);
                (None as Option<Event>).update()
              }
            },
          }
        } else {
          Some(UiEvent::Custom(event).into())
        }
      },
      #[cfg(not(feature = "readline"))]
      TermUiEvent::InputCanceled => {
        if self.state.take().is_some() {
          (None as Option<Event>).update()
        } else {
          Some(UiEvent::Custom(Box::new(TermUiEvent::InputCanceled)).into())
        }
      },
      _ => Some(UiEvent::Custom(event).into()),
    }
  }

  /// Handle a "returnable" custom event.
  fn handle_custom_event_ref(&mut self, event: &mut TermUiEvent) -> Option<UiEvents<Event>> {
    match event {
      TermUiEvent::SearchTask(string, search_state, iter_state) => {
        self.handle_search_task(string, search_state, iter_state)
      },
      TermUiEvent::GetTabState(ref mut tab_state, ref mut iter_state) => {
        let TabState{ref mut queries, ..} = tab_state;
        let selected = Some(self.selection());

        queries.push((self.query(), selected));
        iter_state.advance();
        None
      },
      _ => None,
    }
  }

  /// Retrieve the query associated with this widget.
  pub fn query(&self) -> Query {
    self.query.clone()
  }

  /// Retrieve the selection index with some relative change.
  fn some_selection(&self, add: isize) -> usize {
    let query = self.query();
    let count = query.iter().clone().count();
    let selection = sanitize_selection(self.selection, count);
    debug_assert!(add >= 0 || selection as isize >= add);
    (selection as isize + add) as usize
  }

  /// Retrieve the current selection index.
  ///
  /// The selection index indicates the currently selected task.
  pub fn selection(&self) -> usize {
    self.some_selection(0)
  }

  /// Change the currently selected task.
  fn set_select(&mut self, selection: isize) -> bool {
    let query = self.query();
    let count = query.iter().clone().count();
    let old_selection = sanitize_selection(self.selection, count);
    let new_selection = sanitize_selection(selection, count);

    self.selection = selection;
    new_selection != old_selection
  }

  /// Change the currently selected task.
  fn select(&mut self, change: isize) -> bool {
    // We always make sure to base the given `change` value off of a
    // sanitized selection. Otherwise the result is not as expected.
    let query = self.query();
    let count = query.iter().clone().count();
    let selection = sanitize_selection(self.selection, count);
    let new_selection = selection as isize + change;
    self.set_select(new_selection)
  }

  /// Retrieve a copy of the selected task.
  ///
  /// This method must only be called if tasks are available.
  fn selected_task(&self) -> Task {
    debug_assert!(!self.query().is_empty());

    let selection = self.selection();
    // We maintain the invariant that the selection is always valid,
    // which means that we should always expect a task to be found.
    let query = self.query();
    let task = query.iter().clone().cloned().nth(selection).unwrap();
    task
  }
}

impl Handleable<Event> for TaskListBox {
  /// Check for new input and react to it.
  fn handle(&mut self, event: Event, _cap: &mut dyn MutCap<Event>) -> Option<UiEvents<Event>> {
    match event {
      Event::Key(key) => {
        match key {
          Key::Char(' ') => {
            if !self.query().is_empty() {
              let mut task = self.selected_task();
              let id = task.id();
              task.toggle_complete();
              self.tasks.borrow_mut().update(task);
              self.handle_select_task_start(id).update()
            } else {
              None
            }
          },
          Key::Char('a') => {
            let event = TermUiEvent::SetInOut(InOut::Input("".to_string(), 0));
            let event = UiEvent::Custom(Box::new(event));

            self.state = Some(State::Add);
            Some(event.into())
          },
          Key::Char('d') => {
            if !self.query().is_empty() {
              let id = self.selected_task().id();
              self.tasks.borrow_mut().remove(id);
              (None as Option<Event>).update()
            } else {
              None
            }
          },
          Key::Char('e') => {
            if !self.query().is_empty() {
              let task = self.selected_task();
              let string = task.summary.clone();
              let idx = string.len();
              let event = TermUiEvent::SetInOut(InOut::Input(string, idx));
              let event = UiEvent::Custom(Box::new(event));

              self.state = Some(State::Edit(task));
              Some(event.into())
            } else {
              None
            }
          },
          Key::Char('J') => {
            if !self.query().is_empty() {
              let to_move = self.selected_task();
              let query = self.query();
              let other = query.iter().nth(self.some_selection(1));
              if let Some(other) = other {
                self.tasks.borrow_mut().move_after(to_move.id(), other.id());
                (None as Option<Event>).maybe_update(self.select(1))
              } else {
                None
              }
            } else {
              None
            }
          },
          Key::Char('K') => {
            if !self.query().is_empty() && self.selection() > 0 {
              let to_move = self.selected_task();
              let query = self.query();
              let other = query.iter().nth(self.some_selection(-1));
              if let Some(other) = other {
                self.tasks.borrow_mut().move_before(to_move.id(), other.id());
                (None as Option<Event>).maybe_update(self.select(-1))
              } else {
                None
              }
            } else {
              None
            }
          },
          Key::Char('g') => (None as Option<Event>).maybe_update(self.set_select(0)),
          Key::Char('G') => (None as Option<Event>).maybe_update(self.set_select(isize::MAX)),
          Key::Char('j') => (None as Option<Event>).maybe_update(self.select(1)),
          Key::Char('k') => (None as Option<Event>).maybe_update(self.select(-1)),
          _ => Some(event.into()),
        }
      },
    }
  }

  /// Handle a custom event.
  fn handle_custom(&mut self,
                   event: Box<dyn Any>,
                   _cap: &mut dyn MutCap<Event>) -> Option<UiEvents<Event>> {
    match event.downcast::<TermUiEvent>() {
      Ok(e) => self.handle_custom_event(e),
      Err(e) => panic!("Received unexpected custom event: {:?}", e),
    }
  }

  /// Handle a custom event.
  fn handle_custom_ref(&mut self,
                       event: &mut dyn Any,
                       _cap: &mut dyn MutCap<Event>) -> Option<UiEvents<Event>> {
    match event.downcast_mut::<TermUiEvent>() {
      Some(e) => self.handle_custom_event_ref(e),
      None => panic!("Received unexpected custom event"),
    }
  }
}
