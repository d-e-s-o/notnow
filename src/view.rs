// Copyright (C) 2017-2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::rc::Rc;
use std::str::FromStr as _;

use anyhow::anyhow;
use anyhow::Result;

use crate::formula::Formula;
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
pub struct ViewBuilder {
  templates: Rc<Templates>,
  tasks: Rc<Tasks>,
}

impl ViewBuilder {
  /// Create a new `ViewBuilder` object.
  pub fn new(templates: &Rc<Templates>, tasks: &Rc<Tasks>) -> ViewBuilder {
    Self {
      templates: Rc::clone(templates),
      tasks: Rc::clone(tasks),
    }
  }

  /// Build the final `View` instance.
  pub fn build(&self, name: impl Into<String>, formula: &str) -> Result<View> {
    let formula = FormulaPair {
      formula: if formula.is_empty() {
        None
      } else {
        Some(Formula::from_str(formula)?)
      },
      string: formula.to_string(),
    };

    View::from_formula(
      name.into(),
      formula,
      Rc::clone(&self.templates),
      Rc::clone(&self.tasks),
    )
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
  /// A reference to the `Templates` object we use for instantiating
  /// tags.
  templates: Rc<Templates>,
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
  fn from_formula(
    name: String,
    formula: FormulaPair,
    templates: Rc<Templates>,
    tasks: Rc<Tasks>,
  ) -> Result<Self> {
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
      templates,
      tasks,
      formula: string,
      lits,
    })
  }

  /// Create a new `View` object from a serializable one.
  pub fn with_serde(view: SerView, templates: Rc<Templates>, tasks: Rc<Tasks>) -> Result<Self> {
    let SerView { name, formula } = view;
    Self::from_formula(name, formula, templates, tasks)
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
    let templates = Rc::new(Templates::new());
    let tasks = Tasks::with_serde_tasks(make_tasks(count));
    let tasks = Rc::new(tasks.unwrap());
    let view = ViewBuilder::new(&templates, &tasks)
      .build("test", "")
      .unwrap();
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
    let builder = ViewBuilder::new(&templates, &tasks);

    let view = builder.build("test", "").unwrap();
    assert_eq!(pos_tags(&view), Vec::<String>::new());

    let view = builder.build("test", "complete").unwrap();
    assert_eq!(pos_tags(&view), vec![COMPLETE_TAG]);

    let view = builder.build("test", "tag1 & !tag2").unwrap();
    assert_eq!(pos_tags(&view), vec!["tag1"]);

    let view = builder.build("test", "tag1 | tag2").unwrap();
    assert_eq!(pos_tags(&view), vec!["tag1", "tag2"]);

    let view = builder.build("test", "(tag3 | tag1) & !tag2").unwrap();
    assert_eq!(pos_tags(&view), vec!["tag3", "tag1"]);

    let view = builder.build("test", "tag3 | tag2 | tag5").unwrap();
    assert_eq!(pos_tags(&view), vec!["tag3", "tag2", "tag5"]);

    let view = builder.build("test", "tag3 & tag2 & tag5").unwrap();
    assert_eq!(pos_tags(&view), vec!["tag3", "tag2", "tag5"]);
  }

  /// Check that we can correctly filter all completed tasks.
  #[test]
  fn filter_completions() {
    let (templates, tasks) = make_tagged_tasks(16);
    let complete_tag = templates.instantiate_from_name(COMPLETE_TAG).unwrap();
    let view = ViewBuilder::new(&templates, &tasks)
      .build("test", COMPLETE_TAG)
      .unwrap();

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
    let view = ViewBuilder::new(&templates, &tasks)
      .build("test", "!complete")
      .unwrap();

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
    let view = ViewBuilder::new(&templates, &tasks)
      .build("test", "tag1 & tag2")
      .unwrap();

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
    let view = ViewBuilder::new(&templates, &tasks)
      .build("test", "tag3 | tag1")
      .unwrap();

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
    let view = ViewBuilder::new(&templates, &tasks)
      .build("test", "tag1 & (complete | tag4)")
      .unwrap();

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
    let view = ViewBuilder::new(&templates, &tasks)
      .build("test", "!tag2 & !complete")
      .unwrap();

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
    let view = ViewBuilder::new(&templates, &tasks)
      .build("test", "(!tag2 | !complete) & tag3")
      .unwrap();

    let () = view.iter(|mut iter| {
      assert_eq!(iter.next().unwrap().summary(), "13");
      assert_eq!(iter.next().unwrap().summary(), "14");
      assert_eq!(iter.next().unwrap().summary(), "15");
      assert_eq!(iter.next().unwrap().summary(), "19");
      assert!(iter.next().is_none());
    });
  }
}
