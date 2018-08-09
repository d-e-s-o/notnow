// controller.rs

// *************************************************************************
// * Copyright (C) 2017-2018 Daniel Mueller (deso@posteo.net)              *
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

use std::cell::RefCell;
use std::io::Result;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;

use query::Query;
use tasks::Id as TaskId;
use tasks::Task;
use tasks::Tasks;


/// An object providing higher-level functionality relating to tasks.
#[derive(Debug)]
pub struct Controller {
  path: PathBuf,
  tasks: Rc<RefCell<Tasks>>,
}

impl Controller {
  /// Create a new controller object using the task data at the given path.
  pub fn new<P>(task_path: P) -> Result<Self>
  where
    P: Into<PathBuf> + AsRef<Path>,
  {
    let tasks = Tasks::new(&task_path)?;
    Ok(Self::with_tasks_and_path(tasks, task_path))
  }

  /// Create a new controller object with the given `Tasks` object, with
  /// all future `save` operations happening into the provided path.
  pub fn with_tasks_and_path<P>(tasks: Tasks, path: P) -> Self
  where
    P: Into<PathBuf> + AsRef<Path>,
  {
    Controller {
      path: path.into(),
      tasks: Rc::new(RefCell::new(tasks)),
    }
  }

  /// Save the tasks into a file.
  pub fn save(&self) -> Result<()> {
    self.tasks.borrow().save(&self.path)
  }

  /// Retrieve the tasks associated with this controller.
  pub fn tasks(&self) -> Query {
    Query::new(self.tasks.clone())
  }

  /// Add a new task to the list of tasks.
  pub fn add_task(&self, summary: impl Into<String>) -> TaskId {
    self.tasks.borrow_mut().add(summary)
  }

  /// Remove the task with the given `TaskId`.
  pub fn remove_task(&self, id: TaskId) {
    self.tasks.borrow_mut().remove(id)
  }

  /// Update a task.
  pub fn update_task(&self, task: Task) {
    self.tasks.borrow_mut().update(task)
  }
}
