// Copyright (C) 2017-2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::RefCell;
use std::collections::btree_set::Iter as BTreeSetIter;
use std::collections::BTreeSet;
use std::ops::Deref as _;
use std::ops::DerefMut as _;
use std::rc::Rc;

use anyhow::anyhow;
use anyhow::Result;

use uuid::Uuid;

use crate::db::Db;
use crate::db::Iter as DbIter;
use crate::ops::Op;
use crate::ops::Ops;
use crate::position::Position;
use crate::ser::tasks::Task as SerTask;
use crate::ser::tasks::Tasks as SerTasks;
use crate::ser::ToSerde;
use crate::tags::Tag;
use crate::tags::Templates;


/// The maximum number of undo steps that we keep record of.
// TODO: We may consider making this value user-configurable.
const MAX_UNDO_STEP_COUNT: usize = 64;


type Id = Uuid;


#[derive(Clone, Debug)]
struct TaskInner {
  /// The task's ID.
  id: Id,
  /// The task's summary.
  summary: String,
  /// The task's tags.
  tags: BTreeSet<Tag>,
  /// Reference to the shared `Templates` object from which tags were
  /// instantiated.
  templates: Rc<Templates>,
}


/// A struct representing a task item.
// Note that while conceptually this type could be fully internally
// mutable, in practice most modifying functions still have a &mut self
// receiver. The reason is that we want to force task update (the update
// of the entity in the `Tasks` object) to go through [`Tasks::update`],
// in order to hook into our `Ops` infrastructure and make changes
// reversible. That's enabled through [`Task::update_from`], which makes
// use of internal mutability, to work on a shared reference as stored
// inside `Tasks`.
#[derive(Clone, Debug)]
pub struct Task(RefCell<TaskInner>);

impl Task {
  /// Create a new task.
  #[cfg(test)]
  pub fn new(summary: impl Into<String>) -> Self {
    let inner = TaskInner {
      id: Id::new_v4(),
      summary: summary.into(),
      tags: Default::default(),
      templates: Rc::new(Templates::new()),
    };

    Self(RefCell::new(inner))
  }

  /// Create a task using the given summary.
  pub fn with_summary_and_tags<S>(summary: S, tags: Vec<Tag>, templates: Rc<Templates>) -> Self
  where
    S: Into<String>,
  {
    let inner = TaskInner {
      id: Id::new_v4(),
      summary: summary.into(),
      tags: tags.into_iter().collect(),
      templates,
    };

    Self(RefCell::new(inner))
  }

  /// Create a new task from a serializable one.
  fn with_serde(task: SerTask, templates: Rc<Templates>) -> Result<Self> {
    let mut tags = BTreeSet::new();
    for tag in task.tags.into_iter() {
      let tag = templates
        .instantiate(tag.id)
        .ok_or_else(|| anyhow!("encountered invalid tag ID {}", tag.id))?;
      tags.insert(tag);
    }

    let inner = TaskInner {
      id: task.id,
      summary: task.summary,
      tags,
      templates,
    };
    Ok(Self(RefCell::new(inner)))
  }

  /// Retrieve the [`Task`]'s ID.
  #[cfg(test)]
  #[inline]
  pub fn id(&self) -> Id {
    // SANITY: The type's API surface prevents any borrows from escaping
    //         a function call and we don't call methods on `self` while
    //         a borrow is active.
    self.0.try_borrow().unwrap().id
  }

  /// Retrieve the [`Task`]'s summary.
  #[inline]
  pub fn summary(&self) -> String {
    // SANITY: The type's API surface prevents any borrows from escaping
    //         a function call and we don't call methods on `self` while
    //         a borrow is active.
    self.0.try_borrow().unwrap().summary.clone()
  }

  /// Change this [Task]'s summary.
  #[inline]
  pub fn set_summary(&mut self, summary: String) {
    // SANITY: The type's API surface prevents any borrows from escaping
    //         a function call and we don't call methods on `self` while
    //         a borrow is active.
    self.0.try_borrow_mut().unwrap().summary = summary
  }

  /// Invoke a user-provided function on an iterator over all the task's
  /// tags.
  #[inline]
  pub fn tags<F, R>(&self, mut f: F) -> R
  where
    F: FnMut(BTreeSetIter<'_, Tag>) -> R,
  {
    // SANITY: The type's API surface prevents any borrows from escaping
    //         a function call and we don't call methods on `self` while
    //         a borrow is active.
    f(self.0.try_borrow().unwrap().tags.iter())
  }

  /// Set the tags of the task.
  pub fn set_tags<I>(&mut self, tags: I)
  where
    I: Iterator<Item = Tag>,
  {
    // SANITY: The type's API surface prevents any borrows from escaping
    //         a function call and we don't call methods on `self` while
    //         a borrow is active.
    self.0.try_borrow_mut().unwrap().tags = tags.collect();
  }

  /// Check whether the task has the provided `tag` set.
  #[inline]
  pub fn has_tag(&self, tag: &Tag) -> bool {
    // SANITY: The type's API surface prevents any borrows from escaping
    //         a function call and we don't call methods on `self` while
    //         a borrow is active.
    self.0.try_borrow().unwrap().tags.get(tag).is_some()
  }

  /// Ensure that the provided tag is set on this task.
  #[inline]
  pub fn set_tag(&mut self, tag: Tag) -> bool {
    // SANITY: The type's API surface prevents any borrows from escaping
    //         a function call and we don't call methods on `self` while
    //         a borrow is active.
    self.0.try_borrow_mut().unwrap().tags.insert(tag)
  }

  /// Ensure that the provided tag is not set on this task.
  #[inline]
  pub fn unset_tag(&mut self, tag: &Tag) -> bool {
    // SANITY: The type's API surface prevents any borrows from escaping
    //         a function call and we don't call methods on `self` while
    //         a borrow is active.
    self.0.try_borrow_mut().unwrap().tags.remove(tag)
  }

  /// Update this task with the contents of `other`.
  fn update_from(&self, other: Task) {
    // SANITY: The type's API surface prevents any borrows from escaping
    //         a function call and we don't call methods on `self` while
    //         a borrow is active.
    let mut borrow = self.0.try_borrow_mut().unwrap();
    *borrow.deref_mut() = other.0.into_inner();
  }

  /// Retrieve the `Templates` object associated with this task.
  pub fn templates(&self) -> Rc<Templates> {
    self.0.try_borrow().unwrap().templates.clone()
  }
}

impl ToSerde<SerTask> for Task {
  /// Convert this task into a serializable one.
  fn to_serde(&self) -> SerTask {
    let borrow = self.0.try_borrow().unwrap();
    let TaskInner {
      ref id,
      ref summary,
      ref tags,
      ..
    } = borrow.deref();

    let task = SerTask {
      id: *id,
      summary: summary.clone(),
      tags: tags.iter().map(Tag::to_serde).collect(),
    };

    task
  }
}


/// Add a task to a vector of tasks.
fn add_task(tasks: &mut Db<Task, Position>, task: Rc<Task>, target: Option<Target>) -> Rc<Task> {
  let _entry = if let Some(target) = target {
    let idx = tasks.find(target.task()).unwrap();
    let idx = match target {
      Target::Before(..) => idx,
      Target::After(..) => idx + 1,
    };
    tasks.try_insert(idx, task.clone()).unwrap()
  } else {
    tasks.try_push(task.clone()).unwrap()
  };

  task
}

/// Remove a task from a vector of tasks.
fn remove_task(tasks: &mut Db<Task, Position>, task: &Rc<Task>) -> (Rc<Task>, usize) {
  let idx = tasks.find(task).unwrap();
  let (task, _aux) = tasks.remove(idx);
  (task, idx)
}

/// Update a task in a vector of tasks.
fn update_task(task: &Rc<Task>, other: Task) -> Task {
  // Make a deep copy of the task.
  let before = task.deref().deref().clone();
  let () = task.update_from(other);
  before
}


/// An enum encoding the target location of a task: before or after a
/// task with a given ID.
#[derive(Clone, Debug)]
enum Target {
  /// The target is the spot before the given task.
  Before(Rc<Task>),
  /// The target is the spot after the given task.
  After(Rc<Task>),
}

impl Target {
  fn task(&self) -> &Rc<Task> {
    match self {
      Self::Before(task) | Self::After(task) => task,
    }
  }
}


/// An operation to be performed on a task in a `Tasks` object.
#[derive(Debug)]
enum TaskOp {
  /// An operation adding a task.
  Add {
    task: Rc<Task>,
    after: Option<Rc<Task>>,
  },
  /// An operation removing a task.
  Remove {
    task: Rc<Task>,
    position: Option<usize>,
  },
  /// An operation updating a task.
  Update {
    updated: (Rc<Task>, Task),
    before: Option<Task>,
  },
  /// An operation changing a task's position.
  Move {
    task: Rc<Task>,
    to: Target,
    position: Option<usize>,
  },
}

impl TaskOp {
  fn add(task: Rc<Task>, after: Option<Rc<Task>>) -> Self {
    Self::Add { task, after }
  }

  fn remove(task: Rc<Task>) -> Self {
    Self::Remove {
      task,
      position: None,
    }
  }

  fn update(task: Rc<Task>, updated: Task) -> Self {
    Self::Update {
      updated: (task, updated),
      before: None,
    }
  }

  fn move_(task: Rc<Task>, to: Target) -> Self {
    Self::Move {
      task,
      to,
      position: None,
    }
  }
}

impl Op<Db<Task, Position>, Option<Rc<Task>>> for TaskOp {
  fn exec(&mut self, tasks: &mut Db<Task, Position>) -> Option<Rc<Task>> {
    match self {
      Self::Add {
        ref mut task,
        after,
      } => {
        let added = add_task(tasks, task.clone(), after.clone().map(Target::After));
        Some(added)
      },
      Self::Remove { task, position } => {
        let (_task, idx) = remove_task(tasks, task);
        *position = Some(idx);
        None
      },
      Self::Update { updated, before } => {
        let task = &updated.0;
        let _task = update_task(task, updated.1.clone());
        *before = Some(_task);
        Some(task.clone())
      },
      Self::Move { task, to, position } => {
        // SANITY: The task really should be in our `Tasks` object or we
        //         are in trouble.
        let idx = tasks.find(task).unwrap();
        let (removed, _aux) = tasks.remove(idx);
        // We do not support the case of moving a task with itself as a
        // target. Doing so should be prevented at a higher layer,
        // though.
        debug_assert!(!Rc::ptr_eq(&removed, to.task()));
        *position = Some(idx);

        let task = add_task(tasks, removed, Some(to.clone()));
        Some(task)
      },
    }
  }

  fn undo(&mut self, tasks: &mut Db<Task, Position>) -> Option<Rc<Task>> {
    match self {
      Self::Add { task, .. } => {
        let (_task, _idx) = remove_task(tasks, task);
        None
      },
      Self::Remove { task, position } => {
        // SANITY: The position will always be set at this point.
        let idx = position.unwrap();
        // SANITY: The task had been removed earlier, so it is not
        //         currently present.
        tasks.try_insert(idx, task.clone()).unwrap();
        Some(task.clone())
      },
      Self::Update { updated, before } => {
        // SANITY: `before` is guaranteed to be set on this path.
        let before = before.clone().unwrap();
        let task = &updated.0;
        let _task = update_task(task, before);
        let idx = tasks.find(task).unwrap();
        let task = tasks.get(idx).unwrap();
        Some(task.deref().clone())
      },
      Self::Move { task, position, .. } => {
        // SANITY: `position` is guaranteed to be set on this path.
        let position = position.unwrap();
        let idx = tasks.find(task).unwrap();
        let (removed, _aux) = tasks.remove(idx);
        // SANITY: We just removed the task, so it can't be present.
        let _entry = tasks.try_insert(position, removed.clone()).unwrap();
        Some(removed)
      },
    }
  }
}


/// An iterator over tasks.
pub type TaskIter<'tasks> = DbIter<'tasks, Task, Position>;


#[derive(Debug)]
struct TasksInner {
  templates: Rc<Templates>,
  /// The managed tasks.
  tasks: Db<Task, Position>,
  /// A record of operations in the order they were performed.
  operations: Ops<TaskOp, Db<Task, Position>, Option<Rc<Task>>>,
}


/// A management struct for tasks and their associated data.
#[derive(Debug)]
pub struct Tasks(RefCell<TasksInner>);

impl Tasks {
  /// Create a new `Tasks` object from a serializable one.
  pub fn with_serde(tasks: SerTasks, templates: Rc<Templates>) -> Result<Self> {
    let len = tasks.0.len();
    let tasks = tasks.0.into_iter().enumerate().try_fold(
      Vec::with_capacity(len),
      |mut vec, (idx, task)| -> Result<_> {
        let task = Task::with_serde(task, templates.clone())?;
        let position = Position::from_int(idx);
        vec.push((task, position));
        Result::Ok(vec)
      },
    )?;
    let tasks = Db::from_iter_with_aux(tasks);

    let inner = TasksInner {
      templates,
      tasks,
      operations: Ops::new(MAX_UNDO_STEP_COUNT),
    };

    Ok(Self(RefCell::new(inner)))
  }

  /// Create a new `Tasks` object from a serializable one without any tags.
  #[cfg(test)]
  pub fn with_serde_tasks(tasks: Vec<SerTask>) -> Result<Self> {
    // Test code using this constructor is assumed to only have tasks
    // that have no tags.
    tasks.iter().for_each(|x| assert!(x.tags.is_empty()));

    let tasks = SerTasks::from(tasks);
    let templates = Rc::new(Templates::new());

    Self::with_serde(tasks, templates)
  }

  /// Convert this object into a serializable one.
  pub fn to_serde(&self) -> SerTasks {
    // SANITY: The type's API surface prevents any borrows from escaping
    //         a function call and we don't call methods on `self` while
    //         a borrow is active.
    let tasks = self
      .0
      .try_borrow()
      .unwrap()
      .tasks
      .iter()
      .map(|task| task.to_serde())
      .collect();

    // TODO: We should consider including the operations here as well.
    SerTasks(tasks)
  }

  /// Invoke a user-provided function on an iterator over all tasks.
  #[inline]
  pub fn iter<F, R>(&self, mut f: F) -> R
  where
    F: FnMut(TaskIter<'_>) -> R,
  {
    // SANITY: The type's API surface prevents any borrows from escaping
    //         a function call and we don't call methods on `self` while
    //         a borrow is active.
    f(self.0.try_borrow().unwrap().tasks.iter())
  }

  /// Add a new task.
  pub fn add(&self, summary: String, tags: Vec<Tag>, after: Option<Rc<Task>>) -> Rc<Task> {
    // SANITY: The type's API surface prevents any borrows from escaping
    //         a function call and we don't call methods on `self` while
    //         a borrow is active.
    let mut borrow = self.0.try_borrow_mut().unwrap();
    let TasksInner {
      ref mut templates,
      ref mut operations,
      ref mut tasks,
      ..
    } = borrow.deref_mut();

    let task = Rc::new(Task::with_summary_and_tags(
      summary,
      tags,
      templates.clone(),
    ));
    let op = TaskOp::add(task, after);
    // SANITY: We know that an "add" operation always returns a task, so
    //         this unwrap will never panic.
    let task = operations.exec(op, tasks).unwrap();

    task
  }

  /// Remove a task.
  pub fn remove(&self, task: Rc<Task>) {
    // SANITY: The type's API surface prevents any borrows from escaping
    //         a function call and we don't call methods on `self` while
    //         a borrow is active.
    let mut borrow = self.0.try_borrow_mut().unwrap();
    let TasksInner {
      ref mut operations,
      ref mut tasks,
      ..
    } = borrow.deref_mut();

    let op = TaskOp::remove(task);
    operations.exec(op, tasks);
  }

  /// Update a task.
  pub fn update(&self, task: Rc<Task>, updated: Task) {
    // SANITY: The type's API surface prevents any borrows from escaping
    //         a function call and we don't call methods on `self` while
    //         a borrow is active.
    let mut borrow = self.0.try_borrow_mut().unwrap();
    let TasksInner {
      ref mut operations,
      ref mut tasks,
      ..
    } = borrow.deref_mut();

    let op = TaskOp::update(task, updated);
    operations.exec(op, tasks);
  }

  /// Reorder the task referenced by `to_move` before `other`.
  pub fn move_before(&self, to_move: Rc<Task>, other: Rc<Task>) {
    if !Rc::ptr_eq(&to_move, &other) {
      // SANITY: The type's API surface prevents any borrows from escaping
      //         a function call and we don't call methods on `self` while
      //         a borrow is active.
      let mut borrow = self.0.try_borrow_mut().unwrap();
      let TasksInner {
        ref mut operations,
        ref mut tasks,
        ..
      } = borrow.deref_mut();

      let to = Target::Before(other);
      let op = TaskOp::move_(to_move, to);
      operations.exec(op, tasks);
    }
  }

  /// Reorder the tasks referenced by `to_move` after `other`.
  pub fn move_after(&self, to_move: Rc<Task>, other: Rc<Task>) {
    if !Rc::ptr_eq(&to_move, &other) {
      // SANITY: The type's API surface prevents any borrows from escaping
      //         a function call and we don't call methods on `self` while
      //         a borrow is active.
      let mut borrow = self.0.try_borrow_mut().unwrap();
      let TasksInner {
        ref mut operations,
        ref mut tasks,
        ..
      } = borrow.deref_mut();

      let to = Target::After(other);
      let op = TaskOp::move_(to_move, to);
      operations.exec(op, tasks);
    }
  }

  /// Undo the "most recent" operation.
  pub fn undo(&self) -> Option<Option<Rc<Task>>> {
    // SANITY: The type's API surface prevents any borrows from escaping
    //         a function call and we don't call methods on `self` while
    //         a borrow is active.
    let mut borrow = self.0.try_borrow_mut().unwrap();
    let TasksInner {
      ref mut operations,
      ref mut tasks,
      ..
    } = borrow.deref_mut();

    operations.undo(tasks)
  }

  /// Redo the last undone operation.
  pub fn redo(&self) -> Option<Option<Rc<Task>>> {
    // SANITY: The type's API surface prevents any borrows from escaping
    //         a function call and we don't call methods on `self` while
    //         a borrow is active.
    let mut borrow = self.0.try_borrow_mut().unwrap();
    let TasksInner {
      ref mut operations,
      ref mut tasks,
      ..
    } = borrow.deref_mut();

    operations.redo(tasks)
  }
}


#[cfg(test)]
pub mod tests {
  use super::*;

  use std::num::NonZeroUsize;

  use crate::ser::tags::Id as SerTemplateId;
  use crate::ser::tags::Template as SerTemplate;
  use crate::ser::tags::Templates as SerTemplates;
  use crate::test::make_tasks;
  use crate::test::COMPLETE_TAG;


  /// Check that we can query and set/unset tags on a task.
  #[test]
  fn task_tag_query_and_adjustment() {
    let templates = vec![SerTemplate {
      id: SerTemplateId::new(NonZeroUsize::new(42).unwrap()),
      name: COMPLETE_TAG.to_string(),
    }];
    let templates = Templates::with_serde(SerTemplates(templates)).unwrap();
    let complete = templates.instantiate_from_name(COMPLETE_TAG);

    let mut task = Task::new("test task");
    assert!(!task.has_tag(&complete));

    assert!(task.set_tag(complete.clone()));
    assert!(task.has_tag(&complete));
    assert!(!task.set_tag(complete.clone()));
    assert!(task.has_tag(&complete));

    assert!(task.unset_tag(&complete));
    assert!(!task.has_tag(&complete));
    assert!(!task.unset_tag(&complete));
    assert!(!task.has_tag(&complete));
  }

  /// Check that the `TaskOp::Add` variant works as expected on an empty
  /// task vector.
  #[test]
  fn exec_undo_task_add_empty() {
    let mut tasks = Db::from_iter_with_aux([]);
    let mut ops = Ops::new(3);

    let task1 = Rc::new(Task::new("task1"));
    let op = TaskOp::add(task1, None);
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.iter().len(), 1);
    assert_eq!(tasks.get(0).unwrap().summary(), "task1");

    ops.undo(&mut tasks);
    assert_eq!(tasks.iter().len(), 0);

    ops.redo(&mut tasks);
    assert_eq!(tasks.iter().len(), 1);
    assert_eq!(tasks.get(0).unwrap().summary(), "task1");
  }

  /// Check that the `TaskOp::Add` variant works as expected on a
  /// non-empty task vector.
  #[test]
  fn exec_undo_task_add_non_empty() {
    let iter = [Task::new("task1")]
      .into_iter()
      .enumerate()
      .map(|(idx, task)| (task, Position::from_int(idx)));
    let mut tasks = Db::from_iter_with_aux(iter);
    let mut ops = Ops::new(3);
    let task2 = Rc::new(Task::new("task2"));
    let op = TaskOp::add(task2, None);
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.iter().len(), 2);
    assert_eq!(tasks.get(0).unwrap().summary(), "task1");
    assert_eq!(tasks.get(1).unwrap().summary(), "task2");

    let task3 = Rc::new(Task::new("task3"));
    let after = tasks.get(0).unwrap().deref().clone();
    let op = TaskOp::add(task3, Some(after));
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.iter().len(), 3);
    assert_eq!(tasks.get(0).unwrap().summary(), "task1");
    assert_eq!(tasks.get(1).unwrap().summary(), "task3");
    assert_eq!(tasks.get(2).unwrap().summary(), "task2");

    ops.undo(&mut tasks);
    assert_eq!(tasks.iter().len(), 2);
    assert_eq!(tasks.get(0).unwrap().summary(), "task1");
    assert_eq!(tasks.get(1).unwrap().summary(), "task2");

    ops.undo(&mut tasks);
    assert_eq!(tasks.iter().len(), 1);
    assert_eq!(tasks.get(0).unwrap().summary(), "task1");
  }

  /// Check that the `TaskOp::Remove` variant works as expected on a
  /// task vector with only a single task.
  #[test]
  fn exec_undo_task_remove_single() {
    let iter = [Task::new("task1")]
      .into_iter()
      .enumerate()
      .map(|(idx, task)| (task, Position::from_int(idx)));
    let mut tasks = Db::from_iter_with_aux(iter);
    let mut ops = Ops::new(3);

    let task = tasks.get(0).unwrap().deref().clone();
    let op = TaskOp::remove(task);
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.iter().len(), 0);

    ops.undo(&mut tasks);
    assert_eq!(tasks.iter().len(), 1);
    assert_eq!(tasks.get(0).unwrap().summary(), "task1");

    ops.redo(&mut tasks);
    assert_eq!(tasks.iter().len(), 0);
  }

  /// Check that the `TaskOp::Remove` variant works as expected on a
  /// task vector with multiple tasks.
  #[test]
  fn exec_undo_task_remove_multi() {
    let iter = [Task::new("task1"), Task::new("task2"), Task::new("task3")]
      .into_iter()
      .enumerate()
      .map(|(idx, task)| (task, Position::from_int(idx)));
    let mut tasks = Db::from_iter_with_aux(iter);
    let mut ops = Ops::new(3);

    let task = tasks.get(1).unwrap().deref().clone();
    let op = TaskOp::remove(task);
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.iter().len(), 2);
    assert_eq!(tasks.get(0).unwrap().summary(), "task1");
    assert_eq!(tasks.get(1).unwrap().summary(), "task3");

    ops.undo(&mut tasks);
    assert_eq!(tasks.iter().len(), 3);
    assert_eq!(tasks.get(0).unwrap().summary(), "task1");
    assert_eq!(tasks.get(1).unwrap().summary(), "task2");
    assert_eq!(tasks.get(2).unwrap().summary(), "task3");

    ops.redo(&mut tasks);
    assert_eq!(tasks.iter().len(), 2);
    assert_eq!(tasks.get(0).unwrap().summary(), "task1");
    assert_eq!(tasks.get(1).unwrap().summary(), "task3");
  }

  /// Check that the `TaskOp::Update` variant works as expected.
  #[test]
  fn exec_undo_task_update() {
    let iter = [Task::new("task1"), Task::new("task2")]
      .into_iter()
      .enumerate()
      .map(|(idx, task)| (task, Position::from_int(idx)));
    let mut tasks = Db::from_iter_with_aux(iter);
    let mut ops = Ops::new(3);

    let task = tasks.get(0).unwrap().deref().clone();
    // Make a deep copy of the task.
    let mut updated = task.deref().clone();
    updated.set_summary("foo!".to_string());
    let op = TaskOp::update(task, updated);
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.iter().len(), 2);
    assert_eq!(tasks.get(0).unwrap().summary(), "foo!");
    assert_eq!(tasks.get(1).unwrap().summary(), "task2");

    ops.undo(&mut tasks);
    assert_eq!(tasks.iter().len(), 2);
    assert_eq!(tasks.get(0).unwrap().summary(), "task1");
    assert_eq!(tasks.get(1).unwrap().summary(), "task2");

    ops.redo(&mut tasks);
    assert_eq!(tasks.iter().len(), 2);
    assert_eq!(tasks.get(0).unwrap().summary(), "foo!");
    assert_eq!(tasks.get(1).unwrap().summary(), "task2");
  }

  /// Check that the `TaskOp::Update` variant works as expected when
  /// only a single task is present and the operation is no-op.
  #[test]
  fn exec_undo_task_move() {
    let iter = [Task::new("task1"), Task::new("task2")]
      .into_iter()
      .enumerate()
      .map(|(idx, task)| (task, Position::from_int(idx)));
    let mut tasks = Db::from_iter_with_aux(iter);
    let mut ops = Ops::new(3);

    let task = tasks.get(1).unwrap().deref().clone();
    let before = tasks.get(0).unwrap().deref().clone();
    let op = TaskOp::move_(task, Target::Before(before));
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.iter().len(), 2);
    assert_eq!(tasks.get(0).unwrap().summary(), "task2");
    assert_eq!(tasks.get(1).unwrap().summary(), "task1");

    ops.undo(&mut tasks);
    assert_eq!(tasks.iter().len(), 2);
    assert_eq!(tasks.get(0).unwrap().summary(), "task1");
    assert_eq!(tasks.get(1).unwrap().summary(), "task2");

    let task = tasks.get(1).unwrap().deref().clone();
    let after = tasks.get(0).unwrap().deref().clone();
    let op = TaskOp::move_(task, Target::After(after));
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.iter().len(), 2);
    assert_eq!(tasks.get(0).unwrap().summary(), "task1");
    assert_eq!(tasks.get(1).unwrap().summary(), "task2");

    ops.undo(&mut tasks);
    assert_eq!(tasks.iter().len(), 2);
    assert_eq!(tasks.get(0).unwrap().summary(), "task1");
    assert_eq!(tasks.get(1).unwrap().summary(), "task2");
  }

  /// Check that we can add a task to a `Tasks` object.
  #[test]
  fn add_task() {
    let task_vec = make_tasks(3);
    let tasks = Tasks::with_serde_tasks(task_vec.clone()).unwrap();
    let tags = Default::default();
    let task = tasks.add("4".to_string(), tags, None);

    let tasks = tasks.to_serde().into_task_vec();
    let mut expected = task_vec;
    let () = expected.push(task.to_serde());
    assert_eq!(tasks, expected);
  }

  /// Check that adding a task after another works correctly.
  #[test]
  fn add_task_after() {
    let task_vec = make_tasks(3);
    let tasks = Tasks::with_serde_tasks(task_vec.clone()).unwrap();
    let after = tasks.0.borrow().tasks.get(0).unwrap().deref().clone();
    let tags = Default::default();
    let task = tasks.add("4".to_string(), tags, Some(after));

    let tasks = tasks.to_serde().into_task_vec();
    let mut expected = task_vec;
    let () = expected.insert(1, task.to_serde());

    assert_eq!(tasks, expected);
  }

  /// Test that removing a task from a `Tasks` object works as it
  /// should.
  #[test]
  fn remove_task() {
    let task_vec = make_tasks(3);
    let tasks = Tasks::with_serde_tasks(task_vec.clone()).unwrap();
    let task = tasks.iter(|mut iter| iter.nth(1).unwrap().clone());
    tasks.remove(task);

    let tasks = tasks.to_serde().into_task_vec();
    let mut expected = task_vec;
    expected.remove(1);

    assert_eq!(tasks, expected);
  }

  /// Check that we can update a task in a `Tasks` object.
  #[test]
  fn update_task() {
    let task_vec = make_tasks(3);
    let tasks = Tasks::with_serde_tasks(task_vec.clone()).unwrap();
    let task = tasks.iter(|mut iter| iter.nth(1).unwrap().clone());
    // Make a deep copy of the task.
    let mut updated = task.deref().clone();
    updated.set_summary("amended".to_string());
    tasks.update(task, updated);

    let tasks = tasks.to_serde().into_task_vec();
    let mut expected = task_vec;
    expected[1].summary = "amended".to_string();

    assert_eq!(tasks, expected);
  }

  /// Check that moving a task before the first one works as expected.
  #[test]
  fn move_before_for_first() {
    let task_vec = make_tasks(3);
    let tasks = Tasks::with_serde_tasks(task_vec.clone()).unwrap();
    let task1 = tasks.iter(|mut iter| iter.next().unwrap().clone());
    let task2 = tasks.iter(|mut iter| iter.nth(1).unwrap().clone());
    tasks.move_before(task1, task2);

    let tasks = tasks.to_serde().into_task_vec();
    let expected = task_vec;
    assert_eq!(tasks, expected);
  }

  /// Check that moving a task after the last one works as expected.
  #[test]
  fn move_after_for_last() {
    let task_vec = make_tasks(3);
    let tasks = Tasks::with_serde_tasks(task_vec.clone()).unwrap();
    let task1 = tasks.iter(|mut iter| iter.nth(2).unwrap().clone());
    let task2 = tasks.iter(|mut iter| iter.nth(1).unwrap().clone());
    tasks.move_after(task1, task2);

    let expected = task_vec;
    let tasks = tasks.to_serde().into_task_vec();
    assert_eq!(tasks, expected);
  }

  /// Check that moving a task before another works as expected.
  #[test]
  fn move_before() {
    let task_vec = make_tasks(4);
    let tasks = Tasks::with_serde_tasks(task_vec.clone()).unwrap();
    let task1 = tasks.iter(|mut iter| iter.nth(2).unwrap().clone());
    let task2 = tasks.iter(|mut iter| iter.nth(1).unwrap().clone());
    tasks.move_before(task1, task2);

    let tasks = tasks.to_serde().into_task_vec();
    let mut expected = task_vec;
    expected.swap(2, 1);

    assert_eq!(tasks, expected);
  }

  /// Check that moving a task after another works as expected.
  #[test]
  fn move_after() {
    let task_vec = make_tasks(4);
    let tasks = Tasks::with_serde_tasks(task_vec.clone()).unwrap();
    let task1 = tasks.iter(|mut iter| iter.nth(1).unwrap().clone());
    let task2 = tasks.iter(|mut iter| iter.nth(2).unwrap().clone());
    tasks.move_after(task1, task2);

    let tasks = tasks.to_serde().into_task_vec();
    let mut expected = task_vec;
    expected.swap(1, 2);
    assert_eq!(tasks, expected);
  }
}
