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
#[cfg(test)]
use std::iter::FromIterator;
use std::rc::Rc;

use tags::Tag;
use tasks::Task;
use tasks::Tasks;


/// A builder object to create a `Query`.
// Strictly speaking the builder contains the same members as the actual
// `Query` object and, hence, could be merged into it easily. However,
// the API would be rather unnatural and non-obvious. A `Query` is
// supposed to be something that does not change over its lifetime.
pub struct QueryBuilder {
  tasks: Rc<RefCell<Tasks>>,
  tags: Vec<Vec<Tag>>,
}

impl QueryBuilder {
  /// Create a new `QueryBuilder` object.
  pub fn new(tasks: Rc<RefCell<Tasks>>) -> QueryBuilder {
    QueryBuilder {
      tasks: tasks,
      tags: Default::default(),
    }
  }

  /// Add a new conjunction containing the given tag to the query.
  ///
  /// Note that ANDed tags always associate with previously ANDed ones.
  /// That is, if you ORed a tag before you won't be able to OR any more
  /// tags to that same tag after a tag was ANDed in. E.g.,
  ///
  /// query
  ///  .or(tag1)  // `and` or `or` act equivalently for the first tag
  ///  .and(tag2)
  ///  .or(tag3)
  ///  .and(tag4)
  ///
  /// Is equivalent to tag1 && (tag2 || tag3) && tag4.
  #[cfg(test)]
  pub fn and(mut self, tag: impl Into<Tag>) -> QueryBuilder {
    // An AND always starts a new vector of ORs.
    self.tags.push(vec![tag.into()]);
    self
  }

  /// Add a new tag to the last disjunction.
  ///
  /// Please see `Query::and` for more details on how ORed tags
  /// associate with one another and with ANDed ones.
  #[cfg(test)]
  pub fn or(mut self, tag: impl Into<Tag>) -> QueryBuilder {
    let last = self.tags.pop();
    match last {
      Some(mut last) => {
        last.push(tag.into());
        self.tags.push(last);
      },
      None => self.tags.push(vec![tag.into()]),
    };
    self
  }

  /// Build the final `Query` instance.
  pub fn build(self, name: impl Into<String>) -> Query {
    Query {
      name: name.into(),
      tasks: self.tasks,
      tags: self.tags,
    }
  }
}


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
  /// The name of the query.
  // TODO: This attribute does not really belong in here. Once we have
  //       the necessary infrastructure for storing it elsewhere it
  //       should be removed from this struct.
  name: String,
  /// A reference to the `Tasks` object we use for retrieving the set of
  /// available tasks.
  tasks: Rc<RefCell<Tasks>>,
  /// Tags are stored in Conjunctive Normal Form, meaning we have a
  /// large AND (all elements in the outer vector) of ORs (all the
  /// elements in the inner vector).
  tags: Vec<Vec<Tag>>,
}

impl Query {
  /// Check if one of the given tags matches the available ones.
  fn matches<'tag, I>(&self, tags: &[Tag], avail_tags: &I) -> bool
  where
    I: Iterator<Item = &'tag Tag> + Clone,
  {
    // Iterate over disjunctions and check if any of them matches.
    for tag in tags {
      if avail_tags.clone().any(|x| x == tag) {
        return true
      }
    }
    false
  }

  /// Check if the given `tags` match this query's requirements.
  fn matched_by<'tag, I>(&self, avail_tags: &I) -> bool
  where
    I: Iterator<Item = &'tag Tag> + Clone,
  {
    // Iterate over conjunctions; all of them need to match.
    for req_tags in &self.tags {
      // We could create a set for faster inclusion checks instead of
      // passing in an iterator. However, typically tasks only use a
      // small set of tags and so the allocation overhead is assumed to
      // be higher than the iteration cost we incur right now.
      if !self.matches(req_tags, avail_tags) {
        return false
      }
    }
    true
  }

  /// Retrieve an iterator over the tasks with all tag filters applied.
  fn iter<'t, 's: 't>(&'s self, tasks: &'t Tasks) -> impl Iterator<Item = &'t Task> {
    tasks.iter().filter(move |x| self.matched_by(&x.tags()))
  }

  /// Count the number of tasks matched by the query.
  pub fn count(&self) -> usize {
    let tasks = self.tasks.borrow();
    let count = self.iter(&tasks).count();
    count
  }

  /// Returns the nth task for the query.
  pub fn nth(&self, n: usize) -> Option<Task> {
    let tasks = self.tasks.borrow();
    let result = self.iter(&tasks).cloned().nth(n);
    result
  }

  /// Perform an action on each task.
  #[cfg(test)]
  pub fn for_each<F>(&self, action: F)
  where
    F: FnMut(&Task),
  {
    let tasks = self.tasks.borrow();
    self.iter(&tasks).for_each(action);
  }

  /// Iterate over tasks along with indication for current iteration count.
  pub fn enumerate<E, F>(&self, mut action: F) -> Result<(), E>
  where
    F: FnMut(usize, &Task) -> Result<bool, E>,
  {
    let tasks = self.tasks.borrow();
    for (i, task) in self.iter(&tasks).enumerate() {
      if !action(i, task)? {
        break
      }
    }
    Ok(())
  }

  /// Find the position of a `Task` satisfying the given predicate.
  pub fn position<P>(&self, predicate: P) -> Option<usize>
  where
    P: FnMut(&Task) -> bool,
  {
    let tasks = self.tasks.borrow();
    let result = self.iter(&tasks).position(predicate);
    result
  }

  /// Create a collection from the query.
  #[cfg(test)]
  pub fn collect<C>(&self) -> C
  where
    C: FromIterator<Task>,
  {
    let tasks = self.tasks.borrow();
    let result = self.iter(&tasks).cloned().collect();
    result
  }

  /// Check whether the query is empty or not.
  pub fn is_empty(&self) -> bool {
    let tasks = self.tasks.borrow();
    let result = self.iter(&tasks).next().is_none();
    result
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  use tasks::tests::make_tasks;
  use tasks::tests::make_tasks_with_tags;
  use tasks::tests::TaskVec;


  #[test]
  fn count() {
    let tasks = Rc::new(RefCell::new(make_tasks(5)));
    let query = QueryBuilder::new(tasks).build("test");
    assert_eq!(query.count(), 5);
  }

  #[test]
  fn nth() {
    let tasks = Rc::new(RefCell::new(make_tasks(7)));
    let query = QueryBuilder::new(tasks).build("test");
    let expected = make_tasks(7).iter().cloned().nth(3);
    assert_eq!(query.nth(3).as_ref().unwrap().summary, expected.unwrap().summary);
  }

  #[test]
  fn for_each() {
    let mut counter = 0;
    let tasks = Rc::new(RefCell::new(make_tasks(16)));
    let query = QueryBuilder::new(tasks).build("test");
    query.for_each(|_| counter += 1);

    assert_eq!(counter, 16);
  }

  #[test]
  fn enumerate() {
    let tasks = Rc::new(RefCell::new(make_tasks(20)));
    let query = QueryBuilder::new(tasks).build("test");
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
    let tasks = Rc::new(RefCell::new(make_tasks(20)));
    let query = QueryBuilder::new(tasks).build("test");
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
  fn position() {
    let tasks = Rc::new(RefCell::new(make_tasks(3)));
    let query = QueryBuilder::new(tasks).build("test");
    let idx = query.position(|task| task.summary == format!("{}", 2));

    assert_eq!(idx.unwrap(), 1)
  }

  #[test]
  fn collect() {
    let tasks = Rc::new(RefCell::new(make_tasks(3)));
    let query = QueryBuilder::new(tasks).build("test");
    let result = query.collect::<TaskVec>();
    let expected = make_tasks(3).iter().cloned().collect::<TaskVec>();
    assert_eq!(result, expected);
  }

  #[test]
  fn is_empty() {
    let tasks = Rc::new(RefCell::new(make_tasks(0)));
    assert!(QueryBuilder::new(tasks).build("test").is_empty());

    let tasks = Rc::new(RefCell::new(make_tasks(1)));
    assert!(!QueryBuilder::new(tasks).build("test").is_empty());
  }

  #[test]
  fn filter_completions() {
    let tasks = Rc::new(RefCell::new(make_tasks_with_tags(16)));
    let tasks_ = tasks.borrow();
    let templates = tasks_.templates();
    let complete_tag = templates.instantiate(templates.complete_tag().id());
    let query = QueryBuilder::new(tasks.clone())
      .and(complete_tag)
      .build("test");

    query.for_each(|x| assert!(x.is_complete()));

    let mut iter = query.iter(&tasks_);
    assert_eq!(iter.next().unwrap().summary, "2");
    assert_eq!(iter.next().unwrap().summary, "4");
    assert_eq!(iter.next().unwrap().summary, "6");
    assert_eq!(iter.next().unwrap().summary, "8");
    assert_eq!(iter.next().unwrap().summary, "10");
    assert_eq!(iter.next().unwrap().summary, "12");
    assert_eq!(iter.next().unwrap().summary, "14");
    assert_eq!(iter.next().unwrap().summary, "16");
    assert!(iter.next().is_none());
  }

  #[test]
  fn filter_tag1_and_tag2() {
    let tasks = Rc::new(RefCell::new(make_tasks_with_tags(20)));
    let tasks_ = tasks.borrow();
    let templates = tasks_.templates();
    let tag1 = templates.instantiate(templates.iter().nth(1).unwrap().id());
    let tag2 = templates.instantiate(templates.iter().nth(2).unwrap().id());
    let query = QueryBuilder::new(tasks.clone())
      .and(tag1)
      .and(tag2)
      .build("test");

    let mut iter = query.iter(&tasks_);
    assert_eq!(iter.next().unwrap().summary, "11");
    assert_eq!(iter.next().unwrap().summary, "12");
    assert_eq!(iter.next().unwrap().summary, "15");
    assert_eq!(iter.next().unwrap().summary, "16");
    assert_eq!(iter.next().unwrap().summary, "19");
    assert_eq!(iter.next().unwrap().summary, "20");
    assert!(iter.next().is_none());
  }

  #[test]
  fn filter_tag3_or_tag1() {
    let tasks = Rc::new(RefCell::new(make_tasks_with_tags(20)));
    let tasks_ = tasks.borrow();
    let templates = tasks_.templates();
    let tag1 = templates.instantiate(templates.iter().nth(1).unwrap().id());
    let tag3 = templates.instantiate(templates.iter().nth(3).unwrap().id());
    let query = QueryBuilder::new(tasks.clone())
      .or(tag3)
      .or(tag1)
      .build("test");

    let mut iter = query.iter(&tasks_);
    assert_eq!(iter.next().unwrap().summary, "5");
    assert_eq!(iter.next().unwrap().summary, "6");
    assert_eq!(iter.next().unwrap().summary, "7");
    assert_eq!(iter.next().unwrap().summary, "8");
    assert_eq!(iter.next().unwrap().summary, "11");
    assert_eq!(iter.next().unwrap().summary, "12");
    assert_eq!(iter.next().unwrap().summary, "13");
    assert_eq!(iter.next().unwrap().summary, "14");
    assert_eq!(iter.next().unwrap().summary, "15");
    assert_eq!(iter.next().unwrap().summary, "16");
    assert_eq!(iter.next().unwrap().summary, "19");
    assert_eq!(iter.next().unwrap().summary, "20");
    assert!(iter.next().is_none());
  }

  #[test]
  fn filter_tag1_and_complete_or_tag4() {
    let tasks = Rc::new(RefCell::new(make_tasks_with_tags(20)));
    let tasks_ = tasks.borrow();
    let templates = tasks_.templates();
    let complete_tag = templates.instantiate(templates.complete_tag().id());
    let tag1 = templates.instantiate(templates.iter().nth(1).unwrap().id());
    let tag4 = templates.instantiate(templates.iter().nth(4).unwrap().id());
    let query = QueryBuilder::new(tasks.clone())
      .and(tag1)
      .and(complete_tag)
      .or(tag4)
      .build("test");

    let mut iter = query.iter(&tasks_);
    assert_eq!(iter.next().unwrap().summary, "6");
    assert_eq!(iter.next().unwrap().summary, "8");
    assert_eq!(iter.next().unwrap().summary, "12");
    assert_eq!(iter.next().unwrap().summary, "16");
    assert_eq!(iter.next().unwrap().summary, "19");
    assert_eq!(iter.next().unwrap().summary, "20");
    assert!(iter.next().is_none());
  }
}
