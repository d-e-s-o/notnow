// Copyright (C) 2018-2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::id::AllocId;
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
  fn new<S>(id: Id, name: S) -> Self
  where
    S: Into<String>,
  {
    let inner = TemplateInner {
      id,
      name: name.into(),
    };

    Self(Rc::new(inner))
  }

  /// Create a `Template` object from a `SerTemplate`.
  fn with_serde(id: Id, template: SerTemplate) -> Self {
    Self::new(id, template.name)
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
  /// A mapping of all the tag templates, indexed by each one's `Id`,
  /// converted to `usize`.
  templates: BTreeMap<usize, Template>,
}

impl Templates {
  /// Create an empty `Templates` object.
  #[cfg(test)]
  pub fn new() -> Self {
    Self::with_serde(SerTemplates::default()).unwrap().0
  }

  /// Create a `Templates` object from a `SerTemplates` object.
  ///
  /// The conversion also creates a "lookup" table mapping from the IDs
  /// as they were persisted to the in-memory ones.
  pub fn with_serde(templates: SerTemplates) -> Result<(Self, TagMap), SerTagId> {
    let (templates, map) = templates.0.into_iter().try_fold(
      (BTreeMap::new(), TagMap::new()),
      |(mut templates, mut map), template| {
        let serde_id = template.id;
        let (id, entry) = templates.try_reserve_id(serde_id.get()).ok_or(serde_id)?;
        let template = Template::with_serde(id, template);
        let template_id = template.id();
        let _value_ref = entry.insert(template);

        let _previous = map.insert(serde_id, template_id);
        debug_assert_eq!(_previous, None);

        Ok((templates, map))
      },
    )?;

    let templates = Self { templates };
    Ok((templates, map))
  }

  /// Instantiate a tag from the given serialized tag ID.
  ///
  /// This methods return `None` if the provided `id` does not represent
  /// a known tag.
  pub fn instantiate_serde(&self, id: SerTagId) -> Option<Tag> {
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
  pub fn iter(&self) -> impl Iterator<Item = Template> + '_ {
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
      let template = Template::new(id, name);
      let _value_ref = entry.insert(template);
    });
  }
}

impl ToSerde<SerTemplates> for Templates {
  /// Convert the tag templates object into a serializable form.
  fn to_serde(&self) -> SerTemplates {
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

    let (templates, _map) = Templates::with_serde(SerTemplates(vec![template])).unwrap();
    let tag1 = templates.instantiate_from_name("test-tag");
    let tag2 = templates.instantiate_from_name("test-tag");

    assert_eq!(tag1, tag2)
  }
}
