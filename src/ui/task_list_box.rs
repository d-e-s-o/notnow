// Copyright (C) 2018-2021 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::min;
use std::isize;
use std::rc::Rc;

use async_trait::async_trait;

use cell::RefCell;

use gui::derive::Widget;
use gui::Cap;
use gui::Handleable;
use gui::Id;
use gui::MutCap;
use gui::Widget;

use crate::query::Query;
use crate::tasks::Id as TaskId;
use crate::tasks::Task;
use crate::tasks::Tasks;

use super::event::Event;
use super::event::Key;
use super::in_out::InOut;
use super::message::Message;
use super::message::MessageExt;
use super::selectable::Selectable;
use super::tab_bar::SearchState;
use super::tab_bar::TabState;


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

  /// Retrieve a copy of the selected task, if any.
  fn selected_task(&self) -> Option<Task> {
    let selection = self.selection(0);
    self.query.iter().clone().cloned().nth(selection)
  }
}

impl Selectable for TaskListBoxData {
  fn selection_index(&self) -> isize {
    self.selection
  }

  fn set_selection_index(&mut self, selection: isize) {
    self.selection = selection
  }

  fn count(&self) -> usize {
    self.query.iter().clone().count()
  }
}


/// A widget representing a list of `Task` objects.
#[derive(Debug, Widget)]
#[gui(Event = Event, Message = Message)]
pub struct TaskListBox {
  id: Id,
  tab_bar: Id,
  dialog: Id,
  in_out: Id,
}

impl TaskListBox {
  /// Create a new `TaskListBox` widget.
  pub fn new(
    id: Id,
    cap: &mut dyn MutCap<Event, Message>,
    tab_bar: Id,
    dialog: Id,
    in_out: Id,
    selected: Option<usize>,
  ) -> Self {
    let task_list_box = Self {
      id,
      tab_bar,
      dialog,
      in_out,
    };
    let data = task_list_box.data_mut::<TaskListBoxData>(cap);
    let selected = selected.map(|x| min(x, isize::MAX as usize)).unwrap_or(0) as isize;
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
      let update = data.select(idx as isize);
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
          count
            .saturating_sub(1)
            .saturating_sub(data.selection(-offset))
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
      let update = data.select(idx as isize);
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
  async fn handle(&self, cap: &mut dyn MutCap<Event, Message>, event: Event) -> Option<Event> {
    let data = self.data_mut::<TaskListBoxData>(cap);
    match event {
      Event::Key(key, _) => match key {
        Key::Char(' ') => {
          if let Some(mut task) = data.selected_task() {
            let id = task.id();
            task.toggle_complete();
            data.tasks.borrow_mut().update(task);
            self
              .select_task(cap, id)
              .await
              .maybe_update(true)
              .into_event()
          } else {
            None
          }
        },
        Key::Char('a') => {
          data.state = Some(State::Add);
          let message = Message::SetInOut(InOut::Input("".to_string(), 0));
          cap.send(self.in_out, message).await.into_event()
        },
        Key::Char('d') => {
          if let Some(task) = data.selected_task() {
            data.tasks.borrow_mut().remove(task.id());
            MessageExt::maybe_update(None, true).into_event()
          } else {
            None
          }
        },
        Key::Char('e') => {
          if let Some(task) = data.selected_task() {
            let string = task.summary.clone();
            let idx = string.len();
            data.state = Some(State::Edit(task));

            let message = Message::SetInOut(InOut::Input(string, idx));
            cap.send(self.in_out, message).await.into_event()
          } else {
            None
          }
        },
        Key::Char('t') => {
          if let Some(task) = data.selected_task() {
            let message = Message::EditTags(task);
            cap.send(self.dialog, message).await.into_event()
          } else {
            None
          }
        },
        Key::Char('J') => {
          if let Some(to_move) = data.selected_task() {
            let other = data.query.iter().nth(data.selection(1));
            if let Some(other) = other {
              data.tasks.borrow_mut().move_after(to_move.id(), other.id());
              MessageExt::maybe_update(None, data.change_selection(1)).into_event()
            } else {
              None
            }
          } else {
            None
          }
        },
        Key::Char('K') => {
          if let Some(to_move) = data.selected_task() {
            if data.selection(0) > 0 {
              let other = data.query.iter().nth(data.selection(-1));
              if let Some(other) = other {
                data
                  .tasks
                  .borrow_mut()
                  .move_before(to_move.id(), other.id());
                MessageExt::maybe_update(None, data.change_selection(-1)).into_event()
              } else {
                None
              }
            } else {
              None
            }
          } else {
            None
          }
        },
        Key::Char('g') => MessageExt::maybe_update(None, data.select(0)).into_event(),
        Key::Char('G') => MessageExt::maybe_update(None, data.select(isize::MAX)).into_event(),
        Key::Char('j') => MessageExt::maybe_update(None, data.change_selection(1)).into_event(),
        Key::Char('k') => MessageExt::maybe_update(None, data.change_selection(-1)).into_event(),
        _ => Some(event),
      },
      _ => Some(event),
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
                let tags = if let Some(mut task) = data.selected_task() {
                  if task.is_complete() {
                    task.toggle_complete()
                  }
                  let tags = task.tags().cloned().collect();
                  tags
                } else {
                  Default::default()
                };

                let id = data.tasks.borrow_mut().add(text.clone(), tags);
                // We want the new task to be displayed after the
                // currently selected one.
                if let Some(current) = data.selected_task() {
                  // TODO: This movement may lead to a bit surprising
                  //       placement for tasks that were previously
                  //       tagged 'complete', because we move the new
                  //       task just after this one, but given that we
                  //       removed the tag it may end up being displayed
                  //       on a different query altogether -- and at a
                  //       rather random seeming location because of it.
                  //       Eventually we may want to remove the special
                  //       case logic for the 'complete' tag.
                  data.tasks.borrow_mut().move_after(id, current.id());
                }
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
      Message::UpdateTask(task) => {
        data.tasks.borrow_mut().update(task);
        Some(Message::Updated)
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
