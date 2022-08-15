// Copyright (C) 2017-2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::PartialEq;
use std::collections::BTreeMap;
use std::io::Error;
use std::io::ErrorKind;
use std::io::Result;
use std::mem::replace;
use std::rc::Rc;
use std::slice;

use crate::id::Id as IdT;
use crate::ops::Op;
use crate::ops::Ops;
use crate::ser::tasks::Task as SerTask;
use crate::ser::tasks::Tasks as SerTasks;
use crate::ser::ToSerde;
use crate::tags::Id as TagId;
use crate::tags::Tag;
use crate::tags::TagMap;
use crate::tags::Template;
use crate::tags::Templates;


/// The maximum number of undo steps that we keep record of.
// TODO: We may consider making this value user-configurable.
const MAX_UNDO_STEP_COUNT: usize = 64;


#[derive(Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct T(());

pub type Id = IdT<T>;


/// A struct representing a task item.
#[derive(Clone, Debug)]
pub struct Task {
  id: Id,
  pub summary: String,
  tags: BTreeMap<TagId, Tag>,
  templates: Rc<Templates>,
}

impl Task {
  /// Create a new task.
  #[cfg(test)]
  pub fn new(summary: impl Into<String>) -> Self {
    Self {
      id: Id::new(),
      summary: summary.into(),
      tags: Default::default(),
      templates: Rc::new(Templates::new()),
    }
  }

  /// Create a task using the given summary.
  pub fn with_summary_and_tags<S>(summary: S, tags: Vec<Tag>, templates: Rc<Templates>) -> Self
  where
    S: Into<String>,
  {
    Task {
      id: Id::new(),
      summary: summary.into(),
      tags: tags.into_iter().map(|x| (x.id(), x)).collect(),
      templates,
    }
  }

  /// Create a new task from a serializable one.
  fn with_serde(task: SerTask, templates: Rc<Templates>, map: &TagMap) -> Result<Task> {
    let mut tags = BTreeMap::new();
    for tag in task.tags.into_iter() {
      let id = map.get(&tag.id).ok_or_else(|| {
        let error = format!("Encountered invalid tag Id {}", tag.id);
        Error::new(ErrorKind::InvalidInput, error)
      })?;
      tags.insert(*id, templates.instantiate(*id));
    }

    Ok(Self {
      id: Id::new(),
      summary: task.summary,
      tags,
      templates,
    })
  }

  /// Retrieve this task's `Id`.
  pub fn id(&self) -> Id {
    self.id
  }

  /// Retrieve an iterator over this task's tags.
  pub fn tags(&self) -> impl Iterator<Item = &Tag> + Clone {
    self.tags.values()
  }

  /// Set the tags of the task.
  pub fn set_tags<I>(&mut self, tags: I)
  where
    I: Iterator<Item = Tag>,
  {
    self.tags = tags.map(|x| (x.id(), x)).collect();
  }

  /// Retrieve an iterator over all tag templates.
  pub fn templates(&self) -> impl Iterator<Item = Template> + '_ {
    self.templates.iter()
  }

  /// Check whether the task is tagged as complete or not.
  pub fn is_complete(&self) -> bool {
    let id = self.templates.complete_tag().id();
    self.tags.contains_key(&id)
  }

  /// Toggle the completion state of the task.
  pub fn toggle_complete(&mut self) {
    let id = self.templates.complete_tag().id();

    // Try removing the complete tag, if that succeeds we are done (as
    // the tag was present and got removed), otherwise insert it (as it
    // was not present).
    if self.tags.remove(&id).is_none() {
      let tag = self.templates.instantiate(id);
      self.tags.insert(id, tag);
    }
  }
}

impl ToSerde<SerTask> for Task {
  /// Convert this task into a serializable one.
  fn to_serde(&self) -> SerTask {
    SerTask {
      summary: self.summary.clone(),
      tags: self.tags.iter().map(|(_, x)| x.to_serde()).collect(),
    }
  }
}


/// Find the position of a task.
fn find_idx(tasks: &[Task], id: Id) -> usize {
  tasks.iter().position(|task| task.id() == id).unwrap()
}

/// Add a task to a vector of tasks.
fn add_task(tasks: &mut Vec<Task>, task: Task, target: Option<Target>) {
  if let Some(target) = target {
    let idx = find_idx(tasks, target.id());
    let idx = match target {
      Target::Before(..) => idx,
      Target::After(..) => idx + 1,
    };
    tasks.insert(idx, task);
  } else {
    tasks.push(task);
  }
}

/// Remove a task from a vector of tasks.
fn remove_task(tasks: &mut Vec<Task>, id: Id) -> (Task, usize) {
  let idx = find_idx(tasks, id);
  (tasks.remove(idx), idx)
}

/// Update a task in a vector of tasks.
fn update_task(tasks: &mut [Task], task: Task) -> Task {
  let idx = find_idx(tasks, task.id);
  replace(&mut tasks[idx], task)
}


/// An enumeration holding either a task ID or the full task.
///
/// This type is mostly an implementation detail of the `TaskOp`
/// enumeration.
#[derive(Debug)]
enum IdOrTask {
  /// The ID of a task.
  Id(Id),
  /// An actual task along with its position.
  Task(Task, usize),
}

impl IdOrTask {
  fn id(&self) -> Id {
    match self {
      Self::Id(id) => *id,
      Self::Task(task, ..) => task.id,
    }
  }

  fn task(&self) -> (&Task, usize) {
    match self {
      Self::Id(..) => panic!("IdOrTask does not contain a task"),
      Self::Task(task, position) => (task, *position),
    }
  }
}


/// An enum encoding the target location of a task: before or after a
/// task with a given ID.
#[derive(Clone, Copy, Debug)]
enum Target {
  /// The target is the spot before the given task.
  Before(Id),
  /// The target is the spot after the given task.
  After(Id),
}

impl Target {
  fn id(&self) -> Id {
    match self {
      Self::Before(id) | Self::After(id) => *id,
    }
  }
}


/// An operation to be performed on a task in a `Tasks` object.
#[derive(Debug)]
enum TaskOp {
  /// An operation adding a task.
  Add { task: Task, after: Option<Id> },
  /// An operation removing a task.
  Remove { id_or_task: IdOrTask },
  /// An operation updating a task.
  Update { updated: Task, before: Option<Task> },
  /// An operation changing a task's position.
  Move {
    from: usize,
    to: Target,
    task: Option<Task>,
  },
}

impl TaskOp {
  fn add(task: Task, after: Option<Id>) -> Self {
    Self::Add { task, after }
  }

  fn remove(id: Id) -> Self {
    Self::Remove {
      id_or_task: IdOrTask::Id(id),
    }
  }

  fn update(updated: Task) -> Self {
    Self::Update {
      updated,
      before: None,
    }
  }

  fn move_(from: usize, to: Target) -> Self {
    Self::Move {
      from,
      to,
      task: None,
    }
  }
}

impl Op<Vec<Task>, Option<Id>> for TaskOp {
  fn exec(&mut self, tasks: &mut Vec<Task>) -> Option<Id> {
    match self {
      Self::Add { task, after } => {
        add_task(tasks, task.clone(), after.map(Target::After));
        Some(task.id)
      },
      Self::Remove { id_or_task } => {
        let (task, idx) = remove_task(tasks, id_or_task.id());
        *id_or_task = IdOrTask::Task(task, idx);
        None
      },
      Self::Update { updated, before } => {
        let task = update_task(tasks, updated.clone());
        *before = Some(task);
        Some(updated.id)
      },
      Self::Move { from, to, task } => {
        let removed = tasks.remove(*from);
        // We do not support the case of moving a task with itself as a
        // target. Doing so should be prevented at a higher layer,
        // though.
        debug_assert_ne!(removed.id, to.id());
        add_task(tasks, removed.clone(), Some(*to));

        let id = removed.id;
        *task = Some(removed);
        Some(id)
      },
    }
  }

  fn undo(&mut self, tasks: &mut Vec<Task>) -> Option<Id> {
    match self {
      Self::Add { task, .. } => {
        let _ = remove_task(tasks, task.id());
        None
      },
      Self::Remove { id_or_task } => {
        let (task, idx) = id_or_task.task();
        tasks.insert(idx, task.clone());
        Some(task.id)
      },
      Self::Update { updated, before } => {
        let before = before.clone().unwrap();
        debug_assert_eq!(updated.id, before.id);
        let id = before.id;
        let _task = update_task(tasks, before);
        debug_assert_eq!(_task.id(), updated.id());
        Some(id)
      },
      Self::Move { from, task, .. } => {
        let id = task.as_ref().map(|task| task.id).unwrap();
        let idx = find_idx(tasks, id);
        let removed = tasks.remove(idx);
        tasks.insert(*from, removed);
        Some(id)
      },
    }
  }
}


pub type TaskIter<'a> = slice::Iter<'a, Task>;


/// A management struct for tasks and their associated data.
#[derive(Debug)]
pub struct Tasks {
  templates: Rc<Templates>,
  tasks: Vec<Task>,
  /// A record of operations in the order they were performed.
  operations: Ops<TaskOp, Vec<Task>, Option<Id>>,
}

impl Tasks {
  /// Create a new `Tasks` object from a serializable one.
  pub fn with_serde(tasks: SerTasks, templates: Rc<Templates>, map: &TagMap) -> Result<Self> {
    let mut new_tasks = Vec::with_capacity(tasks.0.len());
    for task in tasks.0.into_iter() {
      let task = Task::with_serde(task, templates.clone(), map)?;
      new_tasks.push(task);
    }

    Ok(Self {
      templates,
      tasks: new_tasks,
      operations: Ops::new(MAX_UNDO_STEP_COUNT),
    })
  }

  /// Create a new `Tasks` object from a serializable one without any tags.
  #[cfg(test)]
  pub fn with_serde_tasks(tasks: Vec<SerTask>) -> Result<Self> {
    // Test code using this constructor is assumed to only have tasks
    // that have no tags.
    tasks.iter().for_each(|x| assert!(x.tags.is_empty()));

    let templates = Rc::new(Templates::new());
    let map = Default::default();

    Self::with_serde(SerTasks(tasks), templates, &map)
  }

  /// Convert this object into a serializable one.
  pub fn to_serde(&self) -> SerTasks {
    // TODO: We should consider including the operations here as well.
    SerTasks(self.tasks.iter().map(ToSerde::to_serde).collect())
  }

  /// Retrieve an iterator over the tasks.
  pub fn iter(&self) -> TaskIter<'_> {
    self.tasks.iter()
  }

  /// Add a new task.
  pub fn add(&mut self, summary: String, tags: Vec<Tag>, after: Option<Id>) -> Id {
    let task = Task::with_summary_and_tags(summary, tags, self.templates.clone());
    let id = task.id;
    let op = TaskOp::add(task, after);
    self.operations.exec(op, &mut self.tasks);

    id
  }

  /// Remove a task.
  pub fn remove(&mut self, id: Id) {
    let op = TaskOp::remove(id);
    self.operations.exec(op, &mut self.tasks);
  }

  /// Update a task.
  pub fn update(&mut self, task: Task) {
    let op = TaskOp::update(task);
    self.operations.exec(op, &mut self.tasks);
  }

  /// Reorder the tasks referenced by `to_move` before `other`.
  pub fn move_before(&mut self, to_move: Id, other: Id) {
    if to_move != other {
      let idx = find_idx(&self.tasks, to_move);
      let to = Target::Before(other);
      let op = TaskOp::move_(idx, to);
      self.operations.exec(op, &mut self.tasks);
    }
  }

  /// Reorder the tasks referenced by `to_move` after `other`.
  pub fn move_after(&mut self, to_move: Id, other: Id) {
    if to_move != other {
      let idx = find_idx(&self.tasks, to_move);
      let to = Target::After(other);
      let op = TaskOp::move_(idx, to);
      self.operations.exec(op, &mut self.tasks);
    }
  }

  /// Undo the "most recent" operation.
  #[allow(clippy::option_option)]
  pub fn undo(&mut self) -> Option<Option<Id>> {
    self.operations.undo(&mut self.tasks)
  }

  /// Redo the last undone operation.
  #[allow(clippy::option_option)]
  pub fn redo(&mut self) -> Option<Option<Id>> {
    self.operations.redo(&mut self.tasks)
  }
}


#[allow(unused_results)]
#[cfg(test)]
pub mod tests {
  use super::*;

  use serde_json::from_str as from_json;
  use serde_json::to_string_pretty as to_json;

  use crate::ser::tags::Templates as SerTemplates;
  use crate::test::make_tasks;


  /// Check that the `TaskOp::Add` variant works as expected on an empty
  /// task vector.
  #[test]
  fn exec_undo_task_add_empty() {
    let mut tasks = Vec::new();
    let mut ops = Ops::new(3);

    let task1 = Task::new("task1");
    let op = TaskOp::add(task1, None);
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].summary, "task1");

    ops.undo(&mut tasks);
    assert!(tasks.is_empty());

    ops.redo(&mut tasks);
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].summary, "task1");
  }

  /// Check that the `TaskOp::Add` variant works as expected on a
  /// non-empty task vector.
  #[test]
  fn exec_undo_task_add_non_empty() {
    let mut tasks = vec![Task::new("task1")];
    let mut ops = Ops::new(3);
    let task2 = Task::new("task2");
    let op = TaskOp::add(task2, None);
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].summary, "task1");
    assert_eq!(tasks[1].summary, "task2");

    let task3 = Task::new("task3");
    let op = TaskOp::add(task3, Some(tasks[0].id));
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.len(), 3);
    assert_eq!(tasks[0].summary, "task1");
    assert_eq!(tasks[1].summary, "task3");
    assert_eq!(tasks[2].summary, "task2");

    ops.undo(&mut tasks);
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].summary, "task1");
    assert_eq!(tasks[1].summary, "task2");

    ops.undo(&mut tasks);
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].summary, "task1");
  }

  /// Check that the `TaskOp::Remove` variant works as expected on a
  /// task vector with only a single task.
  #[test]
  fn exec_undo_task_remove_single() {
    let mut tasks = vec![Task::new("task1")];
    let mut ops = Ops::new(3);

    let op = TaskOp::remove(tasks[0].id);
    ops.exec(op, &mut tasks);
    assert!(tasks.is_empty());

    ops.undo(&mut tasks);
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].summary, "task1");

    ops.redo(&mut tasks);
    assert!(tasks.is_empty());
  }

  /// Check that the `TaskOp::Remove` variant works as expected on a
  /// task vector with multiple tasks.
  #[test]
  fn exec_undo_task_remove_multi() {
    let mut tasks = vec![Task::new("task1"), Task::new("task2"), Task::new("task3")];
    let mut ops = Ops::new(3);

    let op = TaskOp::remove(tasks[1].id);
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].summary, "task1");
    assert_eq!(tasks[1].summary, "task3");

    ops.undo(&mut tasks);
    assert_eq!(tasks.len(), 3);
    assert_eq!(tasks[0].summary, "task1");
    assert_eq!(tasks[1].summary, "task2");
    assert_eq!(tasks[2].summary, "task3");

    ops.redo(&mut tasks);
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].summary, "task1");
    assert_eq!(tasks[1].summary, "task3");
  }

  /// Check that the `TaskOp::Update` variant works as expected.
  #[test]
  fn exec_undo_task_update() {
    let mut tasks = vec![Task::new("task1"), Task::new("task2")];
    let mut ops = Ops::new(3);

    let mut new = tasks[0].clone();
    new.summary = "foo!".to_string();
    let op = TaskOp::update(new);
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].summary, "foo!");
    assert_eq!(tasks[1].summary, "task2");

    ops.undo(&mut tasks);
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].summary, "task1");
    assert_eq!(tasks[1].summary, "task2");

    ops.redo(&mut tasks);
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].summary, "foo!");
    assert_eq!(tasks[1].summary, "task2");
  }

  /// Check that the `TaskOp::Update` variant works as expected when
  /// only a single task is present and the operation is no-op.
  #[test]
  fn exec_undo_task_move() {
    let mut tasks = vec![Task::new("task1"), Task::new("task2")];
    let mut ops = Ops::new(3);

    let op = TaskOp::move_(1, Target::Before(tasks[0].id));
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].summary, "task2");
    assert_eq!(tasks[1].summary, "task1");

    ops.undo(&mut tasks);
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].summary, "task1");
    assert_eq!(tasks[1].summary, "task2");

    let op = TaskOp::move_(1, Target::After(tasks[0].id));
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].summary, "task1");
    assert_eq!(tasks[1].summary, "task2");

    ops.undo(&mut tasks);
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].summary, "task1");
    assert_eq!(tasks[1].summary, "task2");
  }

  #[test]
  fn add_task() {
    let mut tasks = Tasks::with_serde_tasks(make_tasks(3)).unwrap();
    let tags = Default::default();
    tasks.add("4".to_string(), tags, None);

    assert_eq!(tasks.to_serde().0, make_tasks(4));
  }

  /// Check that adding a task after another works correctly.
  #[test]
  fn add_task_after() {
    let tasks = make_tasks(3);
    let mut tasks = Tasks::with_serde_tasks(tasks).unwrap();
    let id = tasks.tasks[0].id;
    let tags = Default::default();
    tasks.add("4".to_string(), tags, Some(id));

    let mut expected = make_tasks(4);
    let task = expected.remove(3);
    expected.insert(1, task);

    assert_eq!(tasks.to_serde().0, expected);
  }

  #[test]
  fn remove_task() {
    let mut tasks = Tasks::with_serde_tasks(make_tasks(3)).unwrap();
    let id = tasks.iter().nth(1).unwrap().id();
    tasks.remove(id);

    let mut expected = make_tasks(3);
    expected.remove(1);

    assert_eq!(tasks.to_serde().0, expected);
  }

  #[test]
  fn update_task() {
    let mut tasks = Tasks::with_serde_tasks(make_tasks(3)).unwrap();
    let mut task = tasks.iter().nth(1).unwrap().clone();
    task.summary = "amended".to_string();
    tasks.update(task);

    let mut expected = make_tasks(3);
    expected[1].summary = "amended".to_string();

    assert_eq!(tasks.to_serde().0, expected);
  }

  #[test]
  fn move_before_for_first() {
    let mut tasks = Tasks::with_serde_tasks(make_tasks(3)).unwrap();
    let id1 = tasks.iter().next().unwrap().id();
    let id2 = tasks.iter().nth(1).unwrap().id();
    tasks.move_before(id1, id2);

    let expected = make_tasks(3);
    assert_eq!(tasks.to_serde().0, expected);
  }

  #[test]
  fn move_after_for_last() {
    let mut tasks = Tasks::with_serde_tasks(make_tasks(3)).unwrap();
    let id1 = tasks.iter().nth(2).unwrap().id();
    let id2 = tasks.iter().nth(1).unwrap().id();
    tasks.move_after(id1, id2);

    let expected = make_tasks(3);
    assert_eq!(tasks.to_serde().0, expected);
  }

  #[test]
  fn move_before() {
    let mut tasks = Tasks::with_serde_tasks(make_tasks(4)).unwrap();
    let id1 = tasks.iter().nth(2).unwrap().id();
    let id2 = tasks.iter().nth(1).unwrap().id();
    tasks.move_before(id1, id2);

    let mut expected = make_tasks(4);
    expected.swap(2, 1);

    assert_eq!(tasks.to_serde().0, expected);
  }

  #[test]
  fn move_after() {
    let mut tasks = Tasks::with_serde_tasks(make_tasks(4)).unwrap();
    let id1 = tasks.iter().nth(1).unwrap().id();
    let id2 = tasks.iter().nth(2).unwrap().id();
    tasks.move_after(id1, id2);

    let mut expected = make_tasks(4);
    expected.swap(1, 2);
    assert_eq!(tasks.to_serde().0, expected);
  }

  #[test]
  fn task_completion() {
    let mut task = Task::new("test task");
    assert!(!task.is_complete());
    task.toggle_complete();
    assert!(task.is_complete());
  }

  #[test]
  fn serialize_deserialize_task() {
    let task = Task::new("this is a TODO");
    let serialized = to_json(&task.to_serde()).unwrap();
    let deserialized = from_json::<SerTask>(&serialized).unwrap();

    assert_eq!(deserialized.summary, task.summary);
  }

  #[test]
  fn serialize_deserialize_tasks() {
    let (templates, map) = Templates::with_serde(SerTemplates(Default::default()));
    let templates = Rc::new(templates);
    let tasks = Tasks::with_serde_tasks(make_tasks(3)).unwrap();
    let serialized = to_json(&tasks.to_serde()).unwrap();
    let deserialized = from_json::<SerTasks>(&serialized).unwrap();
    let tasks = Tasks::with_serde(deserialized, templates, &map).unwrap();

    assert_eq!(tasks.to_serde().0, make_tasks(3));
  }
}
