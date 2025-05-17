// Copyright (C) 2024 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::rc::Rc;

use async_trait::async_trait;

use gui::derive::Widget;
use gui::Cap;
use gui::Handleable;
use gui::Id;
use gui::MutCap;
use gui::Widget;

use crate::tasks::Task;
use crate::text::EditableText;

use super::event::Event;
use super::input::InputResult;
use super::input::InputText;
use super::message::Message;
use super::message::MessageExt;
use super::modal::Modal;


#[derive(Debug)]
struct Data {
  /// The ID of the previously focused widget.
  prev_focused: Option<Id>,
  /// The task for which to edit the free-form details.
  task: Rc<Task>,
  /// The task for which to edit the free-form details.
  to_edit: Task,
  /// The task's details.
  details: InputText,
}

impl Data {
  /// Create a new `Data` object from the given `Task` object.
  fn new(task: Rc<Task>, to_edit: Task) -> Self {
    let mut text = EditableText::from_string(to_edit.details());
    let () = text.move_end();

    Self {
      prev_focused: None,
      task,
      details: InputText::builder().with_multi_line(true).build(text),
      to_edit,
    }
  }

  /// Convert the `Data` into a `Task` (and its ID) with updated tags.
  fn into_task(self) -> (Rc<Task>, Task) {
    (self.task, self.to_edit)
  }
}


/// The data associated with a `DetailDialog` widget.
#[derive(Debug)]
pub struct DetailDialogData {
  /// The "inner" data, set when the dialog is active.
  data: Option<Data>,
}

impl DetailDialogData {
  pub fn new() -> Self {
    Self { data: None }
  }

  /// Retrieve the dialog's [`EditableText`].
  pub fn details(&self) -> &EditableText {
    self
      .data
      .as_ref()
      .map(|data| &data.details)
      .expect("detail dialog has no data set")
  }
}


/// A modal dialog used for editing a task's details.
#[derive(Debug, Widget)]
#[gui(Event = Event, Message = Message)]
pub struct DetailDialog {
  id: Id,
}

impl DetailDialog {
  /// Create a new `DetailDialog`.
  pub fn new(id: Id) -> Self {
    Self { id }
  }
}

impl Modal for DetailDialog {
  fn prev_focused(&self, cap: &dyn Cap) -> Option<Id> {
    let data = self.data::<DetailDialogData>(cap);
    data
      .data
      .as_ref()
      .map(|data| data.prev_focused)
      .expect("dialog has no data set")
  }

  fn set_prev_focused(&self, cap: &mut dyn MutCap<Event, Message>, focused: Option<Id>) {
    let data = self.data_mut::<DetailDialogData>(cap);
    data
      .data
      .as_mut()
      .map(|data| data.prev_focused = focused)
      .expect("dialog has no data set")
  }
}

#[async_trait(?Send)]
impl Handleable<Event, Message> for DetailDialog {
  /// Handle an event.
  async fn handle(&self, cap: &mut dyn MutCap<Event, Message>, event: Event) -> Option<Event> {
    match event {
      Event::Key(key, raw) => {
        let data = self.data_mut::<DetailDialogData>(cap);
        let data = data.data.as_mut().unwrap();

        let message = match data.details.handle_key(key, &raw) {
          InputResult::Completed(text) => {
            let widget = self.restore_focus(cap);
            let () = cap.hide(self.id);

            let data = self.data_mut::<DetailDialogData>(cap);
            let data = data.data.take();
            let (task, mut updated) = data.map(Data::into_task).expect("dialog has no data set");
            let () = updated.set_details(text);
            cap.send(widget, Message::UpdateTask(task, updated)).await
          },
          InputResult::Canceled => {
            let _widget = self.restore_focus(cap);
            let () = cap.hide(self.id);

            let data = self.data_mut::<DetailDialogData>(cap);
            let _data = data.data.take();
            // SANITY: We know that this dialog has a parent.
            Some(Message::updated(cap.parent_id(self.id).unwrap()))
          },
          InputResult::Updated => Some(Message::updated(self.id)),
          InputResult::Unchanged => None,
        };

        message.into_event()
      },
      _ => Some(event),
    }
  }

  /// React to a message.
  async fn react(&self, message: Message, cap: &mut dyn MutCap<Event, Message>) -> Option<Message> {
    match message {
      Message::EditDetails(task, edited) => {
        let data = self.data_mut::<DetailDialogData>(cap);
        debug_assert!(data.data.is_none());
        data.data = Some(Data::new(task, edited));

        let () = self.make_focused(cap);
        Some(Message::updated(self.id))
      },
      message => panic!("Received unexpected message: {message:?}"),
    }
  }
}
