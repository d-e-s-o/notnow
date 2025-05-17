// Copyright (C) 2018-2022 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

//! A module providing serialization and deserialization support for
//! task templates and tags.

use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;
use std::str::FromStr;

use anyhow::Context as _;
use anyhow::Error;

use serde::Deserialize;
use serde::Serialize;

use crate::ser::id::Id as IdT;


#[derive(Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct T(());

pub type Id = IdT<T>;


/// The separator to use for separating the components of a `Template`
/// when converting it to a string.
const TEMPLATE_COMPONENT_SEPARATOR: char = ',';


/// A struct for serializing the concept of a tag.
///
/// Objects of this type are used to describe what a tag looks like and
/// are the form in which the concept of a particular tag is persisted.
#[derive(Clone, Debug, PartialEq)]
pub struct Template {
  pub id: Id,
  pub name: String,
}

impl Display for Template {
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    write!(f, "{}{TEMPLATE_COMPONENT_SEPARATOR}{}", self.id, self.name)
  }
}

impl FromStr for Template {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let (id, name) = s
      .split_once(TEMPLATE_COMPONENT_SEPARATOR)
      .with_context(|| format!("string '{s}' is not a properly formatted template"))?;
    let id = Id::from_str(id)?;
    let name = name.to_string();

    Ok(Template { id, name })
  }
}


/// A serializable tag instance.
#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Tag {
  pub id: Id,
}

impl From<Id> for Tag {
  #[inline]
  fn from(id: Id) -> Self {
    Self { id }
  }
}

impl FromStr for Tag {
  type Err = <Id as FromStr>::Err;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let id = Id::from_str(s)?;
    Ok(Self { id })
  }
}

impl Display for Tag {
  #[inline]
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    Display::fmt(&self.id, f)
  }
}


/// A container of `Template` objects that we can deserialize into and
/// serialize from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Templates(pub Vec<Template>);


#[cfg(test)]
mod tests {
  use super::*;

  use crate::ser::backends::Backend;
  use crate::ser::backends::Json;


  /// Check that we can convert a `Template` to a string and parse it
  /// from there again.
  #[test]
  fn emit_parse_template() {
    let template = Template {
      id: Id::try_from(42).unwrap(),
      name: "test tag".to_string(),
    };

    let emitted = template.to_string();
    let parsed = Template::from_str(&emitted).unwrap();

    assert_eq!(parsed, template);
  }

  /// Check that we can convert a `Template` to a string and parse it
  /// from there again.
  #[test]
  fn emit_parse_tag() {
    let tag = Tag {
      id: Id::try_from(usize::MAX).unwrap(),
    };
    let emitted = tag.to_string();
    let parsed = Tag::from_str(&emitted).unwrap();

    assert_eq!(parsed, tag);
  }

  /// Check that we can serialize and deserialize a `Tag`.
  #[test]
  fn serialize_deserialize_tag() {
    let tag = Tag {
      id: Id::try_from(42).unwrap(),
    };
    let serialized = Json::serialize(&tag).unwrap();
    let deserialized = <Json as Backend<Tag>>::deserialize(&serialized).unwrap();

    assert_eq!(deserialized, tag);
  }
}
