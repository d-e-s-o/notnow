// Copyright (C) 2017-2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeSet;
use std::io::Error;
use std::io::ErrorKind;
use std::io::Result;
use std::mem::replace;
use std::rc::Rc;

use crate::db::Db;
use crate::db::Iter as DbIter;
use crate::id::Id as DbId;
use crate::ops::Op;
use crate::ops::Ops;
use crate::ser::tasks::Id as SerTaskId;
use crate::ser::tasks::Task as SerTask;
use crate::ser::tasks::Tasks as SerTasks;
use crate::ser::ToSerde;
use crate::tags::Tag;
use crate::tags::TagMap;
use crate::tags::Templates;


/// The maximum number of undo steps that we keep record of.
// TODO: We may consider making this value user-configurable.
const MAX_UNDO_STEP_COUNT: usize = 64;


pub type Id = DbId<Task>;

impl ToSerde<SerTaskId> for Id {
  /// Convert this task ID into a serializable one.
  fn to_serde(&self) -> SerTaskId {
    SerTaskId::new(self.get())
  }
}


/// A struct representing a task item.
#[derive(Clone, Debug)]
pub struct Task {
  summary: String,
  tags: BTreeSet<Tag>,
  templates: Rc<Templates>,
}

impl Task {
  /// Create a new task.
  #[cfg(test)]
  pub fn new(summary: impl Into<String>) -> Self {
    Self {
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
      summary: summary.into(),
      tags: tags.into_iter().collect(),
      templates,
    }
  }

  /// Create a new task from a serializable one.
  fn with_serde(task: SerTask, templates: Rc<Templates>, map: &TagMap) -> Result<Task> {
    let mut tags = BTreeSet::new();
    for tag in task.tags.into_iter() {
      let id = map.get(&tag.id).ok_or_else(|| {
        let error = format!("Encountered invalid tag Id {}", tag.id);
        Error::new(ErrorKind::InvalidInput, error)
      })?;
      tags.insert(templates.instantiate(*id));
    }

    Ok(Self {
      summary: task.summary,
      tags,
      templates,
    })
  }

  /// Get a reference to this [Task]'s summary.
  #[inline]
  pub fn summary(&self) -> &str {
    &self.summary
  }

  /// Change this [Task]'s summary.
  #[inline]
  pub fn set_summary(&mut self, summary: String) {
    self.summary = summary
  }

  /// Retrieve an iterator over this task's tags.
  pub fn tags(&self) -> impl Iterator<Item = &Tag> + Clone {
    self.tags.iter()
  }

  /// Set the tags of the task.
  pub fn set_tags<I>(&mut self, tags: I)
  where
    I: Iterator<Item = Tag>,
  {
    self.tags = tags.collect();
  }

  /// Check whether the task has the provided `tag` set.
  #[inline]
  pub fn has_tag(&self, tag: &Tag) -> bool {
    self.tags.get(tag).is_some()
  }

  /// Ensure that the provided tag is set on this task.
  #[inline]
  pub fn set_tag(&mut self, tag: Tag) -> bool {
    self.tags.insert(tag)
  }

  /// Ensure that the provided tag is not set on this task.
  #[inline]
  pub fn unset_tag(&mut self, tag: &Tag) -> bool {
    self.tags.remove(tag)
  }

  /// Retrieve the `Templates` object associated with this task.
  pub fn templates(&self) -> &Templates {
    &self.templates
  }
}

impl ToSerde<SerTask> for Task {
  /// Convert this task into a serializable one.
  fn to_serde(&self) -> SerTask {
    SerTask {
      summary: self.summary.clone(),
      tags: self.tags.iter().map(Tag::to_serde).collect(),
    }
  }
}


/// Add a task to a vector of tasks.
fn add_task(tasks: &mut Db<Task>, id: Option<Id>, task: Task, target: Option<Target>) -> Id {
  if let Some(target) = target {
    let idx = tasks.find(target.id()).unwrap();
    let idx = match target {
      Target::Before(..) => idx,
      Target::After(..) => idx + 1,
    };
    tasks.insert(idx, id, task)
  } else {
    tasks.push(id, task)
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
  Add {
    task: (Option<Id>, Task),
    after: Option<Id>,
  },
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
  fn add(task: Task, after: Option<Id>) -> Self {
    Self::Add {
      task: (None, task),
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
      Self::Add {
        ref mut task,
        after,
      } => {
        let id = add_task(tasks, task.0, task.1.clone(), after.map(Target::After));
        // Now that we know the task's ID, remember it in case we need
        // to undo and redo.
        task.0 = Some(id);
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
        *id = Some(removed.0);
        add_task(tasks, *id, removed.1, Some(*to));
        *id
      },
    }
  }

  fn undo(&mut self, tasks: &mut Db<Task>) -> Option<Id> {
    match self {
      Self::Add { task, .. } => {
        // SANITY: The ID will always be set at this point.
        let id = task.0.unwrap();
        let _ = remove_task(tasks, id);
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
        let id = before.0;
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
      .try_fold(Vec::with_capacity(len), |mut vec, (id, task)| {
        let task = Task::with_serde(task, templates.clone(), map)?;
        vec.push((id.get(), task));
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

    let tasks = SerTasks::from(tasks);
    let templates = Rc::new(Templates::new());
    let map = Default::default();

    Self::with_serde(tasks, templates, &map)
  }

  /// Convert this object into a serializable one.
  pub fn to_serde(&self) -> SerTasks {
    // TODO: We should consider including the operations here as well.
    SerTasks(
      self
        .tasks
        .iter()
        .map(|(id, task)| (id.to_serde(), task.to_serde()))
        .collect(),
    )
  }

  /// Retrieve an iterator over the tasks.
  pub fn iter(&self) -> TaskIter<'_> {
    self.tasks.iter()
  }

  /// Add a new task.
  pub fn add(&mut self, summary: String, tags: Vec<Tag>, after: Option<Id>) -> Id {
    let task = Task::with_summary_and_tags(summary, tags, self.templates.clone());
    let op = TaskOp::add(task, after);
    // SANITY: We know that an "add" operation always returns an ID, so
    //         this unwrap will never panic.
    let id = self.operations.exec(op, &mut self.tasks).unwrap();

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

  use std::num::NonZeroUsize;

  use serde_json::from_str as from_json;
  use serde_json::to_string_pretty as to_json;

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
    let (templates, _map) = Templates::with_serde(SerTemplates(templates));
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
    let mut tasks = Db::from_iter([]);
    let mut ops = Ops::new(3);

    let task1 = Task::new("task1");
    let op = TaskOp::add(task1, None);
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
    let mut tasks = Db::from_iter([Task::new("task1")]);
    let mut ops = Ops::new(3);
    let task2 = Task::new("task2");
    let op = TaskOp::add(task2, None);
    ops.exec(op, &mut tasks);
    assert_eq!(tasks.iter().len(), 2);
    assert_eq!(tasks.get(0).unwrap().summary, "task1");
    assert_eq!(tasks.get(1).unwrap().summary, "task2");

    let task3 = Task::new("task3");
    let op = TaskOp::add(task3, Some(tasks.get(0).unwrap().id()));
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
    let mut tasks = Db::from_iter([Task::new("task1")]);
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
    let mut tasks = Db::from_iter([Task::new("task1"), Task::new("task2"), Task::new("task3")]);
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
    let mut tasks = Db::from_iter([Task::new("task1"), Task::new("task2")]);
    let mut ops = Ops::new(3);

    let mut new = tasks.get(0).unwrap().clone();
    new.summary = "foo!".to_string();
    let op = TaskOp::update(tasks.get(0).unwrap().id(), new);
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
    let mut tasks = Db::from_iter([Task::new("task1"), Task::new("task2")]);
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

    let tasks = tasks.to_serde().into_task_vec();
    assert_eq!(tasks, make_tasks(4));
  }

  /// Check that adding a task after another works correctly.
  #[test]
  fn add_task_after() {
    let tasks = make_tasks(3);
    let mut tasks = Tasks::with_serde_tasks(tasks).unwrap();
    let id = tasks.tasks.get(0).unwrap().id();
    let tags = Default::default();
    tasks.add("4".to_string(), tags, Some(id));

    let tasks = tasks.to_serde().into_task_vec();
    let mut expected = make_tasks(4);
    let task = expected.remove(3);
    expected.insert(1, task);

    assert_eq!(tasks, expected);
  }

  #[test]
  fn remove_task() {
    let mut tasks = Tasks::with_serde_tasks(make_tasks(3)).unwrap();
    let id = tasks.iter().nth(1).unwrap().0;
    tasks.remove(id);

    let tasks = tasks.to_serde().into_task_vec();
    let mut expected = make_tasks(3);
    expected.remove(1);

    assert_eq!(tasks, expected);
  }

  #[test]
  fn update_task() {
    let mut tasks = Tasks::with_serde_tasks(make_tasks(3)).unwrap();
    let (id, mut task) = tasks.iter().nth(1).unwrap().clone();
    task.summary = "amended".to_string();
    tasks.update(id, task);

    let tasks = tasks.to_serde().into_task_vec();
    let mut expected = make_tasks(3);
    expected[1].summary = "amended".to_string();

    assert_eq!(tasks, expected);
  }

  #[test]
  fn move_before_for_first() {
    let mut tasks = Tasks::with_serde_tasks(make_tasks(3)).unwrap();
    let id1 = tasks.iter().next().unwrap().0;
    let id2 = tasks.iter().nth(1).unwrap().0;
    tasks.move_before(id1, id2);

    let tasks = tasks.to_serde().into_task_vec();
    let expected = make_tasks(3);
    assert_eq!(tasks, expected);
  }

  #[test]
  fn move_after_for_last() {
    let mut tasks = Tasks::with_serde_tasks(make_tasks(3)).unwrap();
    let id1 = tasks.iter().nth(2).unwrap().0;
    let id2 = tasks.iter().nth(1).unwrap().0;
    tasks.move_after(id1, id2);

    let expected = make_tasks(3);
    let tasks = tasks.to_serde().into_task_vec();
    assert_eq!(tasks, expected);
  }

  #[test]
  fn move_before() {
    let mut tasks = Tasks::with_serde_tasks(make_tasks(4)).unwrap();
    let id1 = tasks.iter().nth(2).unwrap().0;
    let id2 = tasks.iter().nth(1).unwrap().0;
    tasks.move_before(id1, id2);

    let tasks = tasks.to_serde().into_task_vec();
    let mut expected = make_tasks(4);
    expected.swap(2, 1);

    assert_eq!(tasks, expected);
  }

  #[test]
  fn move_after() {
    let mut tasks = Tasks::with_serde_tasks(make_tasks(4)).unwrap();
    let id1 = tasks.iter().nth(1).unwrap().0;
    let id2 = tasks.iter().nth(2).unwrap().0;
    tasks.move_after(id1, id2);

    let tasks = tasks.to_serde().into_task_vec();
    let mut expected = make_tasks(4);
    expected.swap(1, 2);
    assert_eq!(tasks, expected);
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
    let tasks = Tasks::with_serde(deserialized, templates, &map)
      .unwrap()
      .to_serde()
      .into_task_vec();

    assert_eq!(tasks, make_tasks(3));
  }
}
