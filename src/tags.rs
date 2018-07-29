// tags.rs

// *************************************************************************
// * Copyright (C) 2018 Daniel Mueller (deso@posteo.net)                   *
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

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::hash::Hash;
use std::hash::Hasher;
use std::rc::Rc;

use id::Id as IdT;
use ser::tags::Id as SerTagId;
use ser::tags::Tag as SerTag;
use ser::tags::Template as SerTemplate;
use ser::tags::Templates as SerTemplates;

#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct T(());

pub type Id = IdT<T>;


/// A struct defining a particular tag.
#[derive(Clone, Debug, Eq)]
pub struct Template {
  id: Id,
  name: String,
}

impl Template {
  /// Convert the template into a serializable one.
  pub fn to_serde(&self) -> SerTemplate {
    SerTemplate {
      id: self.id.to_serde(),
      name: self.name.clone(),
    }
  }

  /// Retrieve this template's ID.
  pub fn id(&self) -> Id {
    self.id
  }

  /// Retrieve the tag template's name.
  #[cfg(test)]
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
    Template {
      id: Id::new(),
      name: template.name,
    }
  }
}


/// An actual tag instance, which may be associated with a task.
#[derive(Clone, Debug, PartialEq)]
pub struct Tag {
  template: Rc<Template>,
}

impl Tag {
  /// Create a new tag referencing the given template.
  fn new(template: Rc<Template>) -> Tag {
    Tag {
      template: template,
    }
  }

  /// Convert the tag into a serializable one.
  pub fn to_serde(&self) -> SerTag {
    SerTag {
      id: self.template.id.to_serde(),
    }
  }

  /// Retrieve the tag's name.
  #[cfg(test)]
  pub fn name(&self) -> &str {
    self.template.name()
  }
}


/// A map used for converting tags as they were persisted to the
/// in-memory form, preserving the correct mapping to templates.
pub type TagMap = BTreeMap<SerTagId, Id>;


/// A management structure for tag templates.
#[derive(Clone, Debug, PartialEq)]
pub struct Templates {
  /// A set of all the tag templates.
  templates: BTreeSet<Rc<Template>>,
}

impl Templates {
  /// Create an empty `Templates` object.
  pub fn new() -> Self {
    Self::with_serde(Default::default()).0
  }

  /// Create a `Templates` object from a `SerTemplates` object.
  ///
  /// The conversion also creates a "lookup" table mapping from the IDs
  /// as they were persisted to the in-memory ones.
  pub fn with_serde(mut templates: SerTemplates) -> (Self, TagMap) {
    let (templates, map) = templates
      .0
      .drain(..)
      .map(|x| {
        let serde_id = x.id;
        let template = Template::from(x);
        let id = template.id();
        (Rc::new(template), (serde_id, id))
      })
      .unzip();

    let templates = Templates {
      templates: templates,
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

  /// Convert the tag templates object into a serializable form.
  pub fn to_serde(&self) -> SerTemplates {
    SerTemplates(self.templates.iter().map(|x| x.to_serde()).collect())
  }
}
