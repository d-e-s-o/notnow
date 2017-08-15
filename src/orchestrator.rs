// orchestrator.rs

// *************************************************************************
// * Copyright (C) 2017 Daniel Mueller (deso@posteo.net)                   *
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

use controller::Controller;
use tasks::TaskIter;
use tasks::Tasks;


/// A concrete controller suitable for our intents and purposes.
pub struct Orchestrator {
  path: String,
  tasks: Tasks,
}

impl Orchestrator {
  /// Create a new orchestrator object using the task data at the given path.
  pub fn new(task_path: &str) -> Result<Self> {
    let tasks = Tasks::new(task_path)?;

    Ok(Orchestrator {
      path: task_path.to_string(),
      tasks: tasks,
    })
  }
}

impl Controller for Orchestrator {
  fn save(&self) -> Result<()> {
    self.tasks.save(&self.path)
  }

  fn tasks(&self) -> TaskIter {
    self.tasks.iter()
  }
}
