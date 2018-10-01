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

use std::io::Error;
use std::io::ErrorKind;
use std::io::Result as IoResult;
#[cfg(test)]
use std::iter::FromIterator;
use std::rc::Rc;

use cell::RefCell;

use ser::query::Query as SerQuery;
use ser::query::TagLit as SerTagLit;
use tags::Tag;
use tags::TagMap;
use tags::Templates;
use tasks::Task;
use tasks::Tasks;


/// A literal describing whether a tag is negated or not.
#[derive(Clone, Debug)]
enum TagLit {
  Pos(Tag),
  Neg(Tag),
}

impl TagLit {
  /// Convert this object into a serializable one.
  fn to_serde(&self) -> SerTagLit {
    match self {
      TagLit::Pos(tag) => SerTagLit::Pos(tag.to_serde()),
      TagLit::Neg(tag) => SerTagLit::Neg(tag.to_serde()),
    }
  }

  /// Retrieve the contained `Tag`.
  fn tag(&self) -> &Tag {
    match self {
      TagLit::Pos(tag) |
      TagLit::Neg(tag) => tag,
    }
  }

  /// Check whether the literal is a positive one.
  fn is_pos(&self) -> bool {
    match self {
      TagLit::Pos(_) => true,
      TagLit::Neg(_) => false,
    }
  }
}


/// A builder object to create a `Query`.
// Strictly speaking the builder contains the same members as the actual
// `Query` object and, hence, could be merged into it easily. However,
// the API would be rather unnatural and non-obvious. A `Query` is
// supposed to be something that does not change over its lifetime.
pub struct QueryBuilder {
  tasks: Rc<RefCell<Tasks>>,
  lits: Vec<Vec<TagLit>>,
}

impl QueryBuilder {
  /// Create a new `QueryBuilder` object.
  pub fn new(tasks: Rc<RefCell<Tasks>>) -> QueryBuilder {
    QueryBuilder {
      tasks: tasks,
      lits: Default::default(),
    }
  }

  /// Add a new conjunction containing the given literal to the query.
  #[cfg(test)]
  fn and_lit(mut self, lit: TagLit) -> QueryBuilder {
    // An AND always starts a new vector of ORs.
    self.lits.push(vec![lit]);
    self
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
  pub fn and(self, tag: impl Into<Tag>) -> QueryBuilder {
    self.and_lit(TagLit::Pos(tag.into()))
  }

  /// Add a new conjunction containing the given tag in negated form to the query.
  ///
  /// Please see `Query::and` for more details on how ANDed tags
  /// associate with one another and with ORed ones.
  #[cfg(test)]
  pub fn and_not(self, tag: impl Into<Tag>) -> QueryBuilder {
    self.and_lit(TagLit::Neg(tag.into()))
  }

  /// Add a new literal to the last disjunction.
  #[cfg(test)]
  fn or_lit(mut self, lit: TagLit) -> QueryBuilder {
    let last = self.lits.pop();
    match last {
      Some(mut last) => {
        last.push(lit);
        self.lits.push(last);
      },
      None => self.lits.push(vec![lit]),
    };
    self
  }

  /// Add a new tag to the last disjunction.
  ///
  /// Please see `Query::and` for more details on how ORed tags
  /// associate with one another and with ANDed ones.
  #[cfg(test)]
  pub fn or(self, tag: impl Into<Tag>) -> QueryBuilder {
    self.or_lit(TagLit::Pos(tag.into()))
  }

  /// Add a new tag in negated form to the last disjunction.
  ///
  /// Please see `Query::and` for more details on how ORed tags
  /// associate with one another and with ANDed ones.
  #[cfg(test)]
  pub fn or_not(self, tag: impl Into<Tag>) -> QueryBuilder {
    self.or_lit(TagLit::Neg(tag.into()))
  }

  /// Build the final `Query` instance.
  pub fn build(self, name: impl Into<String>) -> Query {
    Query {
      name: name.into(),
      tasks: self.tasks,
      lits: self.lits,
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
  lits: Vec<Vec<TagLit>>,
}

impl Query {
  /// Create a new `Query` object from a serializable one.
  pub fn with_serde(mut query: SerQuery,
                    templates: &Rc<Templates>,
                    map: &TagMap,
                    tasks: Rc<RefCell<Tasks>>) -> IoResult<Self> {
    let mut and_lits = Vec::with_capacity(query.lits.len());
    for mut lits in query.lits.drain(..) {
      let mut or_lits = Vec::with_capacity(lits.len());
      for lit in lits.drain(..) {
        let id = map.get(&lit.id()).ok_or_else(|| {
          let error = format!("Encountered invalid tag Id {}", lit.id());
          Error::new(ErrorKind::InvalidInput, error)
        })?;
        let tag = templates.instantiate(*id);
        let lit = match lit {
          SerTagLit::Pos(_) => TagLit::Pos(tag),
          SerTagLit::Neg(_) => TagLit::Neg(tag),
        };
        or_lits.push(lit);
      }

      and_lits.push(or_lits);
    }

    Ok(Query {
      name: query.name,
      tasks: tasks,
      lits: and_lits,
    })
  }

  /// Convert this query into a serializable one.
  pub fn to_serde(&self) -> SerQuery {
    let lits = self
      .lits
      .iter()
      .map(|lits| lits.iter().map(|x| x.to_serde()).collect())
      .collect();

    SerQuery {
      name: self.name.clone(),
      lits: lits,
    }
  }

  /// Check if one of the given tags matches the available ones.
  fn matches<'tag, I>(&self, lits: &[TagLit], avail_tags: &I) -> bool
  where
    I: Iterator<Item = &'tag Tag> + Clone,
  {
    // Iterate over disjunctions and check if any of them matches.
    for lit in lits {
      let tag = lit.tag();
      let must_exist = lit.is_pos();

      if avail_tags.clone().any(|x| x == tag) == must_exist {
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
    for req_lits in &self.lits {
      // We could create a set for faster inclusion checks instead of
      // passing in an iterator. However, typically tasks only use a
      // small set of tags and so the allocation overhead is assumed to
      // be higher than the iteration cost we incur right now.
      if !self.matches(req_lits, avail_tags) {
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
    self.position_from(0, predicate)
  }

  /// Find the position of a `Task` satisfying the given predicate from a certain index.
  // TODO: This API is not really nice. Can we come up with a
  //       better/more flexible design somehow?
  pub fn position_from<P>(&self, idx: usize, predicate: P) -> Option<usize>
  where
    P: FnMut(&Task) -> bool,
  {
    let tasks = self.tasks.borrow();
    let result = self
      .iter(&tasks)
      .skip(idx)
      .position(predicate)
      .and_then(|x| Some(x + idx));
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

  /// Retrieve the query's name.
  pub fn name(&self) -> &str {
    &self.name
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  use ser::tags::Templates as SerTemplates;
  use ser::tasks::Tasks as SerTasks;
  use tags::Templates;
  use test::make_tasks;
  use test::make_tasks_with_tags;


  /// Create a query with the given number of tasks in it.
  fn make_query(count: usize) -> Query {
    let tasks = Tasks::with_serde_tasks(make_tasks(count));
    let tasks = Rc::new(RefCell::new(tasks.unwrap()));
    let query = QueryBuilder::new(tasks).build("test");
    query
  }

  fn make_tagged_tasks(count: usize) -> (Rc<Templates>, Rc<RefCell<Tasks>>) {
    let (_, templates, tasks) = make_tasks_with_tags(count);
    let (templates, map) = Templates::with_serde(SerTemplates(templates));
    let templates = Rc::new(templates);
    let tasks = SerTasks(tasks);
    let tasks = Tasks::with_serde(tasks, templates.clone(), &map).unwrap();
    let tasks = Rc::new(RefCell::new(tasks));

    (templates, tasks)
  }


  #[test]
  fn count() {
    assert_eq!(make_query(5).count(), 5);
  }

  #[test]
  fn nth() {
    let query = make_query(7);
    let expected = make_tasks(7).iter().cloned().nth(3);
    assert_eq!(query.nth(3).as_ref().unwrap().summary, expected.unwrap().summary);
  }

  #[test]
  fn for_each() {
    let mut counter = 0;
    make_query(16).for_each(|_| counter += 1);

    assert_eq!(counter, 16);
  }

  #[test]
  fn enumerate() {
    let mut counter = 0;
    make_query(20)
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
    let mut counter = 0;
    make_query(20)
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
    let query = make_query(3);
    let idx = query.position(|task| task.summary == format!("{}", 2));

    assert_eq!(idx.unwrap(), 1)
  }

  #[test]
  fn position_from() {
    let query = make_query(10);
    let idx = query.position_from(1, |task| task.summary == format!("{}", 2));
    assert_eq!(idx.unwrap(), 1);

    let idx = query.position_from(3, |task| task.summary == format!("{}", 2));
    assert!(idx.is_none());

    let idx = query.position_from(3, |task| task.summary == format!("{}", 5));
    assert_eq!(idx.unwrap(), 4)
  }

  #[test]
  fn collect() {
    let tasks = make_query(3).collect::<Vec<_>>();
    let tasks = tasks.iter().map(|x| x.to_serde()).collect::<Vec<_>>();
    assert_eq!(tasks, make_tasks(3));
  }

  #[test]
  fn is_empty() {
    assert!(make_query(0).is_empty());
    assert!(!make_query(1).is_empty());
  }

  #[test]
  fn filter_completions() {
    let (templates, tasks) = make_tagged_tasks(16);
    let complete_tag = templates.instantiate(templates.complete_tag().id());
    let query = QueryBuilder::new(tasks.clone())
      .and(complete_tag)
      .build("test");

    query.for_each(|x| assert!(x.is_complete()));

    let tasks = tasks.borrow();
    let mut iter = query.iter(&tasks);
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
  fn filter_no_completions() {
    let (templates, tasks) = make_tagged_tasks(16);
    let complete_tag = templates.instantiate(templates.complete_tag().id());
    let query = QueryBuilder::new(tasks.clone())
      .and_not(complete_tag)
      .build("test");

    query.for_each(|x| assert!(!x.is_complete()));

    let tasks = tasks.borrow();
    let mut iter = query.iter(&tasks);
    assert_eq!(iter.next().unwrap().summary, "1");
    assert_eq!(iter.next().unwrap().summary, "3");
    assert_eq!(iter.next().unwrap().summary, "5");
    assert_eq!(iter.next().unwrap().summary, "7");
    assert_eq!(iter.next().unwrap().summary, "9");
    assert_eq!(iter.next().unwrap().summary, "11");
    assert_eq!(iter.next().unwrap().summary, "13");
    assert_eq!(iter.next().unwrap().summary, "15");
    assert!(iter.next().is_none());
  }

  #[test]
  fn filter_tag1_and_tag2() {
    let (templates, tasks) = make_tagged_tasks(20);
    let tag1 = templates.instantiate(templates.iter().nth(1).unwrap().id());
    let tag2 = templates.instantiate(templates.iter().nth(2).unwrap().id());
    let query = QueryBuilder::new(tasks.clone())
      .and(tag1)
      .and(tag2)
      .build("test");

    let tasks = tasks.borrow();
    let mut iter = query.iter(&tasks);
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
    let (templates, tasks) = make_tagged_tasks(20);
    let tag1 = templates.instantiate(templates.iter().nth(1).unwrap().id());
    let tag3 = templates.instantiate(templates.iter().nth(3).unwrap().id());
    let query = QueryBuilder::new(tasks.clone())
      .or(tag3)
      .or(tag1)
      .build("test");

    let tasks = tasks.borrow();
    let mut iter = query.iter(&tasks);
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
    let (templates, tasks) = make_tagged_tasks(20);
    let complete_tag = templates.instantiate(templates.complete_tag().id());
    let tag1 = templates.instantiate(templates.iter().nth(1).unwrap().id());
    let tag4 = templates.instantiate(templates.iter().nth(4).unwrap().id());
    let query = QueryBuilder::new(tasks.clone())
      .and(tag1)
      .and(complete_tag)
      .or(tag4)
      .build("test");

    let tasks = tasks.borrow();
    let mut iter = query.iter(&tasks);
    assert_eq!(iter.next().unwrap().summary, "6");
    assert_eq!(iter.next().unwrap().summary, "8");
    assert_eq!(iter.next().unwrap().summary, "12");
    assert_eq!(iter.next().unwrap().summary, "16");
    assert_eq!(iter.next().unwrap().summary, "19");
    assert_eq!(iter.next().unwrap().summary, "20");
    assert!(iter.next().is_none());
  }

  #[test]
  fn filter_tag2_and_not_complete() {
    let (templates, tasks) = make_tagged_tasks(20);
    let complete_tag = templates.instantiate(templates.complete_tag().id());
    let tag2 = templates.instantiate(templates.iter().nth(2).unwrap().id());
    let query = QueryBuilder::new(tasks.clone())
      .and_not(tag2)
      .and_not(complete_tag)
      .build("test");

    let tasks = tasks.borrow();
    let mut iter = query.iter(&tasks);
    assert_eq!(iter.next().unwrap().summary, "1");
    assert_eq!(iter.next().unwrap().summary, "3");
    assert_eq!(iter.next().unwrap().summary, "5");
    assert_eq!(iter.next().unwrap().summary, "7");
    assert_eq!(iter.next().unwrap().summary, "13");
    assert_eq!(iter.next().unwrap().summary, "17");
    assert!(iter.next().is_none());
  }

  #[test]
  fn filter_tag2_or_not_complete_and_tag3() {
    let (templates, tasks) = make_tagged_tasks(20);
    let complete_tag = templates.instantiate(templates.complete_tag().id());
    let tag2 = templates.instantiate(templates.iter().nth(2).unwrap().id());
    let tag3 = templates.instantiate(templates.iter().nth(3).unwrap().id());
    let query = QueryBuilder::new(tasks.clone())
      .or_not(tag2)
      .or_not(complete_tag)
      .and(tag3)
      .build("test");

    let tasks = tasks.borrow();
    let mut iter = query.iter(&tasks);
    assert_eq!(iter.next().unwrap().summary, "13");
    assert_eq!(iter.next().unwrap().summary, "14");
    assert_eq!(iter.next().unwrap().summary, "15");
    assert_eq!(iter.next().unwrap().summary, "19");
    assert!(iter.next().is_none());
  }
}
