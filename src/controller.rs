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

use std::io::Result;
use std::path::Path;
use std::path::PathBuf;

use tasks::Task;
use tasks::TaskIter;
use tasks::Tasks;


/// An object providing higher-level functionality relating to tasks.
#[derive(Debug)]
pub struct Controller {
  path: PathBuf,
  tasks: Tasks,
}

impl Controller {
  /// Create a new controller object using the task data at the given path.
  pub fn new<P>(task_path: P) -> Result<Self>
  where
    P: Into<PathBuf> + AsRef<Path>,
  {
    let tasks = Tasks::new(&task_path)?;

    Ok(Controller {
      path: task_path.into(),
      tasks: tasks,
    })
  }

  /// Save the tasks into a file.
  pub fn save(&self) -> Result<()> {
    self.tasks.save(&self.path)
  }

  /// Retrieve the tasks associated with this controller.
  pub fn tasks(&self) -> TaskIter {
    self.tasks.iter()
  }

  /// Add a new task to the list of tasks.
  pub fn add_task(&mut self, task: Task) {
    self.tasks.add(task)
  }

  /// Remove the task at the given index.
  pub fn remove_task(&mut self, index: usize) {
    self.tasks.remove(index)
  }
}
