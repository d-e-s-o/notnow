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

use crate::ser::id::Id as IdT;


#[derive(Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct T(());

pub type Id = IdT<T>;


/// A struct for serializing the concept of a tag.
///
/// Objects of this type are used to describe what a tag looks like and
/// are the form in which the concept of a particular tag is persisted.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Template {
  pub id: Id,
  pub name: String,
}


/// A serializable tag instance.
#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Tag {
  pub id: Id,
}


/// A serializable struct comprising a list of tag templates.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Templates(pub Vec<Template>);


#[cfg(test)]
mod tests {
  use super::*;

  use serde_json::from_str as from_json;
  use serde_json::to_string as to_json;


  #[test]
  fn serialize_deserialize_template() {
    let template = Template {
      id: Id::new(32),
      name: "test-tag".to_string(),
    };
    let serialized = to_json(&template).unwrap();
    let deserialized = from_json::<Template>(&serialized).unwrap();

    assert_eq!(deserialized, template);
  }

  #[test]
  fn serialize_deserialize_tag() {
    let tag = Tag {
      id: Id::new(42),
    };
    let serialized = to_json(&tag).unwrap();
    let deserialized = from_json::<Tag>(&serialized).unwrap();

    assert_eq!(deserialized, tag);
  }

  #[test]
  fn serialize_deserialize_templates() {
    let templates = vec![
      Template {
        id: Id::new(3),
        name: "tag1".to_string(),
      },
      Template {
        id: Id::new(990),
        name: "tag990".to_string(),
      },
    ];
    let templates = Templates(templates);
    let serialized = to_json(&templates).unwrap();
    let deserialized = from_json::<Templates>(&serialized).unwrap();

    assert_eq!(deserialized, templates);
  }
}
