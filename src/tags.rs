// Copyright (C) 2018-2021 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::hash::Hash;
use std::hash::Hasher;
use std::rc::Rc;

use crate::id::Id as IdT;
use crate::ser::tags::Id as SerTagId;
use crate::ser::tags::Tag as SerTag;
use crate::ser::tags::Template as SerTemplate;
use crate::ser::tags::Templates as SerTemplates;
use crate::ser::ToSerde;

#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct T(());

pub type Id = IdT<T>;

/// The name of a tag describing the completion state of a task.
pub const COMPLETE_TAG: &str = "complete";


/// A struct defining a particular tag.
#[derive(Debug, Eq)]
pub struct Template {
  id: Id,
  name: String,
}

impl Template {
  /// Create a new tag template with the given name.
  pub fn new<S>(name: S) -> Self
  where
    S: Into<String>,
  {
    Self {
      id: Id::new(),
      name: name.into(),
    }
  }

  /// Retrieve this template's ID.
  pub fn id(&self) -> Id {
    self.id
  }

  /// Retrieve the tag template's name.
  pub fn name(&self) -> &str {
    &self.name
  }
}

impl Hash for Template {
  fn hash<H>(&self, state: &mut H)
  where
    H: Hasher,
  {
    self.id.hash(state);
  }
}

impl PartialEq for Template {
  fn eq(&self, other: &Template) -> bool {
    let result = self.id == other.id;
    debug_assert!(!result || self.name == other.name);
    result
  }
}

impl PartialOrd for Template {
  fn partial_cmp(&self, other: &Template) -> Option<Ordering> {
    self.id.partial_cmp(&other.id)
  }
}

impl Ord for Template {
  fn cmp(&self, other: &Template) -> Ordering {
    self.id.cmp(&other.id)
  }
}

impl From<SerTemplate> for Template {
  fn from(template: SerTemplate) -> Self {
    Self {
      id: Id::new(),
      name: template.name,
    }
  }
}

impl ToSerde<SerTemplate> for Template {
  /// Convert the template into a serializable one.
  fn to_serde(&self) -> SerTemplate {
    SerTemplate {
      id: self.id.to_serde(),
      name: self.name.clone(),
    }
  }
}


/// An actual tag instance, which may be associated with a task.
#[derive(Clone, Debug, PartialEq)]
pub struct Tag {
  /// The underlying shared template.
  template: Rc<Template>,
}

impl Tag {
  /// Create a new tag referencing the given template.
  pub fn new(template: Rc<Template>) -> Tag {
    Self { template }
  }

  /// Retrieve the tag's ID.
  pub fn id(&self) -> Id {
    self.template.id()
  }

  /// Retrieve the tag's name.
  pub fn name(&self) -> &str {
    self.template.name()
  }

  /// Retrieve the tag's underlying template.
  pub fn template(&self) -> Rc<Template> {
    self.template.clone()
  }
}

impl ToSerde<SerTag> for Tag {
  /// Convert the tag into a serializable one.
  fn to_serde(&self) -> SerTag {
    SerTag {
      id: self.template.id.to_serde(),
    }
  }
}


/// A map used for converting tags as they were persisted to the
/// in-memory form, preserving the correct mapping to templates.
pub type TagMap = BTreeMap<SerTagId, Id>;


/// Ensure the given template set contains a tag with the given name.
fn ensure_contains<S>(templates: &mut BTreeSet<Rc<Template>>, name: S) -> Rc<Template>
where
  S: Into<String> + AsRef<str>,
{
  let found = templates.iter().find(|x| x.name == name.as_ref());

  if let Some(found) = found {
    found.clone()
  } else {
    let rc = Rc::new(Template::new(name.into()));
    let inserted = templates.insert(rc.clone());
    debug_assert!(inserted);
    rc
  }
}

/// A management structure for tag templates.
#[derive(Clone, Debug, PartialEq)]
pub struct Templates {
  /// A set of all the tag templates.
  templates: BTreeSet<Rc<Template>>,
  /// Reference to the tag template representing task completion.
  complete: Rc<Template>,
}

impl Templates {
  /// Create an empty `Templates` object.
  #[cfg(test)]
  pub fn new() -> Self {
    Self::with_serde(Default::default()).0
  }

  /// Create a `Templates` object from a `SerTemplates` object.
  ///
  /// The conversion also creates a "lookup" table mapping from the IDs
  /// as they were persisted to the in-memory ones.
  pub fn with_serde(templates: SerTemplates) -> (Self, TagMap) {
    let (mut templates, map) = templates
      .0
      .into_iter()
      .map(|x| {
        let serde_id = x.id;
        let template = Template::from(x);
        let id = template.id();
        (Rc::new(template), (serde_id, id))
      })
      .unzip();

    let complete = ensure_contains(&mut templates, COMPLETE_TAG);
    let templates = Self {
      templates,
      complete,
    };
    (templates, map)
  }

  /// Instantiate a new tag from the referenced template.
  pub fn instantiate(&self, id: Id) -> Tag {
    let result = self.templates.iter().find(|x| x.id == id);

    match result {
      Some(template) => Tag::new(template.clone()),
      None => panic!("Attempt to create tag from invalid Id {}", id),
    }
  }

  /// Instantiate a new tag based on a name.
  #[cfg(test)]
  pub fn instantiate_from_name(&self, name: &str) -> Tag {
    let result = self.templates.iter().find(|x| x.name == name);

    match result {
      Some(template) => Tag::new(template.clone()),
      None => panic!("Attempt to create tag from invalid name: {}", name),
    }
  }

  /// Retrieve a reference to the 'complete' tag template.
  pub fn complete_tag(&self) -> &Template {
    &self.complete
  }

  /// Retrieve an iterator over all the tag templates.
  pub fn iter(&self) -> impl Iterator<Item = Rc<Template>> + '_ {
    self.templates.iter().map(Rc::clone)
  }
}

impl Extend<Template> for Templates
where
  T: Ord,
{
  fn extend<I>(&mut self, iter: I)
  where
    I: IntoIterator<Item = Template>,
  {
    self.templates.extend(iter.into_iter().map(Rc::new))
  }
}

impl ToSerde<SerTemplates> for Templates {
  /// Convert the tag templates object into a serializable form.
  fn to_serde(&self) -> SerTemplates {
    SerTemplates(self.templates.iter().map(|x| x.to_serde()).collect())
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  use serde_json::from_str as from_json;
  use serde_json::to_string as to_json;


  #[test]
  fn ensure_complete_tag_exists() {
    let templates = Templates::new();
    let template = templates.iter().find(|x| x.name == COMPLETE_TAG).unwrap();
    assert_eq!(template.id(), templates.complete_tag().id());
  }

  #[test]
  fn ensure_complete_tag_is_not_duplicated() {
    let templates = Templates::new();
    let serialized = to_json(&templates.to_serde()).unwrap();
    let deserialized = from_json::<SerTemplates>(&serialized).unwrap();
    let (new_templates, _) = Templates::with_serde(deserialized);

    let count = new_templates
      .iter()
      .fold(0, |c, x| if x.name == COMPLETE_TAG { c + 1 } else { c });
    assert_eq!(count, 1);
  }
}
