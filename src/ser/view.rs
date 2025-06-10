// Copyright (C) 2018-2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

//! A module providing serialization and deserialization support for
//! task views.

use std::fmt::Debug;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;

use serde::Deserialize;
use serde::Serialize;

use crate::ser::tags::Id;
use crate::ser::tags::Tag;


/// A literal that can be serialized and deserialized.
#[derive(Copy, Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TagLit {
  Pos(Tag),
  Neg(Tag),
}

impl Debug for TagLit {
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    match self {
      Self::Pos(tag) => write!(f, "{tag}"),
      Self::Neg(tag) => write!(f, "!{tag}"),
    }
  }
}

impl TagLit {
  /// Retrieve the ID of the wrapped tag.
  pub fn id(&self) -> Id {
    match self {
      TagLit::Pos(tag) | TagLit::Neg(tag) => tag.id,
    }
  }
}


/// A view that can be serialized and deserialized.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct View {
  pub name: String,
  pub lits: Box<[Box<[TagLit]>]>,
}


#[cfg(test)]
mod tests {
  use super::*;

  use crate::ser::backends::Backend;
  use crate::ser::backends::Json;

  use crate::ser::id::Id;


  fn tag(tag: usize) -> Tag {
    Tag {
      id: Id::try_from(tag).unwrap(),
    }
  }


  /// Check that we can serialize and deserialize a `View`.
  #[test]
  fn serialize_deserialize_view() {
    let view = View {
      name: "test-view".to_string(),
      lits: Box::new([
        Box::new([TagLit::Pos(tag(1))]),
        Box::new([TagLit::Pos(tag(2)), TagLit::Neg(tag(3))]),
        Box::new([TagLit::Neg(tag(4)), TagLit::Pos(tag(2))]),
      ]),
    };

    let serialized = Json::serialize(&view).unwrap();
    let deserialized = <Json as Backend<View>>::deserialize(&serialized).unwrap();

    assert_eq!(deserialized, view);
  }
}
