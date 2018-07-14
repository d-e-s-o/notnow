// query.rs

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
use std::iter::FromIterator;
use std::rc::Rc;

use tasks::Task;
use tasks::Tasks;


/// An object representing a particular view onto a `Tasks` object.
///
/// Ultimately a `Query` is conceptually an iterator over a set of
/// `Task` objects. However, there are crucial differences to ordinary
/// iterators.
/// 1) Where an normal iterator requires a true reference to the
///    underlying collection, a `Query` relieves that restriction.
/// 2) Iteration is internal instead of external to reduce the
///    likelihood of borrowing conflicts.
#[derive(Clone, Debug)]
pub struct Query {
  /// A reference to the `Tasks` object we use for retrieving the set of
  /// available tasks.
  tasks: Rc<RefCell<Tasks>>,
}

impl Query {
  /// Create a new `Query` object.
  pub fn new(tasks: Rc<RefCell<Tasks>>) -> Self {
    Query {
      tasks: tasks,
    }
  }

  /// Count the number of tasks matched by the query.
  pub fn count(&self) -> usize {
    self.tasks.borrow().iter().count()
  }

  /// Returns the nth task for the query.
  pub fn nth(&self, n: usize) -> Option<Task> {
    self.tasks.borrow().iter().cloned().nth(n)
  }

  /// Iterate over tasks along with indication for current iteration count.
  pub fn enumerate<E, F>(&self, mut action: F) -> Result<(), E>
  where
    F: FnMut(usize, &Task) -> Result<bool, E>,
  {
    for (i, task) in self.tasks.borrow().iter().enumerate() {
      if !action(i, task)? {
        break
      }
    }
    Ok(())
  }

  /// Create a collection from the query.
  pub fn collect<C>(&self) -> C
  where
    C: FromIterator<Task>,
  {
    self.tasks.borrow().iter().cloned().collect()
  }

  /// Check whether the query is empty or not.
  pub fn is_empty(&self) -> bool {
    self.tasks.borrow().iter().next().is_none()
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  use tasks::tests::make_tasks;


  #[test]
  fn count() {
    let query = Query::new(Rc::new(RefCell::new(make_tasks(5))));
    assert_eq!(query.count(), 5);
  }

  #[test]
  fn nth() {
    let query = Query::new(Rc::new(RefCell::new(make_tasks(7))));
    let expected = make_tasks(7).iter().cloned().nth(3);
    assert_eq!(query.nth(3), expected);
  }

  #[test]
  fn enumerate() {
    let query = Query::new(Rc::new(RefCell::new(make_tasks(20))));
    let mut counter = 0;
    query
      .enumerate::<(), _>(|i, task| {
        assert_eq!(counter, i);
        assert_eq!(task.summary, format!("{}", i + 1));
        counter += 1;
        Ok(true)
      }).unwrap();

    assert_eq!(counter, 20)
  }

  #[test]
  fn enumerate_early_break() {
    let query = Query::new(Rc::new(RefCell::new(make_tasks(20))));
    let mut counter = 0;
    query
      .enumerate::<(), _>(|i, _| {
        if i >= 10 {
          Ok(false)
        } else {
          counter += 1;
          Ok(true)
        }
      }).unwrap();

    assert_eq!(counter, 10)
  }

  #[test]
  fn collect() {
    let query = Query::new(Rc::new(RefCell::new(make_tasks(3))));
    let result = query.collect::<Vec<Task>>();
    let expected = make_tasks(3).iter().cloned().collect::<Vec<Task>>();
    assert_eq!(result, expected);
  }

  #[test]
  fn is_empty() {
    assert!(Query::new(Rc::new(RefCell::new(make_tasks(0)))).is_empty());
    assert!(!Query::new(Rc::new(RefCell::new(make_tasks(1)))).is_empty());
  }
}
