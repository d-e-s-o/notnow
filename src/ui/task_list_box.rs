// Copyright (C) 2018-2024 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::min;
use std::isize;
use std::ops::Deref as _;
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
use crate::text::EditableText;
use crate::view::View;

use super::event::Event;
use super::event::Key;
use super::in_out::InOut;
use super::input::InputText;
use super::message::Message;
use super::message::MessageExt;
use super::selectable::Selectable;
use super::tab_bar::SearchState;
use super::tab_bar::TabState;


/// An enum representing the state a `TaskListBox` can be in.
#[derive(Debug)]
enum State {
  Add,
  Edit { task: Rc<Task>, edited: Task },
}


/// The data associated with a `TaskListBox`.
pub struct TaskListBoxData {
  /// The tasks database.
  tasks: Rc<Tasks>,
  /// The view represented by the `TaskListBox`.
  view: View,
  /// The tag to toggle on a task on press of the respective key.
  toggle_tag: Option<Tag>,
  /// The currently selected task.
  selection: isize,
  /// The state the `TaskListBox` is in.
  state: Option<State>,
}

impl TaskListBoxData {
  /// Create a new `TaskListBoxData` object.
  pub fn new(tasks: Rc<Tasks>, view: View, toggle_tag: Option<Tag>) -> Self {
    Self {
      tasks,
      view,
      toggle_tag,
      selection: 0,
      state: None,
    }
  }

  /// Retrieve the selected task and its ID, if any.
  fn selected_task(&self) -> Option<Rc<Task>> {
    let selection = self.selection(0);
    self.view.iter(|mut iter| iter.nth(selection).cloned())
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
    self.view.iter(|iter| iter.count())
  }
}


/// A widget representing a list of `Task` objects.
#[derive(Debug, Widget)]
#[gui(Event = Event, Message = Message)]
pub struct TaskListBox {
  id: Id,
  tab_bar: Id,
  detail_dialog: Id,
  tag_dialog: Id,
  in_out: Id,
}

impl TaskListBox {
  /// Create a new `TaskListBox` widget.
  pub fn new(
    id: Id,
    cap: &mut dyn MutCap<Event, Message>,
    tab_bar: Id,
    detail_dialog: Id,
    tag_dialog: Id,
    in_out: Id,
    selected: Option<usize>,
  ) -> Self {
    let task_list_box = Self {
      id,
      tab_bar,
      detail_dialog,
      tag_dialog,
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
  /// be covered by our view anymore, meaning we either need to select
  /// it or find somebody else who displays it and ask him to select it.
  async fn handle_select_task(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    task: Rc<Task>,
    done: Option<&mut bool>,
  ) -> Option<Message> {
    let data = self.data_mut::<TaskListBoxData>(cap);
    let idx = data
      .view
      .iter(|mut iter| iter.position(|_task| Rc::ptr_eq(_task, &task)));

    if let Some(idx) = idx {
      if let Some(done) = done {
        *done = true
      }
      data.select(idx as isize).then_some(Message::Updated)
    } else {
      // If there is no `done` we were called directly from within the
      // widget and not in response to a message from the TabBar. In
      // that case, given that we have not found the task in our own
      // view, reach out to the TabBar.
      if done.is_none() {
        let message = Message::SelectTask(task, false);
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
    task: Rc<Task>,
  ) -> Option<Message> {
    self.handle_select_task(cap, task, None).await
  }

  /// Search for a task containing the given string.
  fn search_task_index(
    &self,
    cap: &dyn Cap,
    string: &str,
    search_state: &SearchState,
    reverse: bool,
    exact: bool,
  ) -> Option<usize> {
    let data = self.data::<TaskListBoxData>(cap);
    // Note that because we use the count for index calculation
    // purposes, we subtract one below on every use.
    let count = data.count();
    // First figure out from where we start the search. If we have
    // visited this `TaskListBox` beforehand we may have already visited
    // the first couple of tasks matching the given string and we should
    // skip those.
    let start_idx = match search_state {
      SearchState::Current | SearchState::AfterCurrent => {
        let offset = matches!(search_state, SearchState::AfterCurrent) as isize;
        if reverse {
          count.checked_sub(1)?.checked_sub(data.selection(-offset))?
        } else {
          data.selection(offset)
        }
      },
      SearchState::First => 0,
      SearchState::Done => unreachable!(),
    };

    let cmp_exact = |task: &Rc<Task>| task.summary() == string;
    let cmp_vague = |task: &Rc<Task>| task.summary().to_lowercase().contains(string);

    let check = if exact {
      &cmp_exact as &dyn Fn(&Rc<Task>) -> bool
    } else {
      &cmp_vague as &dyn Fn(&Rc<Task>) -> bool
    };
    // Note that a simpler version of this find magic would just use
    // the `enumerate` functionality. However, for some reason that
    // would require us to work with an `ExactSizeIterator`, which is
    // not something that we can provide.
    if reverse {
      data.view.iter(|iter| {
        iter
          .rev()
          .skip(start_idx)
          .position(check)
          .map(|idx| count.saturating_sub(start_idx + idx + 1))
      })
    } else {
      data.view.iter(|iter| {
        iter
          .skip(start_idx)
          .position(check)
          .map(|idx| start_idx + idx)
      })
    }
  }

  /// Handle a `Message::SearchTask` event.
  async fn handle_search_task(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    string: &str,
    search_state: &mut SearchState,
    reverse: bool,
    exact: bool,
  ) -> Option<Message> {
    let idx = self.search_task_index(cap, string, search_state, reverse, exact);
    if let Some(idx) = idx {
      *search_state = SearchState::Done;

      let data = self.data_mut::<TaskListBoxData>(cap);
      data.select(idx as isize).then_some(Message::Updated)
    } else {
      None
    }
  }

  /// Retrieve the view associated with this widget.
  pub fn view(&self, cap: &dyn Cap) -> View {
    let data = self.data::<TaskListBoxData>(cap);
    data.view.clone()
  }

  /// Retrieve the "toggle tag", if any is configured.
  pub fn toggle_tag(&self, cap: &dyn Cap) -> Option<Tag> {
    let data = self.data::<TaskListBoxData>(cap);
    data.toggle_tag.clone()
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
          if let Some(task) = data.selected_task() {
            if let Some(toggle_tag) = &data.toggle_tag {
              // Make a deep copy of the task to work on.
              let mut updated = task.deref().clone();
              if !updated.unset_tag(toggle_tag) {
                updated.set_tag(toggle_tag.clone());
              }
              cap
                .send(self.id, Message::UpdateTask(task, updated))
                .await
                .into_event()
            } else {
              None
            }
          } else {
            None
          }
        },
        Key::Char('a') => {
          data.state = Some(State::Add);
          let message = Message::SetInOut(InOut::Input(InputText::default()));
          cap.send(self.in_out, message).await.into_event()
        },
        Key::Char('d') => {
          if let Some(task) = data.selected_task() {
            let () = data.tasks.remove(task);
            Some(Event::Updated)
          } else {
            None
          }
        },
        Key::Char('e') => {
          if let Some(task) = data.selected_task() {
            // Make a deep copy of the task.
            let edited = Task::clone(task.deref());
            let string = edited.summary();
            data.state = Some(State::Edit { task, edited });

            let mut text = EditableText::from_string(string);
            let () = text.move_end();

            let message = Message::SetInOut(InOut::Input(InputText::new(text)));
            cap.send(self.in_out, message).await.into_event()
          } else {
            None
          }
        },
        Key::Char('t') => {
          if let Some(task) = data.selected_task() {
            // Make a deep copy of the task to work on.
            let edited = Task::clone(task.deref());
            let message = Message::EditTags(task, edited);
            cap.send(self.tag_dialog, message).await.into_event()
          } else {
            None
          }
        },
        Key::Char('\n') => {
          if let Some(task) = data.selected_task() {
            let edited = Task::clone(task.deref());
            let message = Message::EditDetails(task, edited);
            cap.send(self.detail_dialog, message).await.into_event()
          } else {
            None
          }
        },
        Key::Char('J') => {
          if let Some(to_move) = data.selected_task() {
            let other = data
              .view
              .iter(|mut iter| iter.nth(data.selection(1)).cloned());
            if let Some(other) = other {
              let () = data.tasks.move_after(to_move, other);
              data.change_selection(1).then_some(Event::Updated)
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
              let other = data
                .view
                .iter(|mut iter| iter.nth(data.selection(-1)).cloned());
              if let Some(other) = other {
                let () = data.tasks.move_before(to_move, other);
                data.change_selection(-1).then_some(Event::Updated)
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
        Key::Char('g') => data.select(0).then_some(Event::Updated),
        Key::Char('G') => data.select(isize::MAX).then_some(Event::Updated),
        Key::Char('j') => data.change_selection(1).then_some(Event::Updated),
        Key::Char('k') => data.change_selection(-1).then_some(Event::Updated),
        Key::Char('*') => {
          if let Some(selected) = data.selected_task() {
            let message = Message::StartTaskSearch(selected.summary());
            cap.send(self.tab_bar, message).await.into_event()
          } else {
            None
          }
        },
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
                let tags = if let Some(task) = data.selected_task() {
                  // Copy all tags except for the one that we allow
                  // toggling.
                  task.tags(|iter| {
                    iter
                      .filter(|tag| Some(*tag) != data.toggle_tag.as_ref())
                      .cloned()
                      .collect()
                  })
                } else {
                  // If there is no selected task to take as a
                  // "template", fall back to assigning all positive
                  // literals from the view as tags. The user can always
                  // deselect them, but having it show up without tags
                  // (which would be the only other way we can conjure
                  // up to handle this case), is much worse of a user
                  // experience.
                  data.view.positive_tag_iter().cloned().collect()
                };

                // We want the new task to be displayed after the
                // currently selected one, so find the ID of the
                // currently selected task first.
                // TODO: The movement initiated here may lead to a bit
                //       surprising placement for tasks that were
                //       previously tagged 'complete', because we move
                //       the new task just after this one, but given
                //       that we removed the tag it may end up being
                //       displayed on a different view altogether -- and
                //       at a rather random seeming location because of
                //       it. Eventually we may want to remove the
                //       special case logic for the 'complete' tag.
                let after = data.selected_task();
                let task = data.tasks.add(text.clone(), tags, after);
                self.select_task(cap, task).await
              } else {
                None
              }
            },
            State::Edit { task, mut edited } => {
              // Editing a task to empty just removes the task
              // altogether.
              if !text.is_empty() {
                edited.set_summary(text.clone());
                data.tasks.update(task.clone(), edited);
                self
                  .select_task(cap, task)
                  .await
                  .maybe_update(Some(Message::Updated))
              } else {
                data.tasks.remove(task);
                Some(Message::Updated)
              }
            },
          }
        } else {
          cap.send(self.tab_bar, message).await
        }
      },
      Message::UpdateTask(task, updated) => {
        data.tasks.update(task.clone(), updated);

        // Try to select the task now that something may have changed
        // (such as its tags).
        self
          .select_task(cap, task)
          .await
          .maybe_update(Some(Message::Updated))
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
      Message::SelectTask(task, done) => {
        self.handle_select_task(cap, task.clone(), Some(done)).await
      },
      Message::SearchTask(string, search_state, reverse, exact) => {
        self
          .handle_search_task(cap, string, search_state, *reverse, *exact)
          .await
      },
      Message::GetTabState(ref mut tab_state) => {
        let TabState { ref mut views, .. } = tab_state;
        let data = self.data::<TaskListBoxData>(cap);
        let selected = Some(data.selection(0));

        views.push((self.view(cap), selected));
        None
      },
      message => panic!("Received unexpected message: {message:?}"),
    }
  }
}
