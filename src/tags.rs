// Copyright (C) 2018-2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
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


#[derive(Debug, Eq)]
struct TemplateInner {
  id: Id,
  name: String,
}

impl PartialEq for TemplateInner {
  fn eq(&self, other: &TemplateInner) -> bool {
    let result = self.id == other.id;
    debug_assert!(!result || self.name == other.name);
    result
  }
}

impl PartialOrd for TemplateInner {
  fn partial_cmp(&self, other: &TemplateInner) -> Option<Ordering> {
    self.id.partial_cmp(&other.id)
  }
}

impl Ord for TemplateInner {
  fn cmp(&self, other: &TemplateInner) -> Ordering {
    self.id.cmp(&other.id)
  }
}


/// A struct defining a particular tag.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Template(Rc<TemplateInner>);

impl Template {
  /// Create a new tag template with the given name.
  pub fn new<S>(name: S) -> Self
  where
    S: Into<String>,
  {
    let inner = TemplateInner {
      id: Id::new(),
      name: name.into(),
    };

    Self(Rc::new(inner))
  }

  /// Create a `Template` object from a `SerTemplate`.
  fn with_serde(template: SerTemplate) -> Self {
    Self::new(template.name)
  }

  /// Retrieve this template's ID.
  #[inline]
  pub fn id(&self) -> Id {
    self.0.id
  }

  /// Retrieve the tag template's name.
  #[inline]
  pub fn name(&self) -> &str {
    &self.0.name
  }
}

impl ToSerde<SerTemplate> for Template {
  /// Convert the template into a serializable one.
  fn to_serde(&self) -> SerTemplate {
    SerTemplate {
      id: self.0.id.to_serde(),
      name: self.0.name.clone(),
    }
  }
}


/// An actual tag instance, which may be associated with a task.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct Tag {
  /// The underlying shared template.
  template: Template,
}

impl Tag {
  /// Create a new tag referencing the given template.
  pub fn new(template: Template) -> Tag {
    Self { template }
  }

  /// Retrieve the tag's name.
  pub fn name(&self) -> &str {
    self.template.name()
  }

  /// Retrieve the tag's underlying template.
  pub fn template(&self) -> Template {
    self.template.clone()
  }
}

impl ToSerde<SerTag> for Tag {
  /// Convert the tag into a serializable one.
  fn to_serde(&self) -> SerTag {
    SerTag {
      id: self.template.id().to_serde(),
    }
  }
}


/// A map used for converting tags as they were persisted to the
/// in-memory form, preserving the correct mapping to templates.
pub type TagMap = BTreeMap<SerTagId, Id>;


/// A management structure for tag templates.
#[derive(Debug)]
pub struct Templates {
  /// A set of all the tag templates.
  templates: BTreeSet<Template>,
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
    let (templates, map) = templates
      .0
      .into_iter()
      .map(|x| {
        let serde_id = x.id;
        let template = Template::with_serde(x);
        let id = template.id();
        (template, (serde_id, id))
      })
      .unzip();

    let templates = Self { templates };
    (templates, map)
  }

  /// Instantiate a new tag from the referenced template.
  pub fn instantiate(&self, id: Id) -> Tag {
    let result = self.templates.iter().find(|x| x.id() == id);

    match result {
      Some(template) => Tag::new(template.clone()),
      None => panic!("Attempt to create tag from invalid Id {}", id),
    }
  }

  /// Instantiate a new tag based on a name.
  #[cfg(test)]
  pub fn instantiate_from_name(&self, name: &str) -> Tag {
    let result = self.templates.iter().find(|x| x.name() == name);

    match result {
      Some(template) => Tag::new(template.clone()),
      None => panic!("Attempt to create tag from invalid name: {}", name),
    }
  }

  /// Retrieve an iterator over all the tag templates.
  pub fn iter(&self) -> impl Iterator<Item = Template> + '_ {
    self.templates.iter().cloned()
  }
}

impl Extend<Template> for Templates {
  fn extend<I>(&mut self, iter: I)
  where
    I: IntoIterator<Item = Template>,
  {
    self.templates.extend(iter.into_iter())
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


  /// Check that different `Tag` objects instantiated from the same
  /// `Template` are considered equal.
  #[test]
  fn different_instantiated_tags_are_equal() {
    let template = SerTemplate {
      id: SerTagId::try_from(42).unwrap(),
      name: "test-tag".to_string(),
    };

    let (templates, _map) = Templates::with_serde(SerTemplates(vec![template]));
    let tag1 = templates.instantiate_from_name("test-tag");
    let tag2 = templates.instantiate_from_name("test-tag");

    assert_eq!(tag1, tag2)
  }
}
