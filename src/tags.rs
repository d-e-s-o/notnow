// Copyright (C) 2018-2023 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::hash::Hash;
use std::hash::Hasher;
use std::rc::Rc;

use crate::id::AllocId;
use crate::id::Id as IdT;
use crate::ser::tags::Id as SerTagId;
use crate::ser::tags::Tag as SerTag;
use crate::ser::tags::Template as SerTemplate;
use crate::ser::tags::Templates as SerTemplates;
use crate::ser::tags::T;
use crate::ser::ToSerde;

type Id = IdT<T>;


/// A type representing a template for a tag.
#[derive(Debug, Eq)]
pub struct Template {
  id: Id,
  name: String,
}

impl Template {
  /// Create a new tag template with the given name.
  fn new<S>(id: Id, name: S) -> Self
  where
    S: Into<String>,
  {
    Self {
      id,
      name: name.into(),
    }
  }

  /// Create a `Template` object from a `SerTemplate`.
  fn with_serde(id: Id, template: SerTemplate) -> Self {
    Self::new(id, template.name)
  }

  /// Retrieve the tag template's name.
  #[inline]
  pub fn name(&self) -> &str {
    &self.name
  }
}

impl Hash for Template {
  fn hash<H>(&self, hasher: &mut H)
  where
    H: Hasher,
  {
    self.id.hash(hasher)
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
    Some(self.cmp(other))
  }
}

impl Ord for Template {
  fn cmp(&self, other: &Template) -> Ordering {
    self.id.cmp(&other.id)
  }
}

impl ToSerde for Template {
  type Output = SerTemplate;

  /// Convert the template into a serializable one.
  fn to_serde(&self) -> Self::Output {
    SerTemplate {
      id: self.id.to_serde(),
      name: self.name.clone(),
    }
  }
}


/// An actual tag instance, which may be associated with a task.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct Tag {
  /// The underlying shared template.
  template: Rc<Template>,
}

impl Tag {
  /// Create a new tag referencing the given template.
  pub fn new(template: Rc<Template>) -> Self {
    Self { template }
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

impl ToSerde for Tag {
  type Output = SerTag;

  /// Convert the tag into a serializable one.
  fn to_serde(&self) -> Self::Output {
    SerTag {
      id: self.template.id.to_serde(),
    }
  }
}


/// A management structure for tag templates.
#[derive(Debug)]
pub struct Templates {
  /// A mapping of all the tag templates, indexed by each one's `Id`,
  /// converted to `usize`.
  templates: BTreeMap<usize, Rc<Template>>,
}

impl Templates {
  /// Create an empty `Templates` object.
  #[cfg(test)]
  pub fn new() -> Self {
    Self::with_serde(SerTemplates::default()).unwrap()
  }

  /// Create a `Templates` object from a `SerTemplates` object.
  pub fn with_serde(templates: SerTemplates) -> Result<Self, SerTagId> {
    let templates =
      templates
        .0
        .into_iter()
        .try_fold(BTreeMap::new(), |mut templates, template| {
          let (id, entry) = templates
            .try_reserve_id(template.id.get())
            .ok_or(template.id)?;
          let template = Rc::new(Template::with_serde(id, template));
          let _value_ref = entry.insert(template);

          Ok(templates)
        })?;

    Ok(Self { templates })
  }

  /// Instantiate a tag from the given serialized tag ID.
  ///
  /// This methods return `None` if the provided `id` does not represent
  /// a known tag.
  pub fn instantiate(&self, id: SerTagId) -> Option<Tag> {
    self
      .templates
      .get(&id.get())
      .map(|template| Tag::new(template.clone()))
  }

  /// Instantiate a new tag based on a name.
  #[cfg(test)]
  pub fn instantiate_from_name(&self, name: &str) -> Tag {
    self
      .templates
      .values()
      .find(|template| template.name() == name)
      .map(|template| Tag::new(template.clone()))
      .unwrap_or_else(|| panic!("Attempt to create tag from invalid name: {}", name))
  }

  /// Retrieve an iterator over all the tag templates.
  pub fn iter(&self) -> impl Iterator<Item = Rc<Template>> + '_ {
    self.templates.values().cloned()
  }
}

#[cfg(test)]
impl<S> Extend<S> for Templates
where
  S: Into<String>,
{
  fn extend<I>(&mut self, iter: I)
  where
    I: IntoIterator<Item = S>,
  {
    let () = iter.into_iter().for_each(|name| {
      let (id, entry) = self.templates.allocate_id();
      let template = Rc::new(Template::new(id, name));
      let _value_ref = entry.insert(template);
    });
  }
}

impl ToSerde for Templates {
  type Output = SerTemplates;

  /// Convert the tag templates object into a serializable form.
  fn to_serde(&self) -> Self::Output {
    SerTemplates(
      self
        .templates
        .values()
        .map(|template| template.to_serde())
        .collect(),
    )
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

    let templates = Templates::with_serde(SerTemplates(vec![template])).unwrap();
    let tag1 = templates.instantiate_from_name("test-tag");
    let tag2 = templates.instantiate_from_name("test-tag");

    assert_eq!(tag1, tag2)
  }
}
