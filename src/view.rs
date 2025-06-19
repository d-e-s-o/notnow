// Copyright (C) 2017-2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::rc::Rc;

use anyhow::anyhow;
use anyhow::Result;

use crate::formula::Formula;
use crate::ser::view::cnf_to_formula;
use crate::ser::view::formula_to_cnf;
use crate::ser::view::FormulaPair;
use crate::ser::view::TagLit as SerTagLit;
use crate::ser::view::View as SerView;
use crate::ser::ToSerde;
use crate::tags::Tag;
use crate::tags::Templates;
use crate::tasks::Task;
use crate::tasks::TaskIter;
use crate::tasks::Tasks;


/// A literal describing whether a tag is negated or not.
#[derive(Clone, Debug)]
enum TagLit {
  Pos(Tag),
  Neg(Tag),
}

impl TagLit {
  /// Retrieve the contained `Tag`.
  fn tag(&self) -> &Tag {
    match self {
      TagLit::Pos(tag) | TagLit::Neg(tag) => tag,
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

impl From<&TagLit> for Formula {
  fn from(other: &TagLit) -> Self {
    match other {
      TagLit::Pos(tag) => Formula::Var(tag.template().name().to_string()),
      TagLit::Neg(tag) => !Formula::Var(tag.template().name().to_string()),
    }
  }
}


/// An object providing filtered iteration over an iterator of tasks.
#[derive(Clone, Debug)]
pub struct Filter<'tasks> {
  iter: TaskIter<'tasks>,
  lits: &'tasks [Box<[TagLit]>],
}

impl<'tasks> Filter<'tasks> {
  /// Create a new `Filter` wrapping an iterator and filtering using the given set of literals.
  fn new(iter: TaskIter<'tasks>, lits: &'tasks [Box<[TagLit]>]) -> Self {
    Self { iter, lits }
  }

  /// Check if one of the given tags matches the available ones.
  fn matches<'tag, I>(lits: &[TagLit], avail_tags: &I) -> bool
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

  /// Check if the given `tags` match this view's requirements.
  fn matched_by<'tag, I>(&self, avail_tags: &I) -> bool
  where
    I: Iterator<Item = &'tag Tag> + Clone,
  {
    // Iterate over conjunctions; all of them need to match.
    for req_lits in self.lits {
      // We could create a set for faster inclusion checks instead of
      // passing in an iterator. However, typically tasks only use a
      // small set of tags and so the allocation overhead is assumed to
      // be higher than the iteration cost we incur right now.
      if !Self::matches(req_lits, avail_tags) {
        return false
      }
    }
    true
  }
}

impl<'tasks> Iterator for Filter<'tasks> {
  type Item = &'tasks Rc<Task>;

  /// Advance the iterator yielding the next matching task or None.
  fn next(&mut self) -> Option<Self::Item> {
    // TODO: Should really be a for loop or even just a .find()
    //       invocation, however, both versions do not compile due to
    //       borrowing/ownership conflicts.
    loop {
      match self.iter.next() {
        Some(task) => {
          if task.tags(|iter| self.matched_by(&iter)) {
            return Some(task)
          }
        },
        None => return None,
      }
    }
  }
}

impl DoubleEndedIterator for Filter<'_> {
  fn next_back(&mut self) -> Option<Self::Item> {
    loop {
      match self.iter.next_back() {
        Some(task) => {
          if task.tags(|iter| self.matched_by(&iter)) {
            return Some(task)
          }
        },
        None => return None,
      }
    }
  }
}


/// A builder object to create a `View`.
// Strictly speaking the builder contains the same members as the actual
// `View` object and, hence, could be merged into it easily. However,
// the API would be rather unnatural and non-obvious. A `View` is
// supposed to be something that does not change over its lifetime.
pub struct ViewBuilder {
  tasks: Rc<Tasks>,
  lits: Vec<Vec<TagLit>>,
}

impl ViewBuilder {
  /// Create a new `ViewBuilder` object.
  pub fn new(tasks: Rc<Tasks>) -> ViewBuilder {
    Self {
      tasks,
      lits: Default::default(),
    }
  }

  /// Add a new conjunction containing the given literal to the view.
  #[cfg(test)]
  fn and_lit(mut self, lit: TagLit) -> ViewBuilder {
    // An AND always starts a new vector of ORs.
    self.lits.push(vec![lit]);
    self
  }

  /// Add a new conjunction containing the given tag to the view.
  ///
  /// Note that ANDed tags always associate with previously ANDed ones.
  /// That is, if you ORed a tag before you won't be able to OR any more
  /// tags to that same tag after a tag was ANDed in. E.g.,
  ///
  /// view
  ///  .or(tag1)  // `and` or `or` act equivalently for the first tag
  ///  .and(tag2)
  ///  .or(tag3)
  ///  .and(tag4)
  ///
  /// Is equivalent to tag1 && (tag2 || tag3) && tag4.
  #[cfg(test)]
  pub fn and(self, tag: impl Into<Tag>) -> ViewBuilder {
    self.and_lit(TagLit::Pos(tag.into()))
  }

  /// Add a new conjunction containing the given tag in negated form to
  /// the view.
  ///
  /// Please see `View::and` for more details on how ANDed tags
  /// associate with one another and with ORed ones.
  #[cfg(test)]
  pub fn and_not(self, tag: impl Into<Tag>) -> ViewBuilder {
    self.and_lit(TagLit::Neg(tag.into()))
  }

  /// Add a new literal to the last disjunction.
  #[cfg(test)]
  fn or_lit(mut self, lit: TagLit) -> ViewBuilder {
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
  /// Please see `View::and` for more details on how ORed tags
  /// associate with one another and with ANDed ones.
  #[cfg(test)]
  pub fn or(self, tag: impl Into<Tag>) -> ViewBuilder {
    self.or_lit(TagLit::Pos(tag.into()))
  }

  /// Add a new tag in negated form to the last disjunction.
  ///
  /// Please see `View::and` for more details on how ORed tags
  /// associate with one another and with ANDed ones.
  #[cfg(test)]
  pub fn or_not(self, tag: impl Into<Tag>) -> ViewBuilder {
    self.or_lit(TagLit::Neg(tag.into()))
  }

  /// Build the final `View` instance.
  pub fn build(self, name: impl Into<String>) -> View {
    let lits = self
      .lits
      .into_iter()
      .map(|vec| vec.into_boxed_slice())
      .collect::<Box<[_]>>();

    View {
      name: name.into(),
      tasks: self.tasks,
      // TODO: This conversion should not exist at a conceptual level.
      //       We should eventually transition everything to a world
      //       where we start with a formula instead of going back to
      //       it.
      formula: cnf_to_formula::<TagLit>(&lits)
        .map(|f| f.to_string())
        .unwrap_or_default(),
      lits,
    }
  }
}


/// An object representing a particular view onto a `Tasks` object.
///
/// Ultimately a `View` is conceptually an iterator over a set of `Task`
/// objects. However, there are crucial differences to ordinary
/// iterators.
/// 1) Where an normal iterator requires a true reference to the
///    underlying collection, a `View` relieves that restriction.
/// 2) Iteration is internal instead of external to reduce the
///    likelihood of borrowing conflicts.
#[derive(Clone, Debug)]
pub struct View {
  /// The name of the view.
  // TODO: This attribute does not really belong in here. Once we have
  //       the necessary infrastructure for storing it elsewhere it
  //       should be removed from this struct.
  name: String,
  /// A reference to the `Tasks` object we use for retrieving the set of
  /// available tasks.
  tasks: Rc<Tasks>,
  /// The textual representation of the logical formula describing the
  /// view (and from which the Conjunctive Normal Form is derived).
  formula: String,
  /// Tags are stored in Conjunctive Normal Form, meaning we have a
  /// large AND (all the "outer" elements) of ORs (all the inner ones).
  lits: Box<[Box<[TagLit]>]>,
}

impl View {
  /// Create a new `View` object from a serializable one.
  pub fn with_serde(view: SerView, templates: &Rc<Templates>, tasks: Rc<Tasks>) -> Result<Self> {
    let SerView { name, formula } = view;
    let FormulaPair { string, formula } = formula;

    let lits = if let Some(formula) = formula {
      let cnf =
        formula_to_cnf(formula).ok_or_else(|| anyhow!("encountered invalid tag with value `0`"))?;
      let lits = cnf
        .iter()
        .map(|b| {
          b.into_iter()
            .map(|lit| {
              let tag = templates
                .instantiate_from_name(lit.name())
                .ok_or_else(|| anyhow!("encountered invalid tag `{}`", lit.name()))?;
              let lit = match lit {
                SerTagLit::Pos(_) => TagLit::Pos(tag),
                SerTagLit::Neg(_) => TagLit::Neg(tag),
              };
              Ok(lit)
            })
            .collect::<Result<Box<[_]>>>()
        })
        .collect::<Result<Box<[_]>>>()?;
      lits
    } else {
      Box::default()
    };

    Ok(Self {
      name,
      tasks,
      formula: string,
      lits,
    })
  }

  /// Invoke a user-provided function on an iterator over the tasks
  /// represented by this view.
  #[inline]
  pub fn iter<F, R>(&self, mut f: F) -> R
  where
    F: FnMut(Filter<'_>) -> R,
  {
    self.tasks.iter(|iter| f(Filter::new(iter, &self.lits)))
  }

  /// Retrieve an iterator over all tags of the positive literals in
  /// this `View`.
  pub fn positive_tag_iter(&self) -> impl Iterator<Item = &Tag> {
    self.lits.iter().flat_map(|disjunctions| {
      disjunctions.iter().filter_map(|literal| match literal {
        TagLit::Pos(tag) => Some(tag),
        TagLit::Neg(..) => None,
      })
    })
  }

  /// Check whether the view is empty or not.
  #[cfg(test)]
  pub fn is_empty(&self) -> bool {
    self.iter(|mut iter| iter.next().is_none())
  }

  /// Retrieve the view's name.
  pub fn name(&self) -> &str {
    &self.name
  }
}

impl ToSerde for View {
  type Output = SerView;

  /// Convert this view into a serializable one.
  fn to_serde(&self) -> Self::Output {
    SerView {
      name: self.name.clone(),
      formula: FormulaPair {
        string: self.formula.clone(),
        // We intend for the resulting object to the serializable and for
        // that we only need the string.
        formula: None,
      },
    }
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  use crate::ser::tags::Templates as SerTemplates;
  use crate::ser::tasks::Tasks as SerTasks;
  use crate::tags::Templates;
  use crate::test::make_tasks;
  use crate::test::make_tasks_with_tags;
  use crate::test::COMPLETE_TAG;


  /// Create a view with the given number of tasks in it.
  fn make_view(count: usize) -> View {
    let tasks = Tasks::with_serde_tasks(make_tasks(count));
    let tasks = Rc::new(tasks.unwrap());
    let view = ViewBuilder::new(tasks).build("test");
    view
  }

  fn make_tagged_tasks(count: usize) -> (Rc<Templates>, Rc<Tasks>) {
    let (_, templates, tasks) = make_tasks_with_tags(count);
    let templates = Rc::new(Templates::with_serde(SerTemplates(templates)).unwrap());
    let tasks = SerTasks::from(tasks);
    let tasks = Tasks::with_serde(tasks, Rc::clone(&templates)).unwrap();
    let tasks = Rc::new(tasks);

    (templates, tasks)
  }


  /// Check that we can identify an empty `View`.
  #[test]
  fn is_empty() {
    assert!(make_view(0).is_empty());
    assert!(!make_view(1).is_empty());
  }

  /// Make sure that we can retrieve an iterator over all positive tag
  /// literals in a `View`.
  #[test]
  fn pos_tag_iteration() {
    fn pos_tags(view: &View) -> Vec<String> {
      view
        .positive_tag_iter()
        .map(|tag| tag.name().to_string())
        .collect::<Vec<_>>()
    }

    let (templates, tasks) = make_tagged_tasks(20);

    let view = ViewBuilder::new(Rc::clone(&tasks)).build("test");
    assert_eq!(pos_tags(&view), Vec::<String>::new());

    let view = ViewBuilder::new(Rc::clone(&tasks))
      .and(templates.instantiate_from_name(COMPLETE_TAG).unwrap())
      .build("test");
    assert_eq!(pos_tags(&view), vec![COMPLETE_TAG]);

    let view = ViewBuilder::new(Rc::clone(&tasks))
      .and(templates.instantiate_from_name("tag1").unwrap())
      .and_not(templates.instantiate_from_name("tag2").unwrap())
      .build("test");
    assert_eq!(pos_tags(&view), vec!["tag1"]);

    let view = ViewBuilder::new(Rc::clone(&tasks))
      .and(templates.instantiate_from_name("tag1").unwrap())
      .or(templates.instantiate_from_name("tag2").unwrap())
      .build("test");
    assert_eq!(pos_tags(&view), vec!["tag1", "tag2"]);

    let view = ViewBuilder::new(Rc::clone(&tasks))
      .and(templates.instantiate_from_name("tag3").unwrap())
      .or(templates.instantiate_from_name("tag1").unwrap())
      .and_not(templates.instantiate_from_name("tag2").unwrap())
      .build("test");
    assert_eq!(pos_tags(&view), vec!["tag3", "tag1"]);

    let view = ViewBuilder::new(Rc::clone(&tasks))
      .or(templates.instantiate_from_name("tag3").unwrap())
      .or(templates.instantiate_from_name("tag2").unwrap())
      .or(templates.instantiate_from_name("tag5").unwrap())
      .build("test");
    assert_eq!(pos_tags(&view), vec!["tag3", "tag2", "tag5"]);

    let view = ViewBuilder::new(tasks)
      .and(templates.instantiate_from_name("tag3").unwrap())
      .and(templates.instantiate_from_name("tag2").unwrap())
      .and(templates.instantiate_from_name("tag5").unwrap())
      .build("test");
    assert_eq!(pos_tags(&view), vec!["tag3", "tag2", "tag5"]);
  }

  /// Check that we can correctly filter all completed tasks.
  #[test]
  fn filter_completions() {
    let (templates, tasks) = make_tagged_tasks(16);
    let complete_tag = templates.instantiate_from_name(COMPLETE_TAG).unwrap();
    let view = ViewBuilder::new(tasks)
      .and(complete_tag.clone())
      .build("test");

    let () = view.iter(|mut iter| {
      let () = iter
        .clone()
        .for_each(|task| assert!(task.has_tag(&complete_tag)));

      assert_eq!(iter.next().unwrap().summary(), "2");
      assert_eq!(iter.next().unwrap().summary(), "4");
      assert_eq!(iter.next().unwrap().summary(), "6");
      assert_eq!(iter.next().unwrap().summary(), "8");
      assert_eq!(iter.next().unwrap().summary(), "10");
      assert_eq!(iter.next().unwrap().summary(), "12");
      assert_eq!(iter.next().unwrap().summary(), "14");
      assert_eq!(iter.next().unwrap().summary(), "16");
      assert!(iter.next().is_none());
    });
  }

  #[test]
  fn filter_no_completions() {
    let (templates, tasks) = make_tagged_tasks(16);
    let complete_tag = templates.instantiate_from_name(COMPLETE_TAG).unwrap();
    let view = ViewBuilder::new(tasks)
      .and_not(complete_tag.clone())
      .build("test");

    view.iter(|mut iter| {
      let () = iter
        .clone()
        .for_each(|task| assert!(!task.has_tag(&complete_tag)));

      assert_eq!(iter.next().unwrap().summary(), "1");
      assert_eq!(iter.next().unwrap().summary(), "3");
      assert_eq!(iter.next().unwrap().summary(), "5");
      assert_eq!(iter.next().unwrap().summary(), "7");
      assert_eq!(iter.next().unwrap().summary(), "9");
      assert_eq!(iter.next().unwrap().summary(), "11");
      assert_eq!(iter.next().unwrap().summary(), "13");
      assert_eq!(iter.next().unwrap().summary(), "15");
      assert!(iter.next().is_none());
    })
  }

  #[test]
  fn filter_tag1_and_tag2() {
    let (templates, tasks) = make_tagged_tasks(20);
    let tag1 = templates
      .instantiate_from_name(templates.iter().nth(1).unwrap().name())
      .unwrap();
    let tag2 = templates
      .instantiate_from_name(templates.iter().nth(2).unwrap().name())
      .unwrap();
    let view = ViewBuilder::new(tasks).and(tag1).and(tag2).build("test");

    let () = view.iter(|mut iter| {
      assert_eq!(iter.next().unwrap().summary(), "11");
      assert_eq!(iter.next().unwrap().summary(), "12");
      assert_eq!(iter.next().unwrap().summary(), "15");
      assert_eq!(iter.next().unwrap().summary(), "16");
      assert_eq!(iter.next().unwrap().summary(), "19");
      assert_eq!(iter.next().unwrap().summary(), "20");
      assert!(iter.next().is_none());
    });
  }

  #[test]
  fn filter_tag3_or_tag1() {
    let (templates, tasks) = make_tagged_tasks(20);
    let tag1 = templates
      .instantiate_from_name(templates.iter().nth(1).unwrap().name())
      .unwrap();
    let tag3 = templates
      .instantiate_from_name(templates.iter().nth(3).unwrap().name())
      .unwrap();
    let view = ViewBuilder::new(tasks).or(tag3).or(tag1).build("test");

    let () = view.iter(|mut iter| {
      assert_eq!(iter.next().unwrap().summary(), "5");
      assert_eq!(iter.next().unwrap().summary(), "6");
      assert_eq!(iter.next().unwrap().summary(), "7");
      assert_eq!(iter.next().unwrap().summary(), "8");
      assert_eq!(iter.next().unwrap().summary(), "11");
      assert_eq!(iter.next().unwrap().summary(), "12");
      assert_eq!(iter.next().unwrap().summary(), "13");
      assert_eq!(iter.next().unwrap().summary(), "14");
      assert_eq!(iter.next().unwrap().summary(), "15");
      assert_eq!(iter.next().unwrap().summary(), "16");
      assert_eq!(iter.next().unwrap().summary(), "19");
      assert_eq!(iter.next().unwrap().summary(), "20");
      assert!(iter.next().is_none());
    });
  }

  #[test]
  fn filter_tag1_and_complete_or_tag4() {
    let (templates, tasks) = make_tagged_tasks(20);
    let complete_tag = templates.instantiate_from_name(COMPLETE_TAG).unwrap();
    let tag1 = templates
      .instantiate_from_name(templates.iter().nth(1).unwrap().name())
      .unwrap();
    let tag4 = templates
      .instantiate_from_name(templates.iter().nth(4).unwrap().name())
      .unwrap();
    let view = ViewBuilder::new(tasks)
      .and(tag1)
      .and(complete_tag)
      .or(tag4)
      .build("test");

    let () = view.iter(|mut iter| {
      assert_eq!(iter.next().unwrap().summary(), "6");
      assert_eq!(iter.next().unwrap().summary(), "8");
      assert_eq!(iter.next().unwrap().summary(), "12");
      assert_eq!(iter.next().unwrap().summary(), "16");
      assert_eq!(iter.next().unwrap().summary(), "19");
      assert_eq!(iter.next().unwrap().summary(), "20");
      assert!(iter.next().is_none());
    });
  }

  #[test]
  fn filter_tag2_and_not_complete() {
    let (templates, tasks) = make_tagged_tasks(20);
    let complete_tag = templates.instantiate_from_name(COMPLETE_TAG).unwrap();
    let tag2 = templates
      .instantiate_from_name(templates.iter().nth(2).unwrap().name())
      .unwrap();
    let view = ViewBuilder::new(tasks)
      .and_not(tag2)
      .and_not(complete_tag)
      .build("test");

    let () = view.iter(|mut iter| {
      assert_eq!(iter.next().unwrap().summary(), "1");
      assert_eq!(iter.next().unwrap().summary(), "3");
      assert_eq!(iter.next().unwrap().summary(), "5");
      assert_eq!(iter.next().unwrap().summary(), "7");
      assert_eq!(iter.next().unwrap().summary(), "13");
      assert_eq!(iter.next().unwrap().summary(), "17");
      assert!(iter.next().is_none());
    });
  }

  #[test]
  fn filter_tag2_or_not_complete_and_tag3() {
    let (templates, tasks) = make_tagged_tasks(20);
    let complete_tag = templates.instantiate_from_name(COMPLETE_TAG).unwrap();
    let tag2 = templates
      .instantiate_from_name(templates.iter().nth(2).unwrap().name())
      .unwrap();
    let tag3 = templates
      .instantiate_from_name(templates.iter().nth(3).unwrap().name())
      .unwrap();
    let view = ViewBuilder::new(tasks)
      .or_not(tag2)
      .or_not(complete_tag)
      .and(tag3)
      .build("test");

    let () = view.iter(|mut iter| {
      assert_eq!(iter.next().unwrap().summary(), "13");
      assert_eq!(iter.next().unwrap().summary(), "14");
      assert_eq!(iter.next().unwrap().summary(), "15");
      assert_eq!(iter.next().unwrap().summary(), "19");
      assert!(iter.next().is_none());
    });
  }
}
