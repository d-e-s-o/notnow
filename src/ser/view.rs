// Copyright (C) 2018-2023 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

//! A module providing serialization and deserialization support for
//! task views.

use serde::Deserialize;
use serde::Serialize;

use crate::ser::tags::Id;
use crate::ser::tags::Tag;


/// A literal that can be serialized and deserialized.
#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TagLit {
  Pos(Tag),
  Neg(Tag),
}

impl TagLit {
  /// Retrieve the ID of the wrapped tag.
  pub fn id(&self) -> Id {
    match self {
      TagLit::Pos(tag) | TagLit::Neg(tag) => tag.id,
    }
  }
}


#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct ViewImpl {
  name: String,
  lits: Vec<Vec<TagLit>>,
}


// This type exists solely for the purpose of preserving backwards
// compatibility with respect to the configuration format used, and
// where earlier versions of the program stored a `View` together with
// the index of the selected task.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
enum ViewTuple {
  Tuple((ViewImpl, Option<usize>)),
  View(ViewImpl),
}

impl From<ViewTuple> for View {
  fn from(other: ViewTuple) -> Self {
    match other {
      ViewTuple::View(view) | ViewTuple::Tuple((view, ..)) => {
        let ViewImpl { name, lits } = view;
        View { name, lits }
      },
    }
  }
}


/// A view that can be serialized and deserialized.
// TODO: Remove the `from` conversion with the next compatibility
//       breaking release.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(from = "ViewTuple")]
pub struct View {
  pub name: String,
  pub lits: Vec<Vec<TagLit>>,
}


#[cfg(test)]
mod tests {
  use super::*;

  use crate::ser::backends::Backend;
  use crate::ser::backends::Json;

  use crate::ser::id::Id;


  /// Check that we can serialize and deserialize a `View`.
  #[test]
  fn serialize_deserialize_view() {
    let tag1 = Tag {
      id: Id::try_from(1).unwrap(),
    };
    let tag2 = Tag {
      id: Id::try_from(2).unwrap(),
    };
    let tag3 = Tag {
      id: Id::try_from(3).unwrap(),
    };
    let tag4 = Tag {
      id: Id::try_from(4).unwrap(),
    };

    let view = View {
      name: "test-view".to_string(),
      lits: vec![
        vec![TagLit::Pos(tag1)],
        vec![TagLit::Pos(tag2), TagLit::Neg(tag3)],
        vec![TagLit::Neg(tag4), TagLit::Pos(tag2)],
      ],
    };

    let serialized = Json::serialize(&view).unwrap();
    let deserialized = <Json as Backend<View>>::deserialize(&serialized).unwrap();

    assert_eq!(deserialized, view);
  }
}
