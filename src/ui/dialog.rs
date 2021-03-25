// Copyright (C) 2021 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::Ordering;
use std::collections::HashSet;

use async_trait::async_trait;

use gui::derive::Widget;
use gui::Cap;
use gui::Handleable;
use gui::Id;
use gui::MutCap;
use gui::Widget;

use crate::tags::Tag;
use crate::tasks::Task;

use super::event::Event;
use super::event::Key;
use super::message::Message;
use super::message::MessageExt;
use super::modal::Modal;
use super::selectable::Selectable;


/// An enum for tags present on a task.
#[derive(Clone, Debug, PartialEq)]
pub enum SetUnsetTag {
  /// A set tag of a task.
  Set(Tag),
  /// A template for a tag.
  Unset(Tag),
}


/// A comparison function for two `Tag` objects, sorting them
/// by their names.
fn cmp_template(lhs: &Tag, rhs: &Tag) -> Ordering {
  lhs.name().to_lowercase().cmp(&rhs.name().to_lowercase())
}


/// Prepare a properly sorted list of tags mirroring those of the
/// provided task.
fn prepare_tags(task: &Task) -> Vec<SetUnsetTag> {
  let set = task
    .tags()
    .map(|tag| tag.template().id())
    .collect::<HashSet<_>>();

  let mut unset = task
    .templates()
    .filter(|template| !set.contains(&template.id()))
    .map(Tag::new)
    .collect::<Vec<_>>();
  unset.sort_by(cmp_template);

  let mut set = task.tags().cloned().collect::<Vec<_>>();
  set.sort_by(cmp_template);

  set
    .into_iter()
    .map(SetUnsetTag::Set)
    .chain(unset.into_iter().map(SetUnsetTag::Unset))
    .collect::<Vec<_>>()
}


#[derive(Debug, PartialEq)]
struct Data {
  /// The ID of the previously focused widget.
  prev_focused: Option<Id>,
  /// The task for which to configure the tags.
  task: Task,
  /// The tags to configure.
  tags: Vec<SetUnsetTag>,
  /// The currently selected tag.
  selection: isize,
}

#[allow(unused)]
impl Data {
  /// Create a new `Data` object from the given `Task` object.
  fn new(task: Task) -> Self {
    let tags = prepare_tags(&task);

    Self {
      prev_focused: None,
      task,
      tags,
      selection: 0,
    }
  }
}


/// The data associated with a `Dialog` widget.
#[derive(Debug)]
pub struct DialogData {
  /// The "inner" data, set when the dialog is active.
  data: Option<Data>,
}

impl DialogData {
  pub fn new() -> Self {
    Self { data: None }
  }
}

impl Selectable for DialogData {
  fn selection_index(&self) -> isize {
    self
      .data
      .as_ref()
      .map(|data| data.selection)
      .expect("dialog has no data set")
  }

  fn set_selection_index(&mut self, selection: isize) {
    self
      .data
      .as_mut()
      .map(|mut data| data.selection = selection)
      .expect("dialog has no data set")
  }

  fn count(&self) -> usize {
    self
      .data
      .as_ref()
      .map(|data| data.tags.len())
      .expect("dialog has no data set")
  }
}


/// A modal dialog used for editing a task's tags.
#[derive(Debug, Widget)]
#[gui(Event = Event, Message = Message)]
pub struct Dialog {
  id: Id,
}

impl Dialog {
  /// Create a new `Dialog`.
  pub fn new(id: Id) -> Self {
    Self { id }
  }

  /// Handle a key press.
  async fn handle_key(&self, cap: &mut dyn MutCap<Event, Message>, key: Key) -> Option<Message> {
    let data = self.data_mut::<DialogData>(cap);
    match key {
      Key::Esc | Key::Char('\n') | Key::Char('q') => {
        self.restore_focus(cap);
        cap.hide(self.id);

        let data = self.data_mut::<DialogData>(cap);
        data.data = None;

        Some(Message::Updated)
      },
      Key::Char('g') => MessageExt::maybe_update(None, data.select(0)),
      Key::Char('G') => MessageExt::maybe_update(None, data.select(isize::MAX)),
      Key::Char('j') => MessageExt::maybe_update(None, data.change_selection(1)),
      Key::Char('k') => MessageExt::maybe_update(None, data.change_selection(-1)),
      _ => None,
    }
  }
}

impl Modal for Dialog {
  fn prev_focused(&self, cap: &dyn Cap) -> Option<Id> {
    let data = self.data::<DialogData>(cap);
    data
      .data
      .as_ref()
      .map(|data| data.prev_focused)
      .expect("dialog has no data set")
  }

  fn set_prev_focused(&self, cap: &mut dyn MutCap<Event, Message>, focused: Option<Id>) {
    let data = self.data_mut::<DialogData>(cap);
    data
      .data
      .as_mut()
      .map(|mut data| data.prev_focused = focused)
      .expect("dialog has no data set")
  }
}

#[async_trait(?Send)]
impl Handleable<Event, Message> for Dialog {
  /// Handle an event.
  async fn handle(&self, cap: &mut dyn MutCap<Event, Message>, event: Event) -> Option<Event> {
    match event {
      Event::Key(key, _raw) => self.handle_key(cap, key).await.into_event(),
      _ => Some(event),
    }
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  use std::rc::Rc;

  use crate::tags::Template;
  use crate::tags::Templates;
  use crate::tags::COMPLETE_TAG;


  #[test]
  fn tag_preparation() {
    let template_list = vec![
      Template::new("foobaz"),
      Template::new("Z"),
      Template::new("a"),
      Template::new("foobar"),
    ];

    let mut templates = Templates::new();
    templates.extend(template_list);
    let templates = Rc::new(templates);

    // We have two tags set.
    let tags = vec![
      templates.instantiate_from_name("foobaz"),
      templates.instantiate_from_name("foobar"),
    ];

    let task = Task::with_summary_and_tags("do something, mate", tags, templates.clone());
    let tags = prepare_tags(&task);
    let expected = vec![
      SetUnsetTag::Set(templates.instantiate_from_name("foobar")),
      SetUnsetTag::Set(templates.instantiate_from_name("foobaz")),
      SetUnsetTag::Unset(templates.instantiate_from_name("a")),
      SetUnsetTag::Unset(templates.instantiate_from_name(COMPLETE_TAG)),
      SetUnsetTag::Unset(templates.instantiate_from_name("Z")),
    ];
    assert_eq!(tags, expected);
  }
}
