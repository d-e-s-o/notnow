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

use std::any::Any;
use std::cmp::max;
use std::cmp::min;
use std::isize;
use std::rc::Rc;

use cell::RefCell;

use gui::Cap;
use gui::Event;
use gui::Handleable;
use gui::Id;
use gui::Key;
use gui::UiEvent;
use gui::UiEvents;

use event::EventUpdated;
use in_out::InOut;
use query::Query;
use tab_bar::SearchState;
use tab_bar::SelectionState;
use tasks::Id as TaskId;
use tasks::Task;
use tasks::Tasks;
use termui::TermUiEvent;


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
#[derive(Debug, GuiWidget)]
pub struct TaskListBox {
  id: Id,
  tasks: Rc<RefCell<Tasks>>,
  query: Query,
  selection: isize,
  state: Option<State>,
}

impl TaskListBox {
  /// Create a new `TaskListBox` widget.
  pub fn new(id: Id, tasks: Rc<RefCell<Tasks>>, query: Query) -> Self {
    TaskListBox {
      id: id,
      tasks: tasks,
      query: query,
      selection: 0,
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
  fn handle_select_task(&mut self, task_id: TaskId, mut state: SelectionState) -> Option<UiEvents> {
    let idx = self.query.position(|x| x.id() == task_id);
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
  fn handle_select_task_start(&mut self, task_id: TaskId) -> Option<UiEvents> {
    let state = SelectionState::new(self.id);
    self.handle_select_task(task_id, state)
  }

  /// Handle a `TermUiEvent::SearchTask` event.
  fn handle_search_task(&mut self,
                        string: &str,
                        search_state: &mut SearchState,
                        selection_state: &mut SelectionState) -> Option<UiEvents> {
    debug_assert_eq!(string, &string.to_ascii_lowercase());

    // First figure out from where we start the search. If we have
    // visited this `TaskListBox` beforehand we may have already visited
    // the first couple of tasks matching the given string and we should
    // skip those.
    let start_idx = match search_state {
      SearchState::Current => self.selection(),
      SearchState::First => 0,
      SearchState::Task(idx) => *idx + 1,
    };

    let idx = self.query.position_from(start_idx, |x| {
      x.summary.to_ascii_lowercase().contains(string)
    });

    if let Some(idx) = idx {
      *search_state = SearchState::Task(idx);

      let update = self.set_select(idx as isize);
      let event = TermUiEvent::SelectedTask(self.id);
      Some(UiEvent::Custom(Box::new(event))).maybe_update(update)
    } else {
      selection_state.advance();
      *search_state = SearchState::First;
      None
    }
  }

  /// Handle a custom event.
  fn handle_custom_event(&mut self, event: Box<TermUiEvent>) -> Option<UiEvents> {
    match { *event } {
      TermUiEvent::SelectTask(task_id, state) => {
        self.handle_select_task(task_id, state)
      },
      TermUiEvent::EnteredText(text) => {
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

                let id = self.tasks.borrow_mut().add(text, tags);
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
                task.summary = text;
                self.tasks.borrow_mut().update(task);
                self.handle_select_task_start(id).update()
              } else {
                self.tasks.borrow_mut().remove(id);
                (None as Option<Event>).update()
              }
            },
          }
        } else {
          // We did not handle the event. Let the parent deal with it.
          // TODO: With non-lexical lifetimes we should be able to just
          //       reuse `event` instead of creating a new box.
          Some(UiEvent::Custom(Box::new(TermUiEvent::EnteredText(text))).into())
        }
      },
      TermUiEvent::InputCanceled => {
        if self.state.take().is_some() {
          (None as Option<Event>).update()
        } else {
          Some(UiEvent::Custom(Box::new(TermUiEvent::InputCanceled)).into())
        }
      },
      event => Some(UiEvent::Custom(Box::new(event)).into()),
    }
  }

  /// Handle a "returnable" custom event.
  fn handle_custom_event_ref(&mut self, event: &mut TermUiEvent) -> Option<UiEvents> {
    match event {
      TermUiEvent::SearchTask(string, search_state, selection_state) => {
        self.handle_search_task(string, search_state, selection_state)
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
    let count = self.query().count();
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
    let count = self.query().count();
    let old_selection = sanitize_selection(self.selection, count);
    let new_selection = sanitize_selection(selection, count);

    self.selection = selection;
    new_selection != old_selection
  }

  /// Change the currently selected task.
  fn select(&mut self, change: isize) -> bool {
    // We always make sure to base the given `change` value off of a
    // sanitized selection. Otherwise the result is not as expected.
    let count = self.query().count();
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
    self.query().nth(selection).unwrap()
  }
}

impl Handleable for TaskListBox {
  /// Check for new input and react to it.
  fn handle(&mut self, event: Event, _cap: &mut dyn Cap) -> Option<UiEvents> {
    match event {
      Event::KeyDown(key) |
      Event::KeyUp(key) => {
        match key {
          Key::Char(' ') => {
            let mut task = self.selected_task();
            let id = task.id();
            task.toggle_complete();
            self.tasks.borrow_mut().update(task);
            self.handle_select_task_start(id).update()
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
              let other = self.query().nth(self.some_selection(1));
              if let Some(other) = other {
                self.tasks.borrow_mut().move_after(to_move.id(), other.id());
                self.select(1);
                (None as Option<Event>).update()
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
              let other = self.query().nth(self.some_selection(-1));
              if let Some(other) = other {
                self.tasks.borrow_mut().move_before(to_move.id(), other.id());
                self.select(-1);
                (None as Option<Event>).update()
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
  fn handle_custom(&mut self, event: Box<dyn Any>, _cap: &mut dyn Cap) -> Option<UiEvents> {
    match event.downcast::<TermUiEvent>() {
      Ok(e) => self.handle_custom_event(e),
      Err(e) => panic!("Received unexpected custom event: {:?}", e),
    }
  }

  /// Handle a custom event.
  fn handle_custom_ref(&mut self, event: &mut dyn Any, _cap: &mut dyn Cap) -> Option<UiEvents> {
    match event.downcast_mut::<TermUiEvent>() {
      Some(e) => self.handle_custom_event_ref(e),
      None => panic!("Received unexpected custom event"),
    }
  }
}
