// Copyright (C) 2017-2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::PartialEq;
use std::collections::BTreeMap;
use std::io::Error;
use std::io::ErrorKind;
use std::io::Result;
use std::mem::replace;
use std::num::NonZeroUsize;
use std::rc::Rc;

use crate::db::Db;
use crate::db::Id as DbId;
use crate::db::Idable;
use crate::db::Iter as DbIter;
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

pub type Id = DbId<Task>;


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
    let id = NonZeroUsize::new(IdT::<T>::new().get()).unwrap();
    Self {
      id: DbId::from_unique_id(id),
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
    let id = NonZeroUsize::new(IdT::<T>::new().get()).unwrap();
    Task {
      id: DbId::from_unique_id(id),
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

    let id = NonZeroUsize::new(IdT::<T>::new().get()).unwrap();
    Ok(Self {
      id: DbId::from_unique_id(id),
      summary: task.summary,
      tags,
      templates,
    })
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

impl<T> Idable<T> for Task {
  fn id(&self) -> DbId<T> {
    DbId::from_unique_id(self.id.get())
  }
}

impl ToSerde<SerTask> for Task {
  /// Convert this task into a serializable one.
  fn to_serde(&self) -> SerTask {
    SerTask {
      summary: self.summary.clone(),
      tags: self.tags.values().map(|x| x.to_serde()).collect(),
    }
  }
}


/// Add a task to a vector of tasks.
fn add_task(tasks: &mut Db<Task>, task: Task, target: Option<Target>) -> Id {
  if let Some(target) = target {
    let idx = tasks.find(target.id()).unwrap();
    let idx = match target {
      Target::Before(..) => idx,
      Target::After(..) => idx + 1,
    };
    tasks.insert(idx, None, task)
  } else {
    tasks.push(None, task)
  }
}

/// Remove a task from a vector of tasks.
fn remove_task(tasks: &mut Db<Task>, id: Id) -> (Task, usize) {
  let idx = tasks.find(id).unwrap();
  let (_, task) = tasks.remove(idx);
  (task, idx)
}

/// Update a task in a vector of tasks.
fn update_task(tasks: &mut Db<Task>, id: Id, task: Task) -> Task {
  let idx = tasks.find(id).unwrap();
  replace(&mut tasks.get_mut(idx).unwrap(), task)
}


/// An enumeration holding either a task ID or the full task.
///
/// This type is mostly an implementation detail of the `TaskOp`
/// enumeration.
#[derive(Debug)]
enum IdOrTask {
  /// The ID of a task.
  Id(Id),
  /// An actual task along with its ID and position.
  Task { task: (Id, Task), position: usize },
}

impl IdOrTask {
  fn id(&self) -> Id {
    match self {
      Self::Id(id) => *id,
      Self::Task { task, .. } => task.0,
    }
  }

  fn task(&self) -> (Id, &Task, usize) {
    match self {
      Self::Id(..) => panic!("IdOrTask does not contain a task"),
      Self::Task { task, position } => (task.0, &task.1, *position),
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
  Add { task: (Id, Task), after: Option<Id> },
  /// An operation removing a task.
  Remove { id_or_task: IdOrTask },
  /// An operation updating a task.
  Update {
    updated: (Id, Task),
    before: Option<(Id, Task)>,
  },
  /// An operation changing a task's position.
  Move {
    from: usize,
    to: Target,
    id: Option<Id>,
  },
}

impl TaskOp {
  fn add(id: Id, task: Task, after: Option<Id>) -> Self {
    Self::Add {
      task: (id, task),
      after,
    }
  }

  fn remove(id: Id) -> Self {
    Self::Remove {
      id_or_task: IdOrTask::Id(id),
    }
  }

  fn update(id: Id, updated: Task) -> Self {
    Self::Update {
      updated: (id, updated),
      before: None,
    }
  }

  fn move_(from: usize, to: Target) -> Self {
    Self::Move { from, to, id: None }
  }
}

impl Op<Db<Task>, Option<Id>> for TaskOp {
  fn exec(&mut self, tasks: &mut Db<Task>) -> Option<Id> {
    match self {
      Self::Add { task, after } => {
        let id = add_task(tasks, task.1.clone(), after.map(Target::After));
        Some(id)
      },
      Self::Remove { id_or_task } => {
        let id = id_or_task.id();
        let (task, idx) = remove_task(tasks, id);
        *id_or_task = IdOrTask::Task {
          task: (id, task),
          position: idx,
        };
        None
      },
      Self::Update { updated, before } => {
        let id = updated.0;
        let task = update_task(tasks, id, updated.1.clone());
        *before = Some((id, task));
        Some(id)
      },
      Self::Move { from, to, id } => {
        let removed = tasks.remove(*from);
        // We do not support the case of moving a task with itself as a
        // target. Doing so should be prevented at a higher layer,
        // though.
        debug_assert_ne!(removed.0, to.id());
        add_task(tasks, removed.1.clone(), Some(*to));

        *id = Some(removed.0);
        Some(removed.0)
      },
    }
  }

  fn undo(&mut self, tasks: &mut Db<Task>) -> Option<Id> {
    match self {
      Self::Add { task, .. } => {
        let _ = remove_task(tasks, task.1.id());
        None
      },
      Self::Remove { id_or_task } => {
        let (id, task, idx) = id_or_task.task();
        tasks.insert(idx, Some(id), task.clone());
        Some(id)
      },
      Self::Update { updated, before } => {
        let before = before.clone().unwrap();
        debug_assert_eq!(updated.0, before.0);
        let id = before.1.id;
        let _task = update_task(tasks, id, before.1);
        Some(id)
      },
      Self::Move { from, id, .. } => {
        let id = id.unwrap();
        let idx = tasks.find(id).unwrap();
        let removed = tasks.remove(idx);
        tasks.insert(*from, Some(removed.0), removed.1);
        Some(id)
      },
    }
  }
}


pub type TaskIter<'a> = DbIter<'a, (Id, Task)>;


/// A management struct for tasks and their associated data.
#[derive(Debug)]
pub struct Tasks {
  templates: Rc<Templates>,
  /// The managed tasks.
  tasks: Db<Task>,
  /// A record of operations in the order they were performed.
  operations: Ops<TaskOp, Db<Task>, Option<Id>>,
}

impl Tasks {
  /// Create a new `Tasks` object from a serializable one.
  pub fn with_serde(tasks: SerTasks, templates: Rc<Templates>, map: &TagMap) -> Result<Self> {
    let len = tasks.0.len();
    let tasks = tasks
      .0
      .into_iter()
      .try_fold(Vec::with_capacity(len), |mut vec, task| {
        let task = Task::with_serde(task, templates.clone(), map)?;
        vec.push(task);
        Result::Ok(vec)
      })?;
    let tasks = Db::try_from_iter(tasks).map_err(|id| {
      let error = format!("Encountered duplicate task ID {}", id);
      Error::new(ErrorKind::InvalidInput, error)
    })?;

    Ok(Self {
      templates,
      tasks,
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
    SerTasks(self.tasks.iter().map(|(_, task)| task.to_serde()).collect())
  }

  /// Retrieve an iterator over the tasks.
  pub fn iter(&self) -> TaskIter<'_> {
    self.tasks.iter()
  }

  /// Add a new task.
  pub fn add(&mut self, summary: String, tags: Vec<Tag>, after: Option<Id>) -> Id {
    let task = Task::with_summary_and_tags(summary, tags, self.templates.clone());
    let id = task.id;
    let op = TaskOp::add(id, task, after);
    self.operations.exec(op, &mut self.tasks);

    id
  }

  /// Remove a task.
  pub fn remove(&mut self, id: Id) {
    let op = TaskOp::remove(id);
    self.operations.exec(op, &mut self.tasks);
  }

  /// Update a task.
  pub fn update(&mut self, id: Id, task: Task) {
    let op = TaskOp::update(id, task);
    self.operations.exec(op, &mut self.tasks);
  }

  /// Reorder the tasks referenced by `to_move` before `other`.
  pub fn move_before(&mut self, to_move: Id, other: Id) {
    if to_move != other {
      let idx = self.tasks.find(to_move).unwrap();
      let to = Target::Before(other);
      let op = TaskOp::move_(idx, to);
      self.operations.exec(op, &mut self.tasks);
    }
  }

  /// Reorder the tasks referenced by `to_move` after `other`.
  pub fn move_after(&mut self, to_move: Id, other: Id) {
    if to_move != other {
      let idx = self.tasks.find(to_move).unwrap();
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
    let mut tasks = Db::try_from_iter([]).unwrap();
    let mut ops = Ops::new(3);

    let task1 = Task::new("task1");
    let op = TaskOp::add(task1.id, task1, None);
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.iter().len(), 1);
    assert_eq!(tasks.get(0).unwrap().summary, "task1");

    ops.undo(&mut tasks);
    assert_eq!(tasks.iter().len(), 0);

    ops.redo(&mut tasks);
    assert_eq!(tasks.iter().len(), 1);
    assert_eq!(tasks.get(0).unwrap().summary, "task1");
  }

  /// Check that the `TaskOp::Add` variant works as expected on a
  /// non-empty task vector.
  #[test]
  fn exec_undo_task_add_non_empty() {
    let mut tasks = Db::try_from_iter([Task::new("task1")]).unwrap();
    let mut ops = Ops::new(3);
    let task2 = Task::new("task2");
    let op = TaskOp::add(task2.id, task2, None);
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.iter().len(), 2);
    assert_eq!(tasks.get(0).unwrap().summary, "task1");
    assert_eq!(tasks.get(1).unwrap().summary, "task2");

    let task3 = Task::new("task3");
    let op = TaskOp::add(task3.id, task3, Some(tasks.get(0).unwrap().id()));
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.iter().len(), 3);
    assert_eq!(tasks.get(0).unwrap().summary, "task1");
    assert_eq!(tasks.get(1).unwrap().summary, "task3");
    assert_eq!(tasks.get(2).unwrap().summary, "task2");

    ops.undo(&mut tasks);
    assert_eq!(tasks.iter().len(), 2);
    assert_eq!(tasks.get(0).unwrap().summary, "task1");
    assert_eq!(tasks.get(1).unwrap().summary, "task2");

    ops.undo(&mut tasks);
    assert_eq!(tasks.iter().len(), 1);
    assert_eq!(tasks.get(0).unwrap().summary, "task1");
  }

  /// Check that the `TaskOp::Remove` variant works as expected on a
  /// task vector with only a single task.
  #[test]
  fn exec_undo_task_remove_single() {
    let mut tasks = Db::try_from_iter([Task::new("task1")]).unwrap();
    let mut ops = Ops::new(3);

    let op = TaskOp::remove(tasks.get(0).unwrap().id());
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.iter().len(), 0);

    ops.undo(&mut tasks);
    assert_eq!(tasks.iter().len(), 1);
    assert_eq!(tasks.get(0).unwrap().summary, "task1");

    ops.redo(&mut tasks);
    assert_eq!(tasks.iter().len(), 0);
  }

  /// Check that the `TaskOp::Remove` variant works as expected on a
  /// task vector with multiple tasks.
  #[test]
  fn exec_undo_task_remove_multi() {
    let mut tasks =
      Db::try_from_iter([Task::new("task1"), Task::new("task2"), Task::new("task3")]).unwrap();
    let mut ops = Ops::new(3);

    let op = TaskOp::remove(tasks.get(1).unwrap().id());
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.iter().len(), 2);
    assert_eq!(tasks.get(0).unwrap().summary, "task1");
    assert_eq!(tasks.get(1).unwrap().summary, "task3");

    ops.undo(&mut tasks);
    assert_eq!(tasks.iter().len(), 3);
    assert_eq!(tasks.get(0).unwrap().summary, "task1");
    assert_eq!(tasks.get(1).unwrap().summary, "task2");
    assert_eq!(tasks.get(2).unwrap().summary, "task3");

    ops.redo(&mut tasks);
    assert_eq!(tasks.iter().len(), 2);
    assert_eq!(tasks.get(0).unwrap().summary, "task1");
    assert_eq!(tasks.get(1).unwrap().summary, "task3");
  }

  /// Check that the `TaskOp::Update` variant works as expected.
  #[test]
  fn exec_undo_task_update() {
    let mut tasks = Db::try_from_iter([Task::new("task1"), Task::new("task2")]).unwrap();
    let mut ops = Ops::new(3);

    let mut new = tasks.get(0).unwrap().clone();
    new.summary = "foo!".to_string();
    let op = TaskOp::update(new.id, new);
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.iter().len(), 2);
    assert_eq!(tasks.get(0).unwrap().summary, "foo!");
    assert_eq!(tasks.get(1).unwrap().summary, "task2");

    ops.undo(&mut tasks);
    assert_eq!(tasks.iter().len(), 2);
    assert_eq!(tasks.get(0).unwrap().summary, "task1");
    assert_eq!(tasks.get(1).unwrap().summary, "task2");

    ops.redo(&mut tasks);
    assert_eq!(tasks.iter().len(), 2);
    assert_eq!(tasks.get(0).unwrap().summary, "foo!");
    assert_eq!(tasks.get(1).unwrap().summary, "task2");
  }

  /// Check that the `TaskOp::Update` variant works as expected when
  /// only a single task is present and the operation is no-op.
  #[test]
  fn exec_undo_task_move() {
    let mut tasks = Db::try_from_iter([Task::new("task1"), Task::new("task2")]).unwrap();
    let mut ops = Ops::new(3);

    let op = TaskOp::move_(1, Target::Before(tasks.get(0).unwrap().id()));
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.iter().len(), 2);
    assert_eq!(tasks.get(0).unwrap().summary, "task2");
    assert_eq!(tasks.get(1).unwrap().summary, "task1");

    ops.undo(&mut tasks);
    assert_eq!(tasks.iter().len(), 2);
    assert_eq!(tasks.get(0).unwrap().summary, "task1");
    assert_eq!(tasks.get(1).unwrap().summary, "task2");

    let op = TaskOp::move_(1, Target::After(tasks.get(0).unwrap().id()));
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.iter().len(), 2);
    assert_eq!(tasks.get(0).unwrap().summary, "task1");
    assert_eq!(tasks.get(1).unwrap().summary, "task2");

    ops.undo(&mut tasks);
    assert_eq!(tasks.iter().len(), 2);
    assert_eq!(tasks.get(0).unwrap().summary, "task1");
    assert_eq!(tasks.get(1).unwrap().summary, "task2");
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
    let id = tasks.tasks.get(0).unwrap().id();
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
    let id = tasks.iter().nth(1).unwrap().0;
    tasks.remove(id);

    let mut expected = make_tasks(3);
    expected.remove(1);

    assert_eq!(tasks.to_serde().0, expected);
  }

  #[test]
  fn update_task() {
    let mut tasks = Tasks::with_serde_tasks(make_tasks(3)).unwrap();
    let (id, mut task) = tasks.iter().nth(1).unwrap().clone();
    task.summary = "amended".to_string();
    tasks.update(id, task);

    let mut expected = make_tasks(3);
    expected[1].summary = "amended".to_string();

    assert_eq!(tasks.to_serde().0, expected);
  }

  #[test]
  fn move_before_for_first() {
    let mut tasks = Tasks::with_serde_tasks(make_tasks(3)).unwrap();
    let id1 = tasks.iter().next().unwrap().0;
    let id2 = tasks.iter().nth(1).unwrap().0;
    tasks.move_before(id1, id2);

    let expected = make_tasks(3);
    assert_eq!(tasks.to_serde().0, expected);
  }

  #[test]
  fn move_after_for_last() {
    let mut tasks = Tasks::with_serde_tasks(make_tasks(3)).unwrap();
    let id1 = tasks.iter().nth(2).unwrap().0;
    let id2 = tasks.iter().nth(1).unwrap().0;
    tasks.move_after(id1, id2);

    let expected = make_tasks(3);
    assert_eq!(tasks.to_serde().0, expected);
  }

  #[test]
  fn move_before() {
    let mut tasks = Tasks::with_serde_tasks(make_tasks(4)).unwrap();
    let id1 = tasks.iter().nth(2).unwrap().0;
    let id2 = tasks.iter().nth(1).unwrap().0;
    tasks.move_before(id1, id2);

    let mut expected = make_tasks(4);
    expected.swap(2, 1);

    assert_eq!(tasks.to_serde().0, expected);
  }

  #[test]
  fn move_after() {
    let mut tasks = Tasks::with_serde_tasks(make_tasks(4)).unwrap();
    let id1 = tasks.iter().nth(1).unwrap().0;
    let id2 = tasks.iter().nth(2).unwrap().0;
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
