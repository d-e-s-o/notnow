// Copyright (C) 2021-2022 Daniel Mueller (deso@posteo.net)
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
use crate::tasks::Id as TaskId;
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

impl SetUnsetTag {
  /// Retrieve the tag's name.
  pub fn name(&self) -> &str {
    match self {
      Self::Unset(template) | Self::Set(template) => template.name(),
    }
  }

  /// Check whether the tag is set.
  pub fn is_set(&self) -> bool {
    match self {
      Self::Set(_) => true,
      Self::Unset(_) => false,
    }
  }

  /// Toggle the tag.
  fn toggle(&mut self) {
    *self = match self {
      Self::Set(tag) => Self::Unset(tag.clone()),
      Self::Unset(tag) => Self::Set(tag.clone()),
    };
  }
}


/// A comparison function for two `Tag` objects, sorting them
/// by their names.
fn cmp_template(lhs: &Tag, rhs: &Tag) -> Ordering {
  lhs.name().to_lowercase().cmp(&rhs.name().to_lowercase())
}


/// Prepare a properly sorted list of tags mirroring those of the
/// provided task.
fn prepare_tags(task: &Task) -> Vec<SetUnsetTag> {
  let set = task.tags(|iter| iter.map(|tag| tag.template()).collect::<HashSet<_>>());
  let mut unset = task
    .templates()
    .iter()
    .filter(|template| !set.contains(template))
    .map(Tag::new)
    .collect::<Vec<_>>();
  unset.sort_by(cmp_template);

  let mut set = task.tags(|iter| iter.cloned().collect::<Vec<_>>());
  set.sort_by(cmp_template);

  set
    .into_iter()
    .map(SetUnsetTag::Set)
    .chain(unset.into_iter().map(SetUnsetTag::Unset))
    .collect::<Vec<_>>()
}


/// An enum indicating in which direction to search for the next desired
/// entry.
#[derive(Copy, Clone, Debug, PartialEq)]
enum Direction {
  /// Search for the next desired entry in forward direction.
  Forward,
  /// Search for the next desired entry in backward direction.
  Backward,
}


#[derive(Debug)]
struct Data {
  /// The ID of the previously focused widget.
  prev_focused: Option<Id>,
  /// The ID of the task for which to configure the tags.
  task_id: TaskId,
  /// The task for which to configure the tags.
  to_edit: Task,
  /// The tags to configure.
  tags: Vec<SetUnsetTag>,
  /// The currently selected tag.
  selection: isize,
  /// Whether the user has started a "jump to" operation.
  jump_to: Option<Direction>,
}

impl Data {
  /// Create a new `Data` object from the given `Task` object.
  fn new(task_id: TaskId, to_edit: Task) -> Self {
    let tags = prepare_tags(&to_edit);

    Self {
      prev_focused: None,
      task_id,
      to_edit,
      tags,
      selection: 0,
      jump_to: None,
    }
  }

  /// Jump to the next tag beginning with the given character, moving
  /// in the provided direction.
  fn select_task_beginning_with(&mut self, c: char, direction: Direction) -> bool {
    let pattern = &c.to_lowercase().to_string();
    let new_selection = match direction {
      Direction::Forward => self
        .tags
        .iter()
        .enumerate()
        .skip(self.selection(1))
        .find(|(_, tag)| tag.name().to_lowercase().starts_with(pattern)),
      Direction::Backward => self
        .tags
        .iter()
        .enumerate()
        .rev()
        .skip(self.count() - self.selection(0))
        .find(|(_, tag)| tag.name().to_lowercase().starts_with(pattern)),
    };

    if let Some((new_selection, _)) = new_selection {
      self.set_selection_index(new_selection as isize);
      true
    } else {
      false
    }
  }

  /// Convert the `Data` into a `Task` (and its ID) with updated tags.
  fn into_task(mut self) -> (TaskId, Task) {
    let tags = self.tags.into_iter().filter_map(|tag| match tag {
      SetUnsetTag::Set(tag) => Some(tag),
      SetUnsetTag::Unset(_) => None,
    });

    self.to_edit.set_tags(tags);
    (self.task_id, self.to_edit)
  }
}

impl Selectable for Data {
  fn selection_index(&self) -> isize {
    self.selection
  }

  fn set_selection_index(&mut self, selection: isize) {
    self.selection = selection
  }

  fn count(&self) -> usize {
    self.tags.len()
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

  /// Retrieve a reference to the selected tag, if any.
  fn selected_tag(&mut self) -> Option<&mut SetUnsetTag> {
    let selection = self.selection(0);
    self
      .data
      .as_mut()
      .map(|data| data.tags.get_mut(selection))
      .expect("dialog has no data set")
  }

  /// Toggle the currently selected tag, if any.
  fn toggle_tag(&mut self) -> bool {
    self
      .selected_tag()
      .map(|tag| {
        tag.toggle();
        true
      })
      .unwrap_or(false)
  }
}

impl Selectable for DialogData {
  fn selection_index(&self) -> isize {
    self
      .data
      .as_ref()
      .map(|data| data.selection_index())
      .expect("dialog has no data set")
  }

  fn set_selection_index(&mut self, selection: isize) {
    self
      .data
      .as_mut()
      .map(|data| data.set_selection_index(selection))
      .expect("dialog has no data set")
  }

  fn count(&self) -> usize {
    self
      .data
      .as_ref()
      .map(|data| data.count())
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
  #[allow(clippy::option_map_unit_fn)]
  async fn handle_key(&self, cap: &mut dyn MutCap<Event, Message>, key: Key) -> Option<Message> {
    if let Some(result) = self.handle_jump_to(cap, key) {
      return result
    }

    let data = self.data_mut::<DialogData>(cap);
    match key {
      Key::Esc | Key::Char('\n') | Key::Char('q') => {
        let widget = self.restore_focus(cap);
        cap.hide(self.id);

        let data = self.data_mut::<DialogData>(cap);
        let data = data.data.take();

        if key == Key::Char('\n') {
          let (task_id, updated) = data
            .map(|data| data.into_task())
            .expect("dialog has no data set");
          cap
            .send(widget, Message::UpdateTask(task_id, updated))
            .await;
        }

        Some(Message::Updated)
      },
      Key::Char(' ') => MessageExt::maybe_update(None, data.toggle_tag()),
      Key::Char('f') => {
        data
          .data
          .as_mut()
          .map(|data| data.jump_to = Some(Direction::Forward));
        None
      },
      Key::Char('F') => {
        data
          .data
          .as_mut()
          .map(|data| data.jump_to = Some(Direction::Backward));
        None
      },
      Key::Char('g') => MessageExt::maybe_update(None, data.select(0)),
      Key::Char('G') => MessageExt::maybe_update(None, data.select(isize::MAX)),
      Key::Char('j') => MessageExt::maybe_update(None, data.change_selection(1)),
      Key::Char('k') => MessageExt::maybe_update(None, data.change_selection(-1)),
      _ => None,
    }
  }

  /// Handle any "jump to" action.
  fn handle_jump_to(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    key: Key,
  ) -> Option<Option<Message>> {
    let data = self
      .data_mut::<DialogData>(cap)
      .data
      .as_mut()
      .expect("dialog has no data set");

    match data.jump_to {
      Some(direction) => {
        data.jump_to = None;

        match key {
          Key::Char(c) => {
            let updated = data.select_task_beginning_with(c, direction);
            Some(MessageExt::maybe_update(None, updated))
          },
          // All non-char keys just reset the "jump to" flag directly and
          // will be handled they same way they would have been had it not
          // been set to begin with.
          _ => None,
        }
      },
      None => None,
    }
  }

  /// Retrieve the list of set/unset tags.
  pub fn tags<'cap>(&self, cap: &'cap dyn Cap) -> &'cap [SetUnsetTag] {
    let data = self.data::<DialogData>(cap);
    data
      .data
      .as_ref()
      .map(|data| &data.tags)
      .expect("dialog has no data set")
  }

  /// Retrieve the current selection index.
  ///
  /// The selection index indicates the currently selected tag.
  pub fn selection(&self, cap: &dyn Cap) -> usize {
    let data = self.data::<DialogData>(cap);
    data.selection(0)
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

  /// React to a message.
  async fn react(&self, message: Message, cap: &mut dyn MutCap<Event, Message>) -> Option<Message> {
    match message {
      Message::EditTags(task_id, task) => {
        let data = self.data_mut::<DialogData>(cap);
        debug_assert!(data.data.is_none());
        data.data = Some(Data::new(task_id, task));

        self.make_focused(cap);
        Some(Message::Updated)
      },
      m => panic!("Received unexpected message: {:?}", m),
    }
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  use std::ops::Deref as _;
  use std::rc::Rc;

  use crate::db::Db;
  use crate::db::UseDefault;
  use crate::tags::Templates;
  use crate::test::COMPLETE_TAG;


  #[test]
  fn tag_preparation() {
    let template_list = vec![COMPLETE_TAG, "foobaz", "Z", "a", "foobar"];
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

  #[test]
  fn data_tag_selection() {
    let template_list = vec![COMPLETE_TAG, "a", "b", "c", "c1", "d", "h", "z"];
    let mut templates = Templates::new();
    templates.extend(template_list);
    let templates = Rc::new(templates);

    // We have two tags set.
    let tags = vec![
      templates.instantiate_from_name("a"),
      templates.instantiate_from_name("h"),
      templates.instantiate_from_name("d"),
    ];

    // The full list of tags will look like this:
    // a, d, h, b, c, c1, complete, z
    let iter = [Task::with_summary_and_tags("task", tags, templates)].map(Rc::new);
    let db = Db::<_, UseDefault>::from_iter(iter);
    let entry = db.get(0).unwrap();
    // Make a deep copy of the task.
    let task = entry.deref().deref().clone();
    let mut data = Data::new(entry.id(), task);
    assert_eq!(data.selection, 0);

    assert!(!data.select_task_beginning_with('h', Direction::Backward));
    assert_eq!(data.selection, 0);
    assert!(data.select_task_beginning_with('h', Direction::Forward));
    assert_eq!(data.selection, 2);

    assert!(data.select_task_beginning_with('z', Direction::Forward));
    assert_eq!(data.selection, 7);

    assert!(data.select_task_beginning_with('c', Direction::Backward));
    assert_eq!(data.selection, 6);
    assert!(data.select_task_beginning_with('c', Direction::Backward));
    assert_eq!(data.selection, 5);
    assert!(data.select_task_beginning_with('c', Direction::Backward));
    assert_eq!(data.selection, 4);
    assert!(!data.select_task_beginning_with('c', Direction::Backward));
    assert_eq!(data.selection, 4);

    assert!(data.select_task_beginning_with('c', Direction::Forward));
    assert_eq!(data.selection, 5);
    assert!(data.select_task_beginning_with('c', Direction::Forward));
    assert_eq!(data.selection, 6);
    assert!(!data.select_task_beginning_with('c', Direction::Forward));
    assert_eq!(data.selection, 6);
  }
}
